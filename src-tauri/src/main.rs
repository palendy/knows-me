//! Placeholder binary entry point.
//!
//! U1 Milestone 0 ships the shared contract library (`knows_me_core`). The real
//! Tauri application (windows, commands, service wiring) is added in U1 full
//! implementation. This entry point exists so the crate builds and runs.

fn main() {
    println!(
        "knows-me core — U1 Milestone 0: shared contracts + mocks are ready.\n\
         Run `cargo test` to exercise the contract mocks.\n\
         The Tauri application is wired up in U1 full implementation."
    );
}
