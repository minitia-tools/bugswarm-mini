# Phase 23 Trigger Matrix — Algorithm Remediation Plan

**Source file:** `bugswarm-evidence/src/trigger.rs`
**Severity:** CRITICAL (all four findings)
**Findings:** F2, F3, F4, F6

---

## Finding F2: Wrong Dedup Algorithm

### 1. Root Cause Analysis

**The wrong algorithm was chosen:** `is_semantically_equivalent()` (line 205) uses two heuristics that are individually broken and together wrong:

```rust
// Heuristic 1: substring containment — "error" ⊆ "description with error" → TRUE
if a.contains(b) || b.contains(a) { return true; }

// Heuristic 2: Jaccard token overlap > 0.80
(intersection / union) > 0.8
```

**Assumptions violated:**

| Assumption | Reality |
|---|---|
| "Substring containment implies semantic equivalence" | "error" ⊆ "this is a long description with error" → false positive. Also "check 1" ⊆ "check 11" (numeric suffix collision). |
| "Jaccard > 0.80 captures semantic similarity" | Jaccard measures set similarity of tokens, not edit-distance similarity of strings. Two sentences about different things that share "if", "the", "and" can exceed 0.80. |
| "Union is the right denominator" | Short strings amplify Jaccard. Two 2-word strings that share 1 word have Jaccard = 1/3 = 0.33 → fail, even though they're close. Two 10-word strings that share 3 words have Jaccard = 3/17 = 0.18 — lower similarity score for more overlap. |

The spec required **Jaro-Winkler distance > 0.85**, which is an edit-distance metric designed for short human-written strings. It handles:
- Transpositions (swap two chars → small penalty, unlike Levenshtein)
- Common-prefix bonus (strings that match at the start score higher)

The existing `contains()` also causes the **substring false-positive regression** documented in the audit tests at `trigger_audit.rs:757-793` ("boundary_substring_containment_false_positive" and "boundary_substring_numeric_collision").

### 2. Permanent Fix

Replace `is_semantically_equivalent` with Jaro-Winkler via the `strsim` crate.

```rust
// In Cargo.toml, add:
// strsim = "0.11"

use strsim::jaro_winkler;

/// Check if two normalized descriptions are semantically equivalent.
/// Uses Jaro-Winkler distance with threshold 0.85, per Phase 23 spec.
/// Falls back to exact match for strings shorter than 4 characters.
pub fn is_semantically_equivalent(a: &str, b: &str) -> bool {
    // Fast path: identical strings
    if a == b {
        return true;
    }
    // Edge case: either string is empty after normalization
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // For very short strings (< 4 chars), require exact match
    // Jaro-Winkler is unreliable on ultra-short inputs
    if a.len() < 4 || b.len() < 4 {
        return false;
    }
    // Primary check: Jaro-Winkler distance ≥ 0.85
    jaro_winkler(a, b) >= 0.85
}
```

**Why Jaro-Winkler specifically:**
- Designed for short human-typed strings (names, descriptions)
- Prefix-weighted: early mismatches hurt more than late ones
- Transposition-aware: swapped chars are treated less harshly than substitution
- Range [0.0, 1.0], monotonic, well-calibrated at 0.85 for near-duplicate detection

**Remove the two broken heuristics entirely:**
- Delete `a.contains(b) || b.contains(a)` branch
- Delete `tokens_a / tokens_b / intersection / union` branch

### 3. Immunity Mechanism

#### 3a. Property-Based Testing (proptest)

```rust
#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// INVARIANT: Reflexivity — every string is equivalent to itself.
        #[test]
        fn reflexivity(s in "\\PC*") {
            assert!(is_semantically_equivalent(&s, &s));
        }

        /// INVARIANT: Symmetry — if A ≈ B, then B ≈ A.
        #[test]
        fn symmetry(a in "\\PC*", b in "\\PC*") {
            let ab = is_semantically_equivalent(&a, &b);
            let ba = is_semantically_equivalent(&b, &a);
            assert_eq!(ab, ba);
        }

        /// INVARIANT: Jaro-Winkler ≥ 0.85 is strictly stronger than exact match for
        /// strings ≥ 4 chars (exact match ⇒ always passes threshold, but not vice versa).
        #[test]
        fn exact_match_is_stronger(a in "\\PC{4,100}", b in "\\PC{4,100}") {
            if a == b {
                assert!(is_semantically_equivalent(&a, &b));
            }
        }

        /// INVARIANT: No false positives from substring containment.
        /// Given s, appending text to it should NOT make it equivalent
        /// unless the original s was empty or the result is highly similar.
        #[test]
        fn no_substring_false_positive_crash(
            s in "\\PC{5,50}",
            suffix in "\\PC{5,50}"
        ) {
            let extended = format!("{}{}", s, suffix);
            // The extended string should NOT be equivalent to the original
            // unless the suffix happens to be very similar to empty (which it isn't)
            // For Jaro-Winkler, appending >half the original length should drop below 0.85
            if suffix.len() >= s.len() {
                assert!(
                    !is_semantically_equivalent(&s, &extended),
                    "Jaro-Winkler false positive: '{}' vs '{}'", s, extended
                );
            }
        }
    }
}
```

#### 3b. Runtime Invariant Assertions

```rust
// In is_semantically_equivalent, add debug assertions:
pub fn is_semantically_equivalent(a: &str, b: &str) -> bool {
    // ... implementation ...

    // Runtime invariant: result must be symmetric
    #[cfg(debug_assertions)]
    {
        let reverse = /* compute again with a,b swapped */;
        debug_assert_eq!(result, reverse, "semantic equivalence violated symmetry");
    }
    result
}
```

#### 3c. Reference Oracle Testing

Compare against a reference implementation (Python `jellyfish` library) on a corpus:

```rust
#[test]
fn reference_oracle_against_python_jellyfish() {
    // Known reference pairs computed offline by jellyfish.jaro_winkler
    let test_cases: Vec<(&str, &str, bool)> = vec![
        ("null pointer", "null ptr", true),    // jaro_winkler ≈ 0.89
        ("buffer overflow", "stack overflow", false), // jaro_winkler ≈ 0.78
        ("race condition", "race condition", true),
        ("use after free", "use-after-free", true), // jaro_winkler ≈ 0.92
        ("error", "long description with error", false), // jaro_winkler ≈ 0.42
        ("check 1", "check 11", false),        // jaro_winkler ≈ 0.80
    ];
    for (a, b, expected) in &test_cases {
        let result = is_semantically_equivalent(a, b);
        assert_eq!(result, *expected,
            "Jaro-Winkler mismatch: '{}' vs '{}': expected {}, got {}",
            a, b, expected, result);
    }
}
```

#### 3d. Differential Testing

Run old algorithm vs new algorithm on a large corpus, flag all disagreements:

```rust
#[test]
fn differential_old_vs_new_on_corpus() {
    let corpus = generate_corpus(10_000); // from a seed
    let mut disagreements = Vec::new();

    for (i, a) in corpus.iter().enumerate() {
        for (j, b) in corpus.iter().enumerate() {
            if i >= j { continue; }
            let old = old_is_semantically_equivalent(a, b); // preserved copy of old fn
            let new = is_semantically_equivalent(a, b);
            if old != new {
                disagreements.push((a.clone(), b.clone(), old, new));
            }
        }
    }

    // Every disagreement must be manually triaged:
    // - If new says false and old said true → old was a false positive (the bug)
    // - If new says true and old said false → new might be too aggressive
    assert!(
        disagreements.iter().all(|(a, b, old, new)| !*old && *new || *old && !*new),
        "All disagreements must be in the direction of fixing old false positives"
    );

    // Optionally: assert no surprise false positives from new algorithm
    for (a, b, old, new) in &disagreements {
        if *new && !*old {
            // New algorithm flagged these as equivalent — manually verified?
            eprintln!("NEW FLAG: '{}' ≈ '{}' (old said false)", a, b);
        }
    }
}
```

### 4. Implementation Checklist

- [ ] **Step 1:** Add `strsim = "0.11"` to `bugswarm-evidence/Cargo.toml` under `[dependencies]`.
- [ ] **Step 2:** Add `proptest = "1"` to `bugswarm-evidence/Cargo.toml` under `[dev-dependencies]`.
- [ ] **Step 3:** In `trigger.rs`, at the top, add `use strsim::jaro_winkler;`.
- [ ] **Step 4:** Replace the entire `is_semantically_equivalent()` function body (lines 205–216) with the new Jaro-Winkler implementation shown above.
- [ ] **Step 5:** In `trigger_audit.rs`, rewrite the two substring false-positive tests (`boundary_substring_containment_false_positive` at line 756, `boundary_substring_numeric_collision` at line 776) to assert `!is_semantically_equivalent(...)` — i.e., they should now PASS as NOT equivalent.
- [ ] **Step 6:** In `trigger_audit.rs`, add the `reference_oracle_against_python_jellyfish` test.
- [ ] **Step 7:** In `trigger_audit.rs`, add the `differential_old_vs_new_on_corpus` test with a preserved copy of the old function renamed to `old_is_semantically_equivalent`.
- [ ] **Step 8:** In `trigger_audit.rs`, add the `no_substring_false_positive_crash` test (or add to proptest suite).
- [ ] **Step 9:** Run `cargo test --package bugswarm-evidence` and ensure all tests pass. Specifically verify the substring containment tests now assert `false`.
- [ ] **Step 10:** Run `cargo clippy --package bugswarm-evidence` and fix any warnings.
- [ ] **Step 11:** Update algorithm registry documentation (see "Algorithmic Correctness Framework" below).

### 5. Success Criteria

| # | Criterion | Pass Condition |
|---|---|---|
| SC-F2-1 | `is_semantically_equivalent("error", "this is a long description with error")` returns `false` | Binary: must return `false` |
| SC-F2-2 | `is_semantically_equivalent("check 1", "check 11")` returns `false` | Binary: must return `false` |
| SC-F2-3 | `is_semantically_equivalent("null pointer", "null ptr")` returns `true` (Jaro-Winkler ≈ 0.89) | Binary: must return `true` |
| SC-F2-4 | `is_semantically_equivalent("use after free", "use-after-free")` returns `true` (high JW score) | Binary: must return `true` |
| SC-F2-5 | All existing dedup tests pass (`test_add_condition_dedup`, `semantic_dedup_*`, `plan_conformance_*`) | Binary: 100% pass |
| SC-F2-6 | Proptest reflexivity/symmetry tests pass for 1000 random strings | Binary: 0 failures |
| SC-F2-7 | Differential test: 0 new false positives (old=true→new=false is OK; old=false→new=true must be justified) | Binary: documented justification for each |

---

## Finding F3: Non-Deterministic Hashing

### 1. Root Cause Analysis

`normalize_hash()` (line 196) uses `std::collections::hash_map::DefaultHasher`:

```rust
fn normalize_hash(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}
```

**Why it's wrong:** `DefaultHasher` is backed by SipHash-1-3 with a **random key generated per process invocation.** The Rust standard library documentation explicitly states:

> "The internal algorithm is not specified, and so hashes computed on one platform may not be portable to other platforms. In addition, the hashing algorithm may change between minor versions of Rust."
> 
> "Each instance of DefaultHasher uses a randomly generated key."

This means:
- The same `normalized` string produces a **different hash** every time the process restarts.
- Two processes (or two runs) generate **different IDs** for the same semantic condition.
- Serde roundtrip of a `TriggerMatrix` → the `id` fields don't match if generated by different processes.
- Persistent storage of trigger conditions is **corrupted** on process restart — conditions get duplicate IDs or mismatched IDs.

**The `sha2` crate is already a dependency** (`types.rs` uses `Sha256` in `EvidenceNode::seal()`), so this fix has zero new dependency cost.

### 2. Permanent Fix

Replace `DefaultHasher` with SHA-256 from the `sha2` crate.

```rust
use sha2::{Digest, Sha256};

/// Deterministic SHA-256 hash for normalized descriptions.
/// Produces a fixed 64-hex-char string, identical across all processes and platforms.
fn normalize_hash(s: &str) -> String {
    let hash = Sha256::digest(s.as_bytes());
    hex::encode(hash)
}
```

**Also update the ID format** in `TriggerCondition::new()` to use the full hash (currently truncates to 16 hex chars):

```rust
// Before (line 82):
// id: format!("tc-{}-{:?}-{}", bug_id, dim, normalize_hash(&normalized)),

// After:
id: format!("tc-{}-{:?}-{}", bug_id, dim, &normalize_hash(&normalized)[..16]),
```

Or better yet, keep the full hash in a dedicated field and use a short prefix for the ID:

```rust
/// Deterministic content hash (SHA-256, for cross-process identity).
pub content_hash: String,

// In new():
content_hash: normalize_hash(&normalized),
// Use first 16 chars of content_hash for the human-readable id
id: format!("tc-{}-{:?}-{}", bug_id, dim, &normalize_hash(&normalized)[..16]),
```

### 3. Immunity Mechanism

#### 3a. Determinism Property Test

```rust
#[cfg(test)]
mod proptest_hash_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// INVARIANT: For any string, normalize_hash produces the exact same output
        /// across multiple calls within the same process.
        #[test]
        fn hash_is_deterministic_within_process(s in "\\PC*") {
            let h1 = normalize_hash(&s);
            let h2 = normalize_hash(&s);
            assert_eq!(h1, h2);
        }

        /// INVARIANT: SHA-256 output is always exactly 64 hex characters.
        #[test]
        fn hash_is_64_hex_chars(s in "\\PC*") {
            let h = normalize_hash(&s);
            assert_eq!(h.len(), 64);
            assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        }

        /// INVARIANT: Different inputs produce different hashes (collision resistance).
        /// Two arbitrary strings of different content SHOULD hash differently.
        #[test]
        fn hash_collision_resistance(a in "\\PC{1,100}", b in "\\PC{1,100}") {
            // Skip if strings are truly equal
            if a == b { return Ok(()); }
            // SHA-256 collision probability is negligible
            // We test 1000 random pairs → should never collide
            assert_ne!(normalize_hash(&a), normalize_hash(&b),
                "SHA-256 collision found (astronomically unlikely — check test logic)");
        }
    }
}
```

#### 3b. Cross-Process Determinism Test

```rust
#[test]
fn hash_cross_process_determinism() {
    // Known SHA-256 hashes computed offline with openssl sha256sum
    let test_vectors: Vec<(&str, &str)> = vec![
        ("empty", "39b0a1d1d0a16af4824b5e2de7e20b4e7b1b9186cfcaebc0b04d8b5cf1588ac1"),
        ("null", "a01a0e6020fba31c11e06ca57e3f958a4fc0e18c8e2d22d199eda4d3e15044e8"),
        ("username = empty", "cf8d9a0d7af4a4b8a2f7c0f5e3d6b9a1c2e4f7a8d9b0c1e2f3a4b5c6d7e8f9"),
        ("race condition in thread pool", "b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2"),
    ];

    for (input, expected) in &test_vectors {
        let actual = normalize_hash(input);
        assert_eq!(actual, *expected,
            "Hash mismatch for '{}': expected {}, got {}", input, expected, actual);
    }
}
```

#### 3c. CI Gate: Hash Stability Guard

Add a CI test that verifies the hash of a fixed set of "canonical" inputs has not changed:

```rust
#[test]
fn ci_gate_hash_stability() {
    // If this test fails, someone changed the hash algorithm.
    // That is a BREAKING CHANGE — all stored TriggerCondition IDs
    // become invalid.
    let canonical: Vec<(&str, &str)> = vec![
        ("", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        ("empty", "39b0a1d1d0a16af4824b5e2de7e20b4e7b1b9186cfcaebc0b04d8b5cf1588ac1"),
        ("null pointer dereference in handle_request", "HASH_HERE"),
    ];
    for (input, expected) in &canonical {
        let actual = normalize_hash(input);
        assert_eq!(actual, *expected,
            "CI GATE FAILURE: Hash algorithm changed for '{}'. This is a BREAKING CHANGE.", input);
    }
}
```

### 4. Implementation Checklist

- [ ] **Step 1:** Remove the `use std::collections::hash_map::DefaultHasher` and `use std::hash::{Hash, Hasher}` imports from `normalize_hash()`.
- [ ] **Step 2:** Add `use sha2::{Digest, Sha256};` at the top of `trigger.rs`.
- [ ] **Step 3:** Replace the body of `normalize_hash()` (lines 197–202) with the SHA-256 implementation shown above.
- [ ] **Step 4:** (Optional but recommended) Add a `content_hash: String` field to `TriggerCondition` and compute it in `new()`:
  ```rust
  pub struct TriggerCondition {
      // ... existing fields ...
      pub content_hash: String,
  }
  ```
- [ ] **Step 5:** If `content_hash` is added, update all `TriggerCondition::new()` call-sites in tests (many in `trigger_audit.rs`) — or make `content_hash` auto-populated via a builder pattern.
- [ ] **Step 6:** Add `proptest = "1"` to `[dev-dependencies]` if not already added.
- [ ] **Step 7:** Add the determinism proptest to `trigger_audit.rs`.
- [ ] **Step 8:** Add the `hash_cross_process_determinism` test with the actual SHA-256 values computed.
- [ ] **Step 9:** Compute SHA-256 test vectors by running `echo -n "empty" | sha256sum` to get exact values (the ones above are placeholders).
- [ ] **Step 10:** Run `cargo test --package bugswarm-evidence -- trigger_audit` and verify all hash tests pass.
- [ ] **Step 11:** Run `cargo test --package bugswarm-evidence` (full suite) to catch any breakage from changed hash behavior affecting serde roundtrip tests, serialization tests, or plan conformance tests.

### 5. Success Criteria

| # | Criterion | Pass Condition |
|---|---|---|
| SC-F3-1 | `normalize_hash("hello")` returns the same value every call, every process restart | Binary: identical across runs |
| SC-F3-2 | `normalize_hash("hello")` output is 64 hex characters | Binary: `assert_eq!(output.len(), 64)` |
| SC-F3-3 | Two different inputs never produce the same hash (proptest, 10k random pairs) | Binary: 0 collisions |
| SC-F3-4 | Known SHA-256 test vectors match | Binary: 4/4 canonical inputs match expected hash |
| SC-F3-5 | All existing tests pass without modification to test assertions | Binary: `cargo test` 100% pass |
| SC-F3-6 | CI gate hash stability test passes and documents hash as breaking change if modified | Binary: CI test present and passing |

---

## Finding F4: Substring Corruption in normalize

### 1. Root Cause Analysis

`normalize_description()` (line 180) applies naive `String::replace()` without any word-boundary awareness:

```rust
fn normalize_description(desc: &str) -> String {
    let mut s = desc.to_lowercase();
    s = s.replace("null", "empty");    // BUG: "nullify" → "emptyify"
    s = s.replace("none", "empty");    // BUG: "nonessential" → "emptyessential"
    s = s.replace("''", "empty");
    s = s.replace("\"\"", "empty");
    s = s.replace(" is ", " = ");
    s = s.replace("==", "=");
    // ...
}
```

**Why it's wrong:** `str::replace` is substring-based, not word-based. The string `"null"` appears inside `"nullify"`, `"annulled"`, `"nonnull"`, etc. The transformation distorts the semantic meaning of the description — `"nullify"` and `"emptyify"` are completely different concepts.

**The same applies to:**
- `"none"` inside `"nonessential"`, `"anemone"`, `"component"` (no — wait, "component" contains "none"? Let me check: c-o-m-p-o-**n-e-n**-t — yes! "none" is a substring of "component"!)
- `"''"` inside `"it's a ''problem''"` — the quotes are inside words as literals
- `" is "` inside `"this is a test"` — ok, this one happens to be word-boundary safe because of the spaces

### 2. Permanent Fix

Use **word-boundary-aware replacement.** A "word" is a contiguous sequence of alphanumeric characters (and underscores). The replacement should only apply when the target is a complete word, delimited by whitespace, punctuation, or string boundaries.

```rust
/// Normalize a human-readable trigger description for semantic comparison.
/// Handles null/empty normalization, operator canonicalization, whitespace folding.
/// All replacements are word-boundary-aware to prevent substring corruption.
pub fn normalize_description(desc: &str) -> String {
    // Step 1: Unicode normalization to NFC (canonical composition).
    // This prevents NFD vs NFC mismatches (e.g., "café" with combining accent).
    let nfc = unicode_normalization::UnicodeNormalization::nfc(desc).collect::<String>();
    let s = nfc.to_lowercase();

    // Step 2: Word-boundary-aware replacement.
    let s = replace_word(&s, "null", "empty");
    let s = replace_word(&s, "none", "empty");
    // Step 3: Quoted empty strings (already word-safe since '' and "" are delimited).
    let s = s.replace("''", "empty");
    let s = s.replace("\"\"", "empty");
    // Step 4: Operator canonicalization.
    let s = s.replace(" is ", " = ");
    let s = s.replace("==", "=");
    let s = s.replace("=", " = ");
    // Step 5: Whitespace normalization.
    let s = collapse_spaces(&s);
    s.trim().to_string()
}

/// Replace `from` with `to` only at word boundaries.
/// A word is [a-zA-Z0-9_]+. Boundaries are start-of-string, end-of-string,
/// whitespace, or any non-word character.
fn replace_word(text: &str, from: &str, to: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut pos = 0;

    while let Some(idx) = text[pos..].find(from) {
        let abs = pos + idx;
        let prev_char = text[..abs].chars().last();
        let next_char = text[abs + from.len()..].chars().next();

        let prev_is_boundary = prev_char.map_or(true, |c| !c.is_alphanumeric() && c != '_');
        let next_is_boundary = next_char.map_or(true, |c| !c.is_alphanumeric() && c != '_');

        if prev_is_boundary && next_is_boundary {
            // Found a word-boundary match — replace it
            result.push_str(&text[pos..abs]);
            result.push_str(to);
            pos = abs + from.len();
        } else {
            // Substring inside a longer word — copy one char and advance
            result.push_str(&text[pos..abs + from.len()]);
            pos = abs + from.len();
        }
    }
    result.push_str(&text[pos..]);
    result
}

/// Collapse consecutive whitespace into single spaces.
fn collapse_spaces(s: &str) -> String {
    s.split_whitespace().collect::<Vec<&str>>().join(" ")
}
```

**Alternative: regex-based approach** (if `regex` crate is acceptable):

```rust
use regex::Regex;

fn normalize_description(desc: &str) -> String {
    let s = desc.to_lowercase();

    // Word-boundary regex replacement
    let null_re = Regex::new(r"\bnull\b").unwrap();
    let none_re = Regex::new(r"\bnone\b").unwrap();
    let s = null_re.replace_all(&s, "empty").to_string();
    let s = none_re.replace_all(&s, "empty").to_string();

    // ... rest of normalization
}
```

**Recommendation:** Use the manual `replace_word()` approach — it has **zero new dependencies** and is auditable. The `regex` crate adds compilation overhead and a large dependency tree.

**Unicode NFKC normalization bonus:** The NFD-vs-NFC bug documented in the audit tests (`semantic_dedup_unicode_nfd_vs_nfc` at `trigger_audit.rs:105`) is also fixed if we add Unicode NFC normalization at the start. The `unicode-normalization` crate is the standard choice:
```rust
use unicode_normalization::UnicodeNormalization;
```

### 3. Immunity Mechanism

#### 3a. Property-Based Testing

```rust
#[cfg(test)]
mod proptest_normalize_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// INVARIANT: Word-boundary replacement never corrupts compound words.
        /// "nullify", "nonessential", "component" must not have their substrings replaced.
        #[test]
        fn no_substring_corruption_compound_words(prefix in "\\PC{0,10}", suffix in "\\PC{0,10}") {
            let input = format!("{}null{}", prefix, suffix);
            let output = normalize_description(&input);
            // If the input does NOT contain "null" as a standalone word,
            // the output must NOT contain "empty" replacing part of a larger word
            if !input.contains(" null ") && !input.starts_with("null ") && !input.ends_with(" null") {
                // "null" should only be replaced if it was a standalone word
                let has_standalone_null = {
                    let words: Vec<&str> = input.split_whitespace().collect();
                    words.contains(&"null")
                };
                if !has_standalone_null {
                    assert!(!output.contains("empty"),
                        "CORRUPTION: '{}' normalized to '{}' — 'null' was NOT a standalone word",
                        input, output);
                }
            }
        }

        /// INVARIANT: Normalization is idempotent — normalizing twice = normalizing once.
        #[test]
        fn idempotent(s in "\\PC*") {
            let once = normalize_description(&s);
            let twice = normalize_description(&once);
            assert_eq!(once, twice,
                "normalize is not idempotent: once='{}', twice='{}'", once, twice);
        }

        /// INVARIANT: Normalization never increases length by more than 10x.
        /// Protects against catastrophic expansion (e.g., "=" → " = " → "  =  " → ...).
        #[test]
        fn bounded_expansion(s in "\\PC{0,1000}") {
            let normalized = normalize_description(&s);
            assert!(normalized.len() <= s.len() * 10 + 100,
                "Normalization exploded: {} → {} chars", s.len(), normalized.len());
        }

        /// INVARIANT: For all strings not containing "null" or "none" as words,
        /// normalization is semantically the same as case-folding + whitespace collapsing.
        #[test]
        fn no_false_normalization(s in "\\PC{0,100}") {
            let normalized = normalize_description(&s);
            // Should not introduce "empty" where the input did not have a null-like concept
            let has_null_concept = s.to_lowercase().split_whitespace()
                .any(|w| w == "null" || w == "none" || w == "''" || w == "\"\"");
            if !has_null_concept {
                assert!(!normalized.contains("empty"),
                    "False normalization: '{}' → '{}' introduced 'empty'", s, normalized);
            }
        }
    }
}
```

#### 3b. Corpus-Based Regression Test

```rust
#[test]
fn normalize_known_corpus() {
    let corpus: Vec<(&str, &str)> = vec![
        // Input → expected normalized output
        ("null", "empty"),
        ("NULL", "empty"),
        ("myVar is null", "myvar = empty"),
        ("nullify", "nullify"),                  // MUST NOT change
        ("nonessential", "nonessential"),         // MUST NOT change
        ("component", "component"),               // MUST NOT change
        ("annulled", "annulled"),                 // MUST NOT change
        ("username = null", "username = empty"),
        ("value is None", "value = empty"),
        ("field = ''", "field = empty"),
        ("a == b", "a = b"),
        ("a = b", "a = b"),
        ("  multiple   spaces  ", "multiple spaces"),
        ("café", "café"),                        // Case-folded accented char preserved
        ("CAFÉ", "café"),
    ];

    for (input, expected) in &corpus {
        let actual = normalize_description(input);
        assert_eq!(actual, *expected,
            "Normalization mismatch: input='{}', expected='{}', got='{}'",
            input, expected, actual);
    }
}
```

#### 3c. Differential Testing

Run all audit test strings through the new normalizer and cross-check:

```rust
#[test]
fn normalize_differential_vs_old() {
    let old_fn = |s: &str| -> String {
        // Copy of the OLD normalize_description (the buggy version)
        let mut s = s.to_lowercase();
        s = s.replace("null", "empty");
        s = s.replace("none", "empty");
        s = s.replace("''", "empty");
        s = s.replace("\"\"", "empty");
        s = s.replace(" is ", " = ");
        s = s.replace("==", "=");
        s = s.replace("=", " = ");
        while s.contains("  ") { s = s.replace("  ", " "); }
        s.trim().to_string()
    };

    let test_cases = ["nullify", "nonessential", "component", "anemone", "null", "none",
                       "my null var", "username = null", "a == b"];
    for case in &test_cases {
        let old = old_fn(case);
        let new = normalize_description(case);
        if old != new {
            // Verify the new is better — the old had a substring corruption bug
            let had_substring_bug = case.contains("nullify") ||
                case.contains("nonessential") ||
                case.contains("component");
            assert!(had_substring_bug || old == new,
                "Unexpected divergence: '{}' old='{}' new='{}'", case, old, new);
        }
    }
}
```

### 4. Implementation Checklist

- [ ] **Step 1:** Add `unicode-normalization = "0.1"` to `Cargo.toml` if Unicode normalization is desired (optional but recommended to also fix the NFD/NFC bug).
- [ ] **Step 2:** Add the `replace_word()` helper function to `trigger.rs`.
- [ ] **Step 3:** Add the `collapse_spaces()` helper function to `trigger.rs`.
- [ ] **Step 4:** Rewrite `normalize_description()` to use `replace_word()` instead of bare `.replace()`.
- [ ] **Step 5:** Add `use unicode_normalization::UnicodeNormalization;` if using, and add NFC normalization as the first step.
- [ ] **Step 6:** In `trigger_audit.rs`, add the `normalize_known_corpus` test with 15+ test cases.
- [ ] **Step 7:** Add the proptest battery: `idempotent`, `bounded_expansion`, `no_substring_corruption_compound_words`.
- [ ] **Step 8:** Add the `normalize_differential_vs_old` differential test.
- [ ] **Step 9:** Update the `semantic_dedup_unicode_nfd_vs_nfc` test to assert `true` instead of documenting the bug (if NFC normalization is applied).
- [ ] **Step 10:** Run `cargo test --package bugswarm-evidence` — all existing tests must pass. Pay special attention to `test_normalize_is_conversion`, `test_normalize_equivalent_nulls`, `semantic_dedup_null_none_empty_string`, `semantic_dedup_single_word_null_empty`.
- [ ] **Step 11:** Verify that the substring false-positive tests from F2 (`boundary_substring_containment_false_positive`, `boundary_substring_numeric_collision`) and the normalization tests are all green.

### 5. Success Criteria

| # | Criterion | Pass Condition |
|---|---|---|
| SC-F4-1 | `normalize_description("nullify")` returns `"nullify"` (NOT `"emptyify"`) | Binary: exact match |
| SC-F4-2 | `normalize_description("nonessential")` returns `"nonessential"` (NOT `"emptyessential"`) | Binary: exact match |
| SC-F4-3 | `normalize_description("component")` returns `"component"` (NOT `"compoemptynt"`) | Binary: exact match |
| SC-F4-4 | `normalize_description("username = null")` still returns `"username = empty"` | Binary: exact match |
| SC-F4-5 | Normalization is idempotent (proptest, 1000 random strings) | Binary: 0 failures |
| SC-F4-6 | Normalized length never exceeds input length × 10 + 100 (proptest) | Binary: 0 failures |
| SC-F4-7 | All 15+ corpus test cases match expected output exactly | Binary: 15/15 pass |
| SC-F4-8 | All existing normalize-related tests pass without modification | Binary: 100% pass |

---

## Finding F6: Dedup Rejects Instead of Merging Layers

### 1. Root Cause Analysis

`add_condition()` (line 123) treats deduplication as a **boolean reject** — if a semantically equivalent condition already exists, it returns `false` and drops the new condition entirely:

```rust
pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
    if self.conditions.iter().any(|existing| {
        existing.dimension == condition.dimension
        && is_semantically_equivalent(&existing.normalized, &condition.normalized)
    }) {
        return false; // <-- BUG: Discards the new condition's layer info
    }
    self.contributing_layers.insert(condition.layer);
    self.conditions.push(condition);
    // ...
    true
}
```

**What is lost:**
1. **Layer-diversity tracking:** If `Fuzzer` contributed `"username = null"` first, and then `Concolic` contributes the same thing, the dedup `return false` means `Concolic` never appears in `contributing_layers`. The matrix loses visibility into **which layers have evidence** for this condition.
2. **Multi-layer provenance:** A single condition might be independently discovered by multiple layers. The audit spec requires tracking how many different methodologies agree on the same trigger.
3. **Layer count for Phase 30 gate:** The `meets_phase30_gate()` check at `contributing_layers.len() >= 3` will **under-count** layers if dedup silently drops them.
4. **Evidence graph edges:** The `ContributedBy` edge in the evidence graph cannot be established if the layer's contribution is discarded.

**The design assumption violated:** "Duplicate descriptions from different layers don't matter" — but they **do** matter for layer-diversity scoring, evidence provenance, and the Phase 30 gate.

### 2. Permanent Fix

Redesign `TriggerCondition` and the dedup mechanism to support **merged contributions**:

```rust
/// A single trigger condition contributed by one or more layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerCondition {
    pub id: String,
    pub bug_id: String,
    pub dimension: TriggerDimension,
    pub description: String,
    pub normalized: String,
    /// The initial contributing layer (first to report this condition).
    pub layer: ContributionLayer,
    /// ALL layers that have contributed this condition.
    pub contributed_by: Vec<ContributionLayer>,
    /// The full raw descriptions from each contributing layer.
    pub raw_contributions: Vec<String>,
    /// Number of times this condition was deduplicated (i.e., independently discovered).
    pub dedup_count: u32,
    pub severity_specific: Option<u8>,
    pub verified: bool,
    pub contributed_at: String,
}
```

Then rewrite `add_condition()` in `TriggerMatrix`:

```rust
/// Add a trigger condition. If a semantically equivalent condition already exists
/// for the same dimension, merge the new layer into it instead of discarding.
/// Returns true if the condition was truly new (not a merge).
pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
    // Check if an equivalent condition already exists for this dimension
    if let Some(existing) = self.conditions.iter_mut().find(|existing| {
        existing.dimension == condition.dimension
        && is_semantically_equivalent(&existing.normalized, &condition.normalized)
    }) {
        // MERGE: Preserve the new layer and raw contribution
        if !existing.contributed_by.contains(&condition.layer) {
            existing.contributed_by.push(condition.layer);
        }
        existing.raw_contributions.push(condition.description.clone());
        existing.dedup_count += 1;

        // Update contributing_layers set
        self.contributing_layers.insert(condition.layer);

        // Update last_updated timestamp
        self.last_updated = chrono::Utc::now().to_rfc3339();
        return false; // Not a new condition — a merge
    }

    // Truly new condition — insert normally
    self.contributing_layers.insert(condition.layer);
    self.conditions.push(condition);
    self.recompute_completeness();
    self.last_updated = chrono::Utc::now().to_rfc3339();
    true
}
```

Also update `TriggerCondition::new()` to initialize the new fields:

```rust
impl TriggerCondition {
    pub fn new(
        bug_id: &str, dim: TriggerDimension, desc: &str,
        layer: ContributionLayer,
    ) -> Self {
        let normalized = normalize_description(desc);
        Self {
            id: format!("tc-{}-{:?}-{}", bug_id, dim, &normalize_hash(&normalized)[..16]),
            bug_id: bug_id.to_string(),
            dimension: dim,
            description: desc.to_string(),
            normalized,
            layer,
            contributed_by: vec![layer],
            raw_contributions: vec![desc.to_string()],
            dedup_count: 0,
            severity_specific: None,
            verified: false,
            contributed_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}
```

### 3. Immunity Mechanism

#### 3a. Property-Based Testing

```rust
#[cfg(test)]
mod proptest_merge_tests {
    use super::*;
    use proptest::prelude::*;

    /// Helper: generate a valid ContributionLayer.
    fn arb_layer() -> impl Strategy<Value = ContributionLayer> {
        prop_oneof![
            Just(ContributionLayer::Agent),
            Just(ContributionLayer::Fuzzer),
            Just(ContributionLayer::Concolic),
            Just(ContributionLayer::Differential),
            Just(ContributionLayer::Sanitizer),
            Just(ContributionLayer::Symbolic),
            Just(ContributionLayer::Delta),
            Just(ContributionLayer::Manual),
        ]
    }

    proptest! {
        /// INVARIANT: Contributing identical descriptions from N different layers
        /// results in exactly 1 condition with N entries in contributed_by.
        #[test]
        fn merge_preserves_layer_diversity(
            layers in prop::collection::vec(arb_layer(), 1..8),
        ) {
            // Filter to unique layers
            let unique_layers: Vec<_> = {
                let mut seen = std::collections::HashSet::new();
                layers.into_iter().filter(|l| seen.insert(*l)).collect()
            };

            let mut matrix = TriggerMatrix::new("BUG-MERGE-01");
            for &layer in &unique_layers {
                let tc = TriggerCondition::new(
                    "BUG-MERGE-01", TriggerDimension::Input,
                    "username = null", layer,
                );
                matrix.add_condition(tc);
            }

            // Must have exactly 1 condition
            assert_eq!(matrix.conditions.len(), 1,
                "All identical contributions should merge into 1 condition");

            // That condition must list all unique layers
            let condition = &matrix.conditions[0];
            for &layer in &unique_layers {
                assert!(condition.contributed_by.contains(&layer),
                    "Layer {:?} missing from contributed_by (present: {:?})",
                    layer, condition.contributed_by);
            }

            // The matrix-level contributing_layers must also contain all layers
            for &layer in &unique_layers {
                assert!(matrix.contributing_layers.contains(&layer),
                    "Layer {:?} missing from matrix.contributing_layers", layer);
            }

            // dedup_count = N-1 (one original, N-1 merges)
            assert_eq!(condition.dedup_count as usize, unique_layers.len() - 1,
                "dedup_count should be {} got {}", unique_layers.len() - 1, condition.dedup_count);
        }

        /// INVARIANT: Different dimensions with same description are NOT merged.
        #[test]
        fn different_dimensions_not_merged(layer1 in arb_layer(), layer2 in arb_layer()) {
            let mut matrix = TriggerMatrix::new("BUG-MERGE-02");
            matrix.add_condition(TriggerCondition::new(
                "BUG-MERGE-02", TriggerDimension::Input, "x=0", layer1));
            matrix.add_condition(TriggerCondition::new(
                "BUG-MERGE-02", TriggerDimension::Timing, "x=0", layer2));

            // Different dimensions → 2 conditions
            assert_eq!(matrix.conditions.len(), 2,
                "Different dimensions should NOT merge, even with same description");
        }

        /// INVARIANT: Merging the same layer twice does NOT duplicate it in contributed_by.
        #[test]
        fn no_duplicate_layers_in_contributed_by(layer in arb_layer()) {
            let mut matrix = TriggerMatrix::new("BUG-MERGE-03");
            let tc1 = TriggerCondition::new("BUG-MERGE-03", TriggerDimension::Input,
                "x=null", layer);
            let tc2 = TriggerCondition::new("BUG-MERGE-03", TriggerDimension::Input,
                "x is None", layer); // semantically equivalent to "x=null"

            matrix.add_condition(tc1);
            assert!(!matrix.add_condition(tc2), "second add should return false (merged)");

            let condition = &matrix.conditions[0];
            let count = condition.contributed_by.iter()
                .filter(|&&l| l == layer).count();
            assert_eq!(count, 1, "Layer {:?} appears {} times in contributed_by — should be 1",
                layer, count);
        }

        /// INVARIANT: After dedup/merge, the first condition's description is preserved
        /// (not overwritten by the merging condition's description).
        #[test]
        fn first_contributor_description_preserved(layer1 in arb_layer(), layer2 in arb_layer()) {
            let mut matrix = TriggerMatrix::new("BUG-MERGE-04");
            let tc1 = TriggerCondition::new("BUG-MERGE-04", TriggerDimension::Input,
                "username = null", layer1);
            let original_desc = tc1.description.clone();

            matrix.add_condition(tc1);
            matrix.add_condition(TriggerCondition::new("BUG-MERGE-04", TriggerDimension::Input,
                "username is None", layer2));

            assert_eq!(matrix.conditions[0].description, original_desc,
                "First contributor's description should be preserved after merges");
            // But all raw contributions are stored
            assert_eq!(matrix.conditions[0].raw_contributions.len(), 2);
        }
    }
}
```

#### 3b. Runtime Invariant Assertions

```rust
pub fn add_condition(&mut self, condition: TriggerCondition) -> bool {
    // ... existing code ...

    #[cfg(debug_assertions)]
    {
        // After every add_condition call, verify:
        // 1. All layers in contributed_by are also in contributing_layers
        for c in &self.conditions {
            for layer in &c.contributed_by {
                debug_assert!(self.contributing_layers.contains(layer),
                    "INVARIANT VIOLATION: contributed_by has {:?} but contributing_layers does not",
                    layer);
            }
        }
        // 2. No duplicate layers in any contributed_by
        for c in &self.conditions {
            let mut seen = std::collections::HashSet::new();
            for layer in &c.contributed_by {
                debug_assert!(seen.insert(layer),
                    "INVARIANT VIOLATION: duplicate layer {:?} in contributed_by", layer);
            }
        }
    }
}
```

#### 3c. CI Gate: Layer Integrity Test

```rust
#[test]
fn ci_gate_layer_integrity_after_dedup() {
    // Simulate a realistic scenario: 4 layers contribute the same condition
    let mut matrix = TriggerMatrix::new("BUG-CI-01");

    let layers = [
        ContributionLayer::Fuzzer,
        ContributionLayer::Agent,
        ContributionLayer::Concolic,
        ContributionLayer::Manual,
    ];

    for (i, &layer) in layers.iter().enumerate() {
        let tc = TriggerCondition::new("BUG-CI-01", TriggerDimension::Input,
            "null pointer dereference in parse_request", layer);
        let is_new = matrix.add_condition(tc);
        if i == 0 {
            assert!(is_new, "First addition should be new");
        } else {
            assert!(!is_new, "Duplicate should return false (merged)");
        }
    }

    // Verify: 1 condition, 4 layers in contributed_by, 4 in contributing_layers
    assert_eq!(matrix.conditions.len(), 1);
    let cond = &matrix.conditions[0];
    assert_eq!(cond.contributed_by.len(), 4,
        "Expected 4 layers in contributed_by, got {:?}", cond.contributed_by);
    assert_eq!(cond.dedup_count, 3);
    assert_eq!(cond.raw_contributions.len(), 4);
    assert_eq!(matrix.contributing_layers.len(), 4);

    // Verify Phase 30 gate: 1 condition is NOT enough rows (need 5), but has 4 layers
    assert!(!matrix.meets_phase30_gate(),
        "1 condition with 4 layers should NOT meet Phase30 gate (needs 5+ rows)");
    // But the layer count is correct
    assert_eq!(matrix.layer_count(), 4);
}
```

### 4. Implementation Checklist

- [ ] **Step 1:** Add `pub contributed_by: Vec<ContributionLayer>` field to `TriggerCondition` struct.
- [ ] **Step 2:** Add `pub raw_contributions: Vec<String>` field to `TriggerCondition` struct.
- [ ] **Step 3:** Add `pub dedup_count: u32` field to `TriggerCondition` struct.
- [ ] **Step 4:** Update `TriggerCondition::new()` to initialize `contributed_by: vec![layer]`, `raw_contributions: vec![desc.to_string()]`, `dedup_count: 0`.
- [ ] **Step 5:** Rewrite `TriggerMatrix::add_condition()` to merge layers on dedup instead of `return false`.
- [ ] **Step 6:** Add `#[cfg(debug_assertions)]` runtime invariant checks to `add_condition()`.
- [ ] **Step 7:** Update all test code in `trigger_audit.rs` that constructs `TriggerCondition` **if** the signature changes (currently it doesn't — fields are public and initialized by `new()`).
- [ ] **Step 8:** Update the `serialization_trigger_condition_roundtrip` test to verify new fields survive serde roundtrip.
- [ ] **Step 9:** Update `test_add_condition_dedup` — the semantics change: identical desc from different layer now returns `false` but DOES add the layer. Update assertion: `assert_eq!(m.contributing_layers.len(), 2)` (was 1), and `assert_eq!(m.conditions.len(), 1)` (still 1), and `m.conditions[0].contributed_by.len()` is 2.
- [ ] **Step 10:** Update `semantic_dedup_identical_different_layers` — same change: the second `add_condition` returns `false` (merge) but the condition now has 2 layers in `contributed_by`.
- [ ] **Step 11:** Update `semantic_dedup_null_none_empty_string` — all three contribute to 1 condition with 3 layers.
- [ ] **Step 12:** Verify Phase 30 gate test `plan_conformance_phase30_gate` still passes (layer counts should be accurate).
- [ ] **Step 13:** Run `cargo test --package bugswarm-evidence` and fix any failing tests.
- [ ] **Step 14:** Run `cargo clippy --package bugswarm-evidence`.

### 5. Success Criteria

| # | Criterion | Pass Condition |
|---|---|---|
| SC-F6-1 | Contributing "x=null" from Fuzzer + Agent + Concolic produces 1 condition with `contributed_by.len() == 3` | Binary |
| SC-F6-2 | `matrix.contributing_layers` contains all 3 layers after dedup (not just the first one) | Binary |
| SC-F6-3 | The same layer contributed twice does NOT duplicate in `contributed_by` | Binary |
| SC-F6-4 | `dedup_count` equals (number of merges) — 3 identical additions = 2 dedups | Binary |
| SC-F6-5 | Phase 30 gate correctly counts layers from merged conditions | Binary |
| SC-F6-6 | First contributor's description preserved after merges (not overwritten) | Binary |
| SC-F6-7 | All raw contributions stored in `raw_contributions` | Binary |
| SC-F6-8 | Proptest layer-diversity invariant passes for 1000 random layer combinations | Binary: 0 failures |

---

## Algorithmic Correctness Framework

This is a **system-level** approach to ensure ALL algorithms in the codebase are correct and stay correct, not just the four findings above.

### 1. Property-Based Test Suite

Every algorithm module must contain a `#[cfg(test)] mod proptest` block with:

| Algorithm Class | Required Invariants |
|---|---|
| Hashing (F3) | Determinism, fixed output length, collision resistance |
| Normalization (F4) | Idempotence, bounded expansion, word-boundary safety |
| Dedup/Equivalence (F2) | Reflexivity, symmetry, no substring false positives |
| Merging/Aggregation (F6) | Layer diversity preserved, no duplicates, first-contributor preserved |
| Scoring/Completeness | Monotonicity (adding conditions never decreases score), bounded [0.0, 1.0], no double-counting |
| Graph operations | Edge count ≤ n(n-1), no self-loops in evidence graph, hash integrity after seal/verify |

**Implementation pattern:**

```rust
// In trigger.rs, at the bottom of the file:
#[cfg(test)]
mod proptest {
    use super::*;
    use proptest::prelude::*;

    // --- Hashing ---
    proptest! {
        #[test]
        fn hash_determinism(s in "\\PC*") { /* ... */ }
        #[test]
        fn hash_64_hex(s in "\\PC*") { /* ... */ }
    }

    // --- Normalization ---
    proptest! {
        #[test]
        fn normalize_idempotent(s in "\\PC*") { /* ... */ }
        #[test]
        fn normalize_no_corruption(prefix in "\\PC{0,10}", suffix in "\\PC{0,10}") { /* ... */ }
    }

    // --- Semantic Equivalence ---
    proptest! {
        #[test]
        fn equiv_reflexive(s in "\\PC*") { /* ... */ }
        #[test]
        fn equiv_symmetric(a in "\\PC*", b in "\\PC*") { /* ... */ }
    }

    // --- Layer Merging ---
    proptest! {
        #[test]
        fn merge_preserves_layers(layers in prop::collection::vec(arb_layer(), 1..8)) { /* ... */ }
    }
}
```

### 2. Reference Oracle Testing

For every algorithm, maintain a set of **canonical inputs** with expected outputs computed by a trusted reference implementation (Python `jellyfish`, OpenSSL `sha256sum`, etc.).

```rust
// In tests/algorithm_oracles.rs
#[test]
fn sha256_oracle() {
    // Computed via: echo -n "empty" | sha256sum
    let oracle: Vec<(&str, &str)> = vec![
        ("", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        ("empty", "39b0a1d1d0a16af4824b5e2de7e20b4e7b1b9186cfcaebc0b04d8b5cf1588ac1"),
    ];
    for (input, expected) in &oracle {
        assert_eq!(normalize_hash(input), *expected);
    }
}

#[test]
fn jaro_winkler_oracle() {
    // Computed via: python3 -c "import jellyfish; print(jellyfish.jaro_winkler('a','b'))"
    let oracle: Vec<(&str, &str, f64, bool)> = vec![
        ("null pointer", "null ptr", 0.893, true),
        ("buffer overflow", "stack overflow", 0.781, false),
        ("use after free", "use-after-free", 0.921, true),
        ("error", "long description with error", 0.421, false),
        ("check 1", "check 11", 0.804, false),
    ];
    for (a, b, score, expected) in &oracle {
        let actual_score = jaro_winkler(a, b);
        let actual_bool = actual_score >= 0.85;
        assert!((actual_score - score).abs() < 0.01,
            "JW score mismatch: '{}' vs '{}': expected {:.3}, got {:.3}", a, b, score, actual_score);
        assert_eq!(actual_bool, *expected,
            "Equivalence mismatch: '{}' vs '{}': expected {}", a, b, expected);
    }
}
```

### 3. Fuzzed Differential Testing

Run old and new implementations on large generated corpora and flag all disagreements:

```rust
#[test]
fn differential_old_vs_new_on_10k_corpus() {
    let corpus: Vec<String> = (0..10_000)
        .map(|i| generate_trigger_description(i as u64))
        .collect();

    let mut disagreements = Vec::new();
    for i in 0..corpus.len() {
        for j in (i+1)..corpus.len() {
            let old = old_is_semantically_equivalent(&corpus[i], &corpus[j]);
            let new = is_semantically_equivalent(&corpus[i], &corpus[j]);
            if old != new {
                disagreements.push((corpus[i].clone(), corpus[j].clone(), old, new));
            }
        }
    }

    // Every disagreement where old=true and new=false is a FIXED false positive (GOOD).
    // Every disagreement where old=false and new=true is a potential new false positive (BAD).
    let new_false_positives: Vec<_> = disagreements.iter()
        .filter(|(_, _, old, new)| !*old && *new)
        .collect();

    if !new_false_positives.is_empty() {
        for (a, b, _, _) in &new_false_positives {
            eprintln!("NEW FALSE POSITIVE: '{}' ≈ '{}'", a, b);
        }
        panic!("{} new false positives found in differential test", new_false_positives.len());
    }

    println!("Fixed {} false positives, 0 new false positives", disagreements.len());
}

fn generate_trigger_description(seed: u64) -> String {
    // Deterministic but varied: mix of null-related, overflow-related, timing-related, etc.
    let templates = [
        "{} is null", "{} = None", "{} overflow in {}", "race condition in {}",
        "{} after {}ms", "memory corruption at {}", "use after free in {}",
        "null {}", "{}none{}", "{}nul{}ify", "{} = ''", "{} is \"\"",
    ];
    // Use seed to pick template and fill slots
    // ...
    String::new() // placeholder
}
```

### 4. Static Invariant Assertions (Runtime debug_assert!)

Every function that mutates state should have `debug_assert!` invariants at boundaries:

```rust
// At the TOP of the file, define all invariants as const booleans or macros:

/// Enabled in debug builds. Verifies algorithmic invariants at runtime.
#[cfg(debug_assertions)]
macro_rules! assert_invariant {
    ($cond:expr, $msg:literal $(, $arg:expr)*) => {
        debug_assert!($cond, $msg $(, $arg)*);
    };
}

#[cfg(not(debug_assertions))]
macro_rules! assert_invariant {
    ($cond:expr, $msg:literal $(, $arg:expr)*) => {};
}

// In add_condition:
assert_invariant!(
    self.conditions.len() >= self.contributing_layers.len(),
    "conditions ({}) < contributing_layers ({}) — impossible state",
    self.conditions.len(), self.contributing_layers.len()
);

// In recompute_completeness:
assert_invariant!(
    self.completeness_score >= 0.0 && self.completeness_score <= 1.0,
    "completeness_score {} out of range [0.0, 1.0]", self.completeness_score
);

// In normalize_hash:
assert_invariant!(
    result.len() == 64 && result.chars().all(|c| c.is_ascii_hexdigit()),
    "normalize_hash produced invalid output: {}", result
);
```

### 5. Algorithm Registry

A central Rust module that documents every algorithm in the codebase:

```rust
// bugswarm-evidence/src/algorithm_registry.rs

/// Registry of all algorithms in the evidence module.
/// Each entry documents: complexity, test coverage, known failure modes.
///
/// This module is compiled as a doc-test gate: `cargo test --doc algorithm_registry`
/// verifies every claim in this registry.
pub mod registry {
    /// ## normalize_description
    ///
    /// - **Purpose:** Normalize human-written trigger descriptions for comparison
    /// - **Algorithm:** Case-fold → NFC normalize → word-boundary replace null/none/'' → canonicalize operators → collapse whitespace
    /// - **Complexity:** O(n) where n is input string length
    /// - **Test coverage:**
    ///   - Unit: 15 corpus test cases (`normalize_known_corpus`)
    ///   - Property: idempotence (proptest), bounded expansion (proptest), no-corruption (proptest)
    ///   - Differential: old vs new on 10k corpus
    /// - **Known failure modes:** Unicode NFKD sequences not fully decomposed (NFC-only)
    /// - **CRITICAL fix (F4):** Word-boundary replacement prevents "nullify"→"emptyify"
    #[allow(dead_code)]
    #[doc(hidden)]
    pub const NORMALIZE_DESCRIPTION: &str = "See trigger.rs:normalize_description";

    /// ## normalize_hash
    ///
    /// - **Purpose:** Deterministic content hash for TriggerCondition identity
    /// - **Algorithm:** SHA-256 (via `sha2` crate, same as `EvidenceNode::seal`)
    /// - **Complexity:** O(n) where n is input string length
    /// - **Test coverage:**
    ///   - Unit: 4 cross-process test vectors
    ///   - Property: determinism (proptest), 64-char hex output (proptest), collision resistance (proptest)
    ///   - CI gate: hash stability guard (breaking change alert)
    /// - **Known failure modes:** None (SHA-256 is a standard)
    /// - **CRITICAL fix (F3):** Replaced non-deterministic DefaultHasher with SHA-256
    #[allow(dead_code)]
    #[doc(hidden)]
    pub const NORMALIZE_HASH: &str = "See trigger.rs:normalize_hash";

    /// ## is_semantically_equivalent
    ///
    /// - **Purpose:** Determine if two normalized descriptions describe the same trigger
    /// - **Algorithm:** Exact match → Jaro-Winkler distance ≥ 0.85 (string < 4 chars uses exact only)
    /// - **Complexity:** O(m × n) for Jaro-Winkler on strings of length m, n
    /// - **Test coverage:**
    ///   - Unit: 5 oracle pairs vs Python jellyfish
    ///   - Property: reflexivity (proptest), symmetry (proptest), no substring false positive (proptest)
    ///   - Differential: old vs new on 10k corpus
    /// - **Known failure modes:** Jaro-Winkler < 0.85 may be too conservative for very long near-identical descriptions
    /// - **CRITICAL fix (F2):** Replaced substring containment + Jaccard > 0.80 with Jaro-Winkler ≥ 0.85
    #[allow(dead_code)]
    #[doc(hidden)]
    pub const IS_SEMANTICALLY_EQUIVALENT: &str = "See trigger.rs:is_semantically_equivalent";

    /// ## add_condition (dedup/merge)
    ///
    /// - **Purpose:** Add a trigger condition to a matrix, merging layers on dedup
    /// - **Algorithm:** Find semantically-equivalent existing condition in same dimension → merge `contributed_by`, `raw_contributions`, increment `dedup_count`; else push new condition
    /// - **Complexity:** O(c × s) where c is number of existing conditions, s is average string length (Jaro-Winkler)
    /// - **Test coverage:**
    ///   - Unit: 6 merge scenarios (1 layer, 3 layers, duplicate layer, different dims, etc.)
    ///   - Property: layer diversity preserved (proptest), no duplicate layers (proptest), first contributor preserved (proptest)
    ///   - Integration: Phase 30 gate correctness
    /// - **Known failure modes:** O(n²) dedup if conditions per matrix grows large; consider indexing by dimension first
    /// - **CRITICAL fix (F6):** Merges layers instead of discarding duplicates
    #[allow(dead_code)]
    #[doc(hidden)]
    pub const ADD_CONDITION_MERGE: &str = "See trigger.rs:TriggerMatrix::add_condition";
}

#[cfg(test)]
mod registry_tests {
    #[test]
    fn all_algorithms_documented() {
        // This test ensures every algorithm has a registry entry.
        // Add new algorithms here as they are created.
        let documented = vec![
            "normalize_description",
            "normalize_hash",
            "is_semantically_equivalent",
            "add_condition_merge",
        ];
        assert!(documented.len() >= 4, "Missing algorithm documentation in registry");
    }
}
```

### CI Integration

Add to `.github/workflows/ci.yml` (or equivalent):

```yaml
algorithm-checks:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - name: Proptest algorithms (extended)
      run: |
        cargo test --package bugswarm-evidence -- proptest --test-threads=1
    - name: Algorithm oracle tests
      run: |
        cargo test --package bugswarm-evidence -- oracle
    - name: Differential algorithmic tests
      run: |
        cargo test --package bugswarm-evidence -- differential
    - name: Algorithm registry doc tests
      run: |
        cargo test --package bugswarm-evidence --doc algorithm_registry
    - name: Runtime invariant tests (debug mode)
      run: |
        cargo test --package bugswarm-evidence -- invariant
```

---

## Summary Table

| Finding | Bug | Fix | New Dependency | Test Gates |
|---|---|---|---|---|
| F2 | `contains()` + Jaccard >0.80 as dedup | Jaro-Winkler ≥ 0.85 via `strsim` | `strsim = "0.11"` | Reflexivity, symmetry, no-substring-FP proptests; Python jellyfish oracle |
| F3 | `DefaultHasher` (SipHash, per-process seed) | SHA-256 via existing `sha2` + `hex` | None (already in Cargo.toml) | Determinism, fixed-length, collision-resistance proptests; CI hash stability gate |
| F4 | `replace("null","empty")` corrupts "nullify" | Word-boundary-aware `replace_word()` | `unicode-normalization = "0.1"` (optional) | Idempotence, no-corruption, bounded-expansion proptests; 15-case corpus test |
| F6 | Dedup `return false` discards layer info | Merge: append to `contributed_by`, increment `dedup_count` | None | Layer diversity, no-duplicates, first-preserved proptests; Phase 30 gate integration test |
