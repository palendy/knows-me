//! Try the Confluence / Jira connectors against a real server from the
//! terminal — the same connect-time verify the app runs, then one sync pass,
//! then a second pass from the returned cursor (which should collect nothing
//! new). Nothing is stored; items are printed.
//!
//! ```bash
//! cd src-tauri
//! CONFLUENCE_BASE_URL=https://confluence.example.com CONFLUENCE_PAT=... \
//! JIRA_BASE_URL=https://jira.example.com JIRA_PAT=... \
//!   cargo run --features atlassian-http --example atlassian_probe
//! ```
//!
//! Set only the pair you want to probe; `CONFLUENCE_WEB_BASE_URL` is optional.
//! `ATLASSIAN_PRINT_CHARS` (default 400) caps how much of each item is shown.
//! Reads `.env` like the other examples.

#[cfg(not(feature = "atlassian-http"))]
fn main() {
    eprintln!("build with: cargo run --features atlassian-http --example atlassian_probe");
    std::process::exit(2);
}

#[cfg(feature = "atlassian-http")]
#[tokio::main]
async fn main() {
    use knows_me_core::core::traits::{NoProgress, ProgressReporter};
    use knows_me_core::core::types::{Cursor, RawItem, SourceKind};
    use knows_me_core::ingestion::connectors::atlassian::{normalize_base_url, AtlassianCreds};
    use knows_me_core::ingestion::connectors::{confluence, jira};
    use std::time::Instant;

    dotenvy::dotenv().ok();
    let env = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let print_chars: usize = env("ATLASSIAN_PRINT_CHARS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(400);

    struct Stderr;
    impl ProgressReporter for Stderr {
        fn progress(&self, source: SourceKind, done: usize, total: usize) {
            eprintln!("  [{source:?}] {done}/{total}");
        }
    }
    let _ = NoProgress;

    fn show(items: &[RawItem], print_chars: usize) {
        for it in items {
            let text = it.text.as_deref().unwrap_or("");
            let head: String = text.chars().take(print_chars).collect();
            println!(
                "--- {} · {} · {} chars\n{}{}\n",
                it.external_id,
                it.collected_at.to_rfc3339(),
                text.chars().count(),
                head,
                if text.chars().count() > print_chars { "…" } else { "" }
            );
        }
    }

    let mut probed = false;

    if let (Some(base), Some(pat)) = (env("CONFLUENCE_BASE_URL"), env("CONFLUENCE_PAT")) {
        probed = true;
        let creds = AtlassianCreds {
            base_url: normalize_base_url(&base),
            pat,
            web_base_url: env("CONFLUENCE_WEB_BASE_URL").map(|u| normalize_base_url(&u)),
        };
        println!("== Confluence {}", creds.base_url);
        let t = Instant::now();
        match confluence::http::verify(&creds).await {
            Ok((who, count)) => println!("verify ({:.1?}): {who} · my pages = {count:?}", t.elapsed()),
            Err(e) => {
                println!("verify FAILED: {e}");
                std::process::exit(1);
            }
        }
        let t = Instant::now();
        match confluence::http::sync(&creds, None, &Stderr).await {
            Ok((items, cursor)) => {
                println!("sync #1 ({:.1?}): {} item(s), cursor = {:?}", t.elapsed(), items.len(), cursor.0);
                show(&items, print_chars);
                let t = Instant::now();
                match confluence::http::sync(&creds, Some(Cursor(cursor.0.clone())), &Stderr).await {
                    Ok((again, c2)) => println!(
                        "sync #2 from cursor ({:.1?}): {} item(s) (0 expected unless the cap was hit), cursor = {:?}",
                        t.elapsed(),
                        again.len(),
                        c2.0
                    ),
                    Err(e) => println!("sync #2 FAILED: {e}"),
                }
            }
            Err(e) => println!("sync FAILED: {e}"),
        }
    }

    if let (Some(base), Some(pat)) = (env("JIRA_BASE_URL"), env("JIRA_PAT")) {
        probed = true;
        let creds = AtlassianCreds {
            base_url: normalize_base_url(&base),
            pat,
            web_base_url: None,
        };
        println!("== Jira {}", creds.base_url);
        let t = Instant::now();
        match jira::http::verify(&creds).await {
            Ok((who, count)) => println!("verify ({:.1?}): {who} · my issues = {count:?}", t.elapsed()),
            Err(e) => {
                println!("verify FAILED: {e}");
                std::process::exit(1);
            }
        }
        let t = Instant::now();
        match jira::http::sync(&creds, None, &Stderr).await {
            Ok((items, cursor)) => {
                println!("sync #1 ({:.1?}): {} item(s), cursor = {:?}", t.elapsed(), items.len(), cursor.0);
                show(&items, print_chars);
                let t = Instant::now();
                match jira::http::sync(&creds, Some(Cursor(cursor.0.clone())), &Stderr).await {
                    Ok((again, c2)) => println!(
                        "sync #2 from cursor ({:.1?}): {} item(s) (0 expected unless the cap was hit), cursor = {:?}",
                        t.elapsed(),
                        again.len(),
                        c2.0
                    ),
                    Err(e) => println!("sync #2 FAILED: {e}"),
                }
            }
            Err(e) => println!("sync FAILED: {e}"),
        }
    }

    if !probed {
        eprintln!("set CONFLUENCE_BASE_URL + CONFLUENCE_PAT and/or JIRA_BASE_URL + JIRA_PAT");
        std::process::exit(2);
    }
}
