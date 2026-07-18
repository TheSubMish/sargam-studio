//! Generates Kotlin/Swift bindings from the compiled library's embedded UniFFI
//! metadata. Usage (after building the cdylib for the target platform):
//!   cargo run -p sargam-mobile --bin uniffi-bindgen -- generate \
//!     --library target/.../libsargam_mobile.so --language kotlin --out-dir bindings/kotlin

fn main() {
    uniffi::uniffi_bindgen_main()
}
