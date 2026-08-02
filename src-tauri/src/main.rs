// Prevent a console window from opening alongside the app on Windows release builds
// (no-op on Linux; kept for portability if other targets are added).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    nextwist_lib::run();
}
