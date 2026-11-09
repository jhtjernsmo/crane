//! The helper as a program of its own, for tests and the CLI. The app runs
//! itself as the helper instead.

fn main() {
    crane_box::helper::run_if_helper();
    eprintln!("Run this through Crane: it is the helper for containers.");
    std::process::exit(2);
}
