// Prevents an extra console window on Windows in release builds. Do not remove.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Crane runs itself again as the helper for containers; that run ends here.
    #[cfg(target_os = "linux")]
    crane_box::helper::run_if_helper();
    crane_lib::run();
}
