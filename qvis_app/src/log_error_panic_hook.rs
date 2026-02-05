use log::error;
use std::panic::{self, PanicHookInfo};
use std::sync::Once;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    type Error;

    #[wasm_bindgen(constructor)]
    fn new() -> Error;

    #[wasm_bindgen(structural, method, getter)]
    fn stack(error: &Error) -> String;
}

fn hook_impl(info: &PanicHookInfo) {
    error!("{info:?}\n\n{}", Error::new().stack());
}

pub fn set_once() {
    static SET_HOOK: Once = Once::new();
    SET_HOOK.call_once(|| {
        panic::set_hook(Box::new(hook_impl));
    });
}
