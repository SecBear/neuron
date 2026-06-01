//! Proc-macro for deriving `SyncOperator` from annotated async functions.
//!
//! # Usage
//!
//! ```rust,ignore
//! #[skg_tool(name = "get_weather", description = "Get weather for a location")]
//! async fn get_weather(location: String, units: Option<String>) -> Result<serde_json::Value, ToolError> {
//!     // ...
//! }
//! ```
//!
//! This generates a `GetWeatherTool` struct implementing `skg_context_engine::SyncOperator`.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{ItemFn, LitStr, Type, parse_macro_input};

/// Parsed arguments from `#[skg_tool(...)]`.
struct MacroArgs {
    name: String,
    description: String,
    concurrent: bool,
}

impl syn::parse::Parse for MacroArgs {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut name: Option<String> = None;
        let mut description: Option<String> = None;
        let mut concurrent = false;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            match ident.to_string().as_str() {
                "name" => {
                    input.parse::<syn::Token![=]>()?;
                    let lit: LitStr = input.parse()?;
                    name = Some(lit.value());
                }
                "description" => {
                    input.parse::<syn::Token![=]>()?;
                    let lit: LitStr = input.parse()?;
                    description = Some(lit.value());
                }
                "concurrent" => {
                    concurrent = true;
                }
                other => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!("unknown attribute key: `{other}`"),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<syn::Token![,]>()?;
            }
        }

        Ok(MacroArgs {
            name: name.ok_or_else(|| {
                syn::Error::new(Span::call_site(), "missing required attribute `name`")
            })?,
            description: description.ok_or_else(|| {
                syn::Error::new(
                    Span::call_site(),
                    "missing required attribute `description`",
                )
            })?,
            concurrent,
        })
    }
}

/// Convert a `snake_case` identifier string to `PascalCase`.
fn snake_to_pascal(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

/// Return `true` if `ty` is `&DispatchContext` (any qualifying path ending in `DispatchContext`).
fn is_dispatch_context_ref(ty: &Type) -> bool {
    if let Type::Reference(r) = ty
        && let Type::Path(tp) = r.elem.as_ref()
        && let Some(last) = tp.path.segments.last()
    {
        return last.ident == "DispatchContext";
    }
    false
}

/// Per-parameter metadata extracted from the function signature.
struct ParamInfo {
    ident: syn::Ident,
    ty: Type,
    /// True if the parameter is `&DispatchContext` (not included in schema, passed through).
    is_ctx: bool,
}

/// Derive a `SyncOperator` implementation from an annotated async function.
///
/// # Attributes
///
/// - `name = "..."` — tool name returned via `CapabilityDescriptor`
/// - `description = "..."` — tool description in the descriptor
/// - `concurrent` — if present, `ExecutionClass::Shared`; otherwise `ExecutionClass::Exclusive`
///
/// # Generated output
///
/// - The original `async fn` is kept intact.
/// - A `pub struct <PascalCase>Tool` struct is generated.
/// - `impl skg_context_engine::SyncOperator for <PascalCase>Tool` is generated.
/// - A `fn new() -> Self` constructor is generated.
///
/// # Parameter handling
///
/// - Parameters of type `&DispatchContext` are excluded from JSON deserialization and
///   passed through to the underlying function via the `execute()` context argument.
/// - `Option<T>` parameters deserialise from JSON when present; `None` when absent.
/// - All other parameters are required in the JSON input.
#[proc_macro_attribute]
pub fn skg_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as MacroArgs);
    let func = parse_macro_input!(item as ItemFn);

    let fn_name = &func.sig.ident;
    let fn_name_str = fn_name.to_string();
    let struct_name_str = format!("{}Tool", snake_to_pascal(&fn_name_str));
    let struct_ident = syn::Ident::new(&struct_name_str, fn_name.span());

    let tool_name = &args.name;
    let tool_desc = &args.description;

    let execution_class = if args.concurrent {
        quote! { ::layer0::capability::ExecutionClass::Shared }
    } else {
        quote! { ::layer0::capability::ExecutionClass::Exclusive }
    };

    // Collect parameter info from the function signature
    let mut params: Vec<ParamInfo> = Vec::new();
    for arg in &func.sig.inputs {
        match arg {
            syn::FnArg::Receiver(_) => {
                // Skip `self` — tool functions should not take self
            }
            syn::FnArg::Typed(pat_ty) => {
                let ident = match pat_ty.pat.as_ref() {
                    syn::Pat::Ident(pi) => pi.ident.clone(),
                    _ => {
                        return syn::Error::new_spanned(
                            &pat_ty.pat,
                            "#[skg_tool]: only simple identifier patterns are supported in parameters",
                        )
                        .into_compile_error()
                        .into();
                    }
                };
                let ty = *pat_ty.ty.clone();
                let is_ctx = is_dispatch_context_ref(&ty);
                params.push(ParamInfo { ident, ty, is_ctx });
            }
        }
    }

    // execute() body: determine whether ctx parameter is used
    let has_ctx = params.iter().any(|p| p.is_ctx);

    // Bind name for the `ctx` parameter in the generated `execute()` method:
    // prefix with `_` when unused to silence dead-code warnings.
    let ctx_param_name: proc_macro2::TokenStream = if has_ctx {
        quote! { ctx }
    } else {
        quote! { _ctx }
    };

    // When the original function takes `&DispatchContext`, the `execute()` implementation
    // must clone `ctx` into an owned value before the `async` block.
    let ctx_clone_stmt: proc_macro2::TokenStream = if has_ctx {
        quote! { let __skg_ctx = ctx.clone(); }
    } else {
        quote! {}
    };
    let ctx_reborrow_stmt: proc_macro2::TokenStream = if has_ctx {
        quote! { let ctx = &__skg_ctx; }
    } else {
        quote! {}
    };

    // Deserialise each non-ctx parameter from the JSON input
    let param_deserializations: Vec<proc_macro2::TokenStream> = params
        .iter()
        .filter(|p| !p.is_ctx)
        .map(|p| {
            let name = &p.ident;
            let name_str = name.to_string();
            let ty = &p.ty;
            quote! {
                let #name: #ty = ::serde_json::from_value(
                    input.get(#name_str).cloned().unwrap_or(::serde_json::Value::Null)
                )
                .map_err(|e| ::layer0::error::ProtocolError::new(
                    ::layer0::error::ErrorCode::InvalidInput,
                    format!("parameter '{}': {}", #name_str, e),
                    false,
                ))?;
            }
        })
        .collect();

    // Arguments forwarded to the original function
    let call_args: Vec<proc_macro2::TokenStream> = params
        .iter()
        .map(|p| {
            if p.is_ctx {
                quote! { ctx }
            } else {
                let name = &p.ident;
                quote! { #name }
            }
        })
        .collect();

    let expanded = quote! {
        #func

        /// Generated tool struct from `#[skg_tool]`.
        pub struct #struct_ident;

        impl #struct_ident {
            /// Create a new instance of this tool.
            pub fn new() -> Self {
                Self
            }
        }

        impl ::std::default::Default for #struct_ident {
            fn default() -> Self {
                Self::new()
            }
        }

        #[::async_trait::async_trait]
        impl ::skg_context_engine::SyncOperator for #struct_ident {
            fn descriptor(&self) -> ::layer0::capability::CapabilityDescriptor {
                ::layer0::capability::CapabilityDescriptor::new(
                    ::layer0::capability::CapabilityId::new(#tool_name),
                    ::layer0::capability::CapabilityKind::Tool,
                    #tool_name,
                    #tool_desc,
                    ::layer0::capability::SchedulingFacts::new(
                        #execution_class,
                        false, false, false, None,
                    ),
                    ::layer0::capability::ApprovalFacts::None,
                    ::layer0::capability::AuthFacts::Open,
                )
            }

            async fn execute(
                &self,
                input: ::layer0::operator::OperatorInput,
                #ctx_param_name: &::layer0::dispatch_context::DispatchContext,
            ) -> ::std::result::Result<::layer0::operator::OperatorOutput, ::layer0::error::ProtocolError> {
                let __skg_input_text = input.message.as_text().unwrap_or("null");
                let __skg_input: ::serde_json::Value = ::serde_json::from_str(__skg_input_text)
                    .map_err(|e| ::layer0::error::ProtocolError::new(
                        ::layer0::error::ErrorCode::InvalidInput,
                        format!("invalid input JSON: {e}"),
                        false,
                    ))?;
                let input = __skg_input;
                #ctx_clone_stmt
                #ctx_reborrow_stmt
                #(#param_deserializations)*
                let result = #fn_name(#(#call_args),*).await
                    .map_err(|e| ::layer0::error::ProtocolError::internal(e.to_string()))?;
                Ok(::layer0::operator::OperatorOutput::new(
                    ::layer0::content::Content::text(result.to_string()),
                    ::layer0::operator::Outcome::Terminal {
                        terminal: ::layer0::operator::TerminalOutcome::Completed,
                    },
                ))
            }
        }
    };

    expanded.into()
}
