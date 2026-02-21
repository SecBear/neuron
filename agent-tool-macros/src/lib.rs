//! Proc-macro crate for deriving Tool implementations.
//!
//! Provides the `#[agent_tool]` attribute macro.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, Pat, ReturnType, Type};

/// Derive a Tool implementation from an async function.
///
/// # Example
///
/// ```ignore
/// #[agent_tool(name = "calculate", description = "Evaluate a math expression")]
/// async fn calculate(
///     /// A mathematical expression like "2 + 2"
///     expression: String,
///     _ctx: &ToolContext,
/// ) -> Result<CalculateOutput, CalculateError> {
///     let result = eval(&expression);
///     Ok(CalculateOutput { result })
/// }
/// ```
#[proc_macro_attribute]
pub fn agent_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(attr as AgentToolArgs);
    let func = syn::parse_macro_input!(item as ItemFn);

    match expand_agent_tool(args, func) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

// Parse the attribute args: name = "...", description = "..."
struct AgentToolArgs {
    name: String,
    description: String,
}

impl syn::parse::Parse for AgentToolArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut description = None;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            let _: syn::Token![=] = input.parse()?;
            let value: syn::LitStr = input.parse()?;

            match ident.to_string().as_str() {
                "name" => name = Some(value.value()),
                "description" => description = Some(value.value()),
                other => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!("unknown attribute: {other}"),
                    ));
                }
            }

            if !input.is_empty() {
                let _: syn::Token![,] = input.parse()?;
            }
        }

        Ok(AgentToolArgs {
            name: name.ok_or_else(|| input.error("missing `name` attribute"))?,
            description: description.ok_or_else(|| input.error("missing `description` attribute"))?,
        })
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + &chars.collect::<String>(),
            }
        })
        .collect()
}

fn expand_agent_tool(
    args: AgentToolArgs,
    func: ItemFn,
) -> syn::Result<proc_macro2::TokenStream> {
    let func_name = &func.sig.ident;
    let vis = &func.vis;
    let pascal = to_pascal_case(&func_name.to_string());
    let tool_struct = format_ident!("{}Tool", pascal);
    let args_struct = format_ident!("{}Args", pascal);

    let tool_name = &args.name;
    let tool_description = &args.description;

    // Extract parameters (skip last one which is &ToolContext)
    let params: Vec<_> = func.sig.inputs.iter().collect();
    if params.is_empty() {
        return Err(syn::Error::new_spanned(
            &func.sig,
            "function must have at least a ctx parameter",
        ));
    }

    let tool_params = &params[..params.len() - 1]; // All except last (ctx)

    // Build Args struct fields
    let mut field_names = Vec::new();
    let mut field_types = Vec::new();
    let mut field_docs = Vec::new();

    for param in tool_params {
        match param {
            FnArg::Typed(pat_type) => {
                let name = match pat_type.pat.as_ref() {
                    Pat::Ident(ident) => &ident.ident,
                    _ => {
                        return Err(syn::Error::new_spanned(
                            pat_type,
                            "expected identifier pattern",
                        ));
                    }
                };
                let ty = &pat_type.ty;

                // Extract doc comments from attributes
                let docs: Vec<_> = pat_type
                    .attrs
                    .iter()
                    .filter(|a| a.path().is_ident("doc"))
                    .cloned()
                    .collect();

                field_names.push(name.clone());
                field_types.push(ty.clone());
                field_docs.push(docs);
            }
            FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    param,
                    "self parameter not supported",
                ));
            }
        }
    }

    // Extract return type: Result<Output, Error>
    let (output_type, error_type) = match &func.sig.output {
        ReturnType::Type(_, ty) => extract_result_types(ty)?,
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                &func.sig,
                "function must return Result<Output, Error>",
            ));
        }
    };

    // Get the function body
    let body = &func.block;

    // Build the field definitions with doc comments
    let field_defs: Vec<_> = field_names
        .iter()
        .zip(field_types.iter())
        .zip(field_docs.iter())
        .map(|((name, ty), docs)| {
            quote! {
                #(#docs)*
                pub #name: #ty
            }
        })
        .collect();

    // Build the destructuring pattern
    let destructure_fields: Vec<_> = field_names.iter().map(|name| quote! { #name }).collect();

    Ok(quote! {
        /// Auto-generated args struct for the tool.
        #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
        #vis struct #args_struct {
            #(#field_defs,)*
        }

        /// Auto-generated tool struct.
        #vis struct #tool_struct;

        impl agent_types::Tool for #tool_struct {
            const NAME: &'static str = #tool_name;
            type Args = #args_struct;
            type Output = #output_type;
            type Error = #error_type;

            fn definition(&self) -> agent_types::ToolDefinition {
                agent_types::ToolDefinition {
                    name: Self::NAME.into(),
                    title: None,
                    description: #tool_description.into(),
                    input_schema: serde_json::to_value(
                        schemars::schema_for!(#args_struct)
                    ).unwrap(),
                    output_schema: None,
                    annotations: None,
                    cache_control: None,
                }
            }

            async fn call(
                &self,
                args: Self::Args,
                ctx: &agent_types::ToolContext,
            ) -> Result<Self::Output, Self::Error> {
                let #args_struct { #(#destructure_fields,)* } = args;
                // Suppress unused variable warning for ctx when it's used as _ctx
                let _ = &ctx;
                #body
            }
        }
    })
}

fn extract_result_types(ty: &Type) -> syn::Result<(Box<Type>, Box<Type>)> {
    if let Type::Path(type_path) = ty {
        let last_segment = type_path
            .path
            .segments
            .last()
            .ok_or_else(|| syn::Error::new_spanned(ty, "expected Result type"))?;

        if last_segment.ident != "Result" {
            return Err(syn::Error::new_spanned(
                ty,
                "return type must be Result<Output, Error>",
            ));
        }

        if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
            let mut types = args.args.iter().filter_map(|arg| {
                if let syn::GenericArgument::Type(t) = arg {
                    Some(t.clone())
                } else {
                    None
                }
            });

            let output = types
                .next()
                .ok_or_else(|| syn::Error::new_spanned(ty, "Result must have Output type"))?;
            let error = types
                .next()
                .ok_or_else(|| syn::Error::new_spanned(ty, "Result must have Error type"))?;

            return Ok((Box::new(output), Box::new(error)));
        }
    }

    Err(syn::Error::new_spanned(
        ty,
        "return type must be Result<Output, Error>",
    ))
}
