//! Headless demo of U1 (Core Platform & Security), end-to-end, no GUI.
//!
//! Exercises the real onboarding → unlock → encrypted store → masking flow so
//! U1 can be demonstrated (and screenshotted) from a terminal:
//!
//! ```bash
//! cargo run
//! ```

use knows_me_core::core::commands;
use knows_me_core::core::traits::Masker;
use knows_me_core::core::types::{Credential, SourceKind, TransferPolicy};
use knows_me_core::AppState;

#[tokio::main]
async fn main() {
    println!("knows-me — U1 (Core Platform & Security) demo\n");

    // Fresh scratch dir so the demo is repeatable.
    let dir = std::env::temp_dir().join(format!("knows-me-demo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let state = AppState::new(&dir);

    let s = commands::status(&state);
    println!(
        "1. First run — initialized={}, unlocked={}",
        s.initialized, s.unlocked
    );

    println!("2. Setting password (Argon2id KDF → 256-bit key, encryption initialized)…");
    commands::setup_password(&state, "correct horse battery staple")
        .await
        .expect("setup");
    let s = commands::status(&state);
    println!(
        "   → initialized={}, unlocked={}",
        s.initialized, s.unlocked
    );

    println!("3. Storing a Gmail credential (encrypted at rest, AES-256-GCM)…");
    let cred = Credential(serde_json::json!({ "oauth_token": "ya29.SECRET-TOKEN" }));
    commands::store_credential(&state, SourceKind::Gmail, cred)
        .await
        .expect("store credential");
    let loaded = commands::load_credential(&state, SourceKind::Gmail)
        .await
        .expect("load")
        .expect("present");
    println!("   → decrypted back in memory: {}", loaded.0);

    println!("4. Masking gateway (identifiers removed before any cloud call)…");
    let masker = state.masker();
    let sample = "ping jane.doe@corp.com and set key=sk-abcdef0123456789ABCD";
    let (masked, map) = masker.mask(sample);
    println!("   raw    : {sample}");
    println!("   masked : {}", masked.text);
    let restored = masker.unmask(&masked, &map);
    println!(
        "   restore: {restored}  (round-trip ok: {})",
        restored == sample
    );

    println!(
        "5. Transfer policy = {:?} (default: mask & minimize)",
        state.transfer_policy()
    );
    commands::set_transfer_policy(&state, TransferPolicy::LocalOnlyNoLlm)
        .await
        .expect("policy");
    println!("   → set to {:?}", state.transfer_policy());

    println!("6. Lock, then unlock with wrong vs. right password…");
    commands::lock(&state);
    let wrong = commands::unlock(&state, "nope").await;
    println!(
        "   wrong password → {}",
        if wrong.is_err() {
            "rejected ✓"
        } else {
            "ACCEPTED (bug!)"
        }
    );
    commands::unlock(&state, "correct horse battery staple")
        .await
        .expect("unlock");
    println!(
        "   right password → unlocked ✓; policy restored = {:?}",
        state.transfer_policy()
    );

    let _ = std::fs::remove_dir_all(&dir);
    println!("\nAll U1 steps completed. Run `cargo test` for the full unit + property suite.");
}
