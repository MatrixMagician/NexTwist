// Prevent a console window from opening alongside the app on Windows release builds
// (no-op on Linux; kept for portability when Phase 5 adds other targets).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    nextwist_lib::run();
}
