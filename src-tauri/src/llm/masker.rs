//! Regex-based [`Masker`]: strips identifiers/secrets before cloud transmission
//! and restores them locally (US-2.2, FR-2.3, R1).
//!
//! Guarantees, verified by property tests below:
//! - **Round-trip** (PBT-02): `unmask(mask(x)) == x` for all inputs.
//! - **Invariant / completeness** (PBT-03): re-scanning the masked output finds
//!   no identifier the patterns recognize — nothing sensitive is left behind.
//!
//! Placeholders use guillemets (`«KIND_n»`) so they cannot collide with normal
//! text or be re-matched by the patterns.

use regex::Regex;

use crate::core::traits::Masker;
use crate::core::types::{MaskedText, UnmaskMap};

/// A recognized-identifier pattern and its placeholder label.
struct Pattern {
    label: &'static str,
    re: Regex,
}

/// Masks emails, secrets/tokens, URLs and phone numbers.
pub struct RegexMasker {
    patterns: Vec<Pattern>,
}

impl Default for RegexMasker {
    fn default() -> Self {
        Self::new()
    }
}

impl RegexMasker {
    pub fn new() -> Self {
        // Order matters only for tie-breaking overlaps; disjoint ranges win.
        let specs: &[(&str, &str)] = &[
            // API keys / tokens first (most specific).
            (
                "SECRET",
                r"(?:sk-[A-Za-z0-9]{16,}|ghp_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9-]{10,})",
            ),
            ("EMAIL", r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}"),
            ("URL", r"https?://[^\s<>()«»]+"),
            // Phone-ish: a run of digits/separators with at least ~9 digits.
            ("PHONE", r"\+?\d(?:[\d ().\-]{7,})\d"),
        ];
        let patterns = specs
            .iter()
            .map(|(label, re)| Pattern {
                label,
                re: Regex::new(re).expect("static masker regex must compile"),
            })
            .collect();
        Self { patterns }
    }

    /// All non-overlapping match ranges, sorted by start (longest wins on ties).
    fn find_matches(&self, text: &str) -> Vec<(usize, usize, &'static str)> {
        let mut raw: Vec<(usize, usize, &'static str)> = Vec::new();
        for p in &self.patterns {
            for m in p.re.find_iter(text) {
                raw.push((m.start(), m.end(), p.label));
            }
        }
        // Earliest start first; for equal starts, longer span first.
        raw.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));

        let mut chosen: Vec<(usize, usize, &'static str)> = Vec::new();
        let mut last_end = 0usize;
        for (start, end, label) in raw {
            if start >= last_end {
                chosen.push((start, end, label));
                last_end = end;
            }
        }
        chosen
    }
}

impl RegexMasker {
    /// One masking pass over `text`, appending any newly-assigned placeholders to
    /// `map`. Returns the rewritten string and whether anything was replaced.
    fn mask_pass(
        &self,
        text: &str,
        map: &mut UnmaskMap,
        counters: &mut std::collections::HashMap<&'static str, usize>,
        assigned: &mut std::collections::HashMap<String, String>,
    ) -> (String, bool) {
        let matches = self.find_matches(text);
        if matches.is_empty() {
            return (text.to_string(), false);
        }
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0usize;
        for (start, end, label) in matches {
            out.push_str(&text[cursor..start]);
            let original = &text[start..end];
            let placeholder = if let Some(ph) = assigned.get(original) {
                ph.clone()
            } else {
                let n = counters.entry(label).or_insert(0);
                *n += 1;
                let ph = format!("«{label}_{n}»");
                assigned.insert(original.to_string(), ph.clone());
                map.insert(ph.clone(), original.to_string());
                ph
            };
            out.push_str(&placeholder);
            cursor = end;
        }
        out.push_str(&text[cursor..]);
        (out, true)
    }
}

impl Masker for RegexMasker {
    fn mask(&self, text: &str) -> (MaskedText, UnmaskMap) {
        let mut map = UnmaskMap::new();
        let mut counters: std::collections::HashMap<&'static str, usize> = Default::default();
        let mut assigned: std::collections::HashMap<String, String> = Default::default();

        // Iterate to a fixed point: a greedy match can overlap a higher-priority
        // one and be dropped in dedup, exposing a residual identifier at the
        // boundary. Placeholders can never re-match, so this always terminates.
        let mut current = text.to_string();
        loop {
            let (next, changed) = self.mask_pass(&current, &mut map, &mut counters, &mut assigned);
            current = next;
            if !changed {
                break;
            }
        }
        (MaskedText { text: current }, map)
    }

    fn unmask(&self, masked: &MaskedText, map: &UnmaskMap) -> String {
        let mut text = masked.text.clone();
        for (placeholder, original) in map.entries() {
            text = text.replace(placeholder, original);
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn masks_email_and_restores() {
        let m = RegexMasker::new();
        let (masked, map) = m.mask("email me at jane.doe@example.com please");
        assert!(!masked.text.contains("jane.doe@example.com"));
        assert!(masked.text.contains("«EMAIL_1»"));
        assert_eq!(
            m.unmask(&masked, &map),
            "email me at jane.doe@example.com please"
        );
    }

    #[test]
    fn masks_api_token() {
        let m = RegexMasker::new();
        let (masked, _) = m.mask("key=sk-abcdefghijklmnop1234 done");
        assert!(!masked.text.contains("sk-abcdefghijklmnop1234"));
        assert!(masked.text.contains("«SECRET_1»"));
    }

    #[test]
    fn repeated_identifier_shares_one_placeholder() {
        let m = RegexMasker::new();
        let (masked, map) = m.mask("a@b.com and again a@b.com");
        assert_eq!(masked.text.matches("«EMAIL_1»").count(), 2);
        assert_eq!(map.entries().len(), 1);
    }

    #[test]
    fn empty_input() {
        let m = RegexMasker::new();
        let (masked, map) = m.mask("");
        assert_eq!(masked.text, "");
        assert!(map.is_empty());
    }

    // --- Property-based tests (PBT-02 round-trip, PBT-03 invariant) ------------
    //
    // PBT-07 generator: realistic prose interleaved with real identifiers,
    // never containing the guillemet sentinels used by placeholders.
    // proptest supplies shrinking + seed reproducibility (PBT-08); the framework
    // itself is the PBT-09 selection for Rust.

    fn word() -> impl Strategy<Value = String> {
        "[a-z]{1,8}".prop_map(|s| s)
    }

    fn email() -> impl Strategy<Value = String> {
        ("[a-z]{1,6}", "[a-z]{1,6}", "com|org|net|io").prop_map(|(u, d, t)| format!("{u}@{d}.{t}"))
    }

    fn token() -> impl Strategy<Value = String> {
        "[A-Za-z0-9]{16,24}".prop_map(|s| format!("sk-{s}"))
    }

    fn phone() -> impl Strategy<Value = String> {
        "[0-9]{9,12}".prop_map(|s| s)
    }

    /// A sentinel-free chunk that is a word, email, token, or phone.
    fn chunk() -> impl Strategy<Value = String> {
        prop_oneof![word(), email(), token(), phone()]
    }

    fn document() -> impl Strategy<Value = String> {
        prop::collection::vec(chunk(), 0..30).prop_map(|parts| parts.join(" "))
    }

    proptest! {
        #[test]
        fn prop_roundtrip(doc in document()) {
            let m = RegexMasker::new();
            let (masked, map) = m.mask(&doc);
            prop_assert_eq!(m.unmask(&masked, &map), doc);
        }

        #[test]
        fn prop_no_identifier_survives(doc in document()) {
            let m = RegexMasker::new();
            let (masked, map) = m.mask(&doc);
            // Completeness: re-scanning the masked output finds nothing.
            let rescan = RegexMasker::new();
            prop_assert!(
                rescan.find_matches(&masked.text).is_empty(),
                "identifier survived masking: {:?}", masked.text
            );
            // And no captured original literally remains.
            for (_ph, original) in map.entries() {
                prop_assert!(!masked.text.contains(original.as_str()));
            }
        }
    }
}
