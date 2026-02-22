//! WASM compatibility shims.
//!
//! On native targets, these are aliases for Send/Sync.
//! On wasm32, the bounds are removed since wasm32 is single-threaded.

use std::future::Future;
use std::pin::Pin;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;

    /// Marker trait equivalent to `Send` on native, unconditional on WASM.
    pub trait WasmCompatSend: Send {}
    impl<T: Send> WasmCompatSend for T {}

    /// Marker trait equivalent to `Sync` on native, unconditional on WASM.
    pub trait WasmCompatSync: Sync {}
    impl<T: Sync> WasmCompatSync for T {}

    /// A boxed future that is `Send` on native and unbound on WASM.
    pub type WasmBoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
}

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::*;

    /// Marker trait equivalent to `Send` on native, unconditional on WASM.
    pub trait WasmCompatSend {}
    impl<T> WasmCompatSend for T {}

    /// Marker trait equivalent to `Sync` on native, unconditional on WASM.
    pub trait WasmCompatSync {}
    impl<T> WasmCompatSync for T {}

    /// A boxed future that is `Send` on native and unbound on WASM.
    pub type WasmBoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_wasm_compat_send<T: WasmCompatSend>() {}
    fn assert_wasm_compat_sync<T: WasmCompatSync>() {}

    #[test]
    fn string_is_wasm_compat_send() {
        assert_wasm_compat_send::<String>();
    }

    #[test]
    fn string_is_wasm_compat_sync() {
        assert_wasm_compat_sync::<String>();
    }

    #[test]
    fn boxed_future_type_alias_compiles() {
        let _fut: WasmBoxedFuture<'_, i32> = Box::pin(async { 42 });
    }
}
