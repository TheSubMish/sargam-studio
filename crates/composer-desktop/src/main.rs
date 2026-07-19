//! Desktop entry point — the actual app is `sargam_desktop_lib::run()` so it
//! can be shared with the Android/iOS builds (see `lib.rs`), which need a
//! library crate target instead of a binary.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sargam_desktop_lib::run();
}
