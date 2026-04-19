//! Canonical VIL term checker.
//! Validates that artifacts use canonical terminology, not VFlow-era legacy aliases.

use serde::{Deserialize, Serialize};

/// Legacy terms that must not appear in new artifacts.
const LEGACY_TERMS: &[(&str, &str)] = &[
    ("v-cel", "vil-expr"),
    ("VRule", "Rule"),
    ("VxApp", "VilServer"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationMode {
    /// Strict: legacy terms in new artifacts are errors.
    Strict,
    /// Compat: legacy terms produce warnings only (for reading old artifacts).
    Compat,
}

/// Alias exposed for the linter API to match the roadmap spec (`CanonicalMode`).
pub type CanonicalMode = ValidationMode;

#[derive(Debug, Default)]
pub struct CanonicalCheckResult {
    pub passed: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Severity classification produced by [`lint_text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanonicalSeverity {
    /// Strict mode: violation must be fixed before the artifact is accepted.
    Error,
    /// Compat mode: violation is informational (legacy alias detected).
    Warning,
}

/// A single canonical-term violation discovered in text.
///
/// `span` is an inclusive-exclusive byte range (`start..end`) such that
/// `&text[start..end] == legacy_term`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalViolation {
    pub span: (usize, usize),
    pub legacy_term: String,
    pub suggested_replacement: String,
    pub severity: CanonicalSeverity,
}

/// Check a string for legacy VIL terms (legacy summary-based API).
/// In Strict mode, any legacy term is an error.
/// In Compat mode, legacy terms produce warnings only.
pub fn check_terms(content: &str, mode: ValidationMode) -> CanonicalCheckResult {
    let mut result = CanonicalCheckResult {
        passed: true,
        warnings: vec![],
        errors: vec![],
    };

    for (legacy, canonical) in LEGACY_TERMS {
        if content.contains(legacy) {
            let msg = format!(
                "legacy alias '{}' detected; use '{}' instead",
                legacy, canonical
            );
            match mode {
                ValidationMode::Strict => {
                    result.errors.push(msg);
                    result.passed = false;
                }
                ValidationMode::Compat => {
                    result.warnings.push(msg);
                }
            }
        }
    }

    result
}

/// Scan `text` for known canonical-term violations and return structured
/// spans.
///
/// Matching uses simple word-boundary rules: a candidate match is only
/// accepted when the characters immediately before and after the legacy
/// term are not part of a typical identifier (letters, digits, `_`, `-`).
/// This prevents false positives from substrings embedded in identifiers
/// (e.g. `VRuleset` or `my-v-celery`).
pub fn lint_text(text: &str, mode: CanonicalMode) -> Vec<CanonicalViolation> {
    let severity = match mode {
        CanonicalMode::Strict => CanonicalSeverity::Error,
        CanonicalMode::Compat => CanonicalSeverity::Warning,
    };

    let bytes = text.as_bytes();
    let mut violations = Vec::new();

    for (legacy, canonical) in LEGACY_TERMS {
        let needle = legacy.as_bytes();
        if needle.is_empty() {
            continue;
        }
        let mut search_start = 0usize;
        while let Some(rel) = find_subsequence(&bytes[search_start..], needle) {
            let start = search_start + rel;
            let end = start + needle.len();

            if is_boundary_match(bytes, start, end) {
                violations.push(CanonicalViolation {
                    span: (start, end),
                    legacy_term: (*legacy).to_string(),
                    suggested_replacement: (*canonical).to_string(),
                    severity,
                });
            }
            // Advance past this match to continue scanning
            search_start = start + 1;
        }
    }

    // Stable ordering: by span start, then by legacy term for determinism
    violations.sort_by(|a, b| {
        a.span
            .0
            .cmp(&b.span.0)
            .then_with(|| a.legacy_term.cmp(&b.legacy_term))
    });
    violations
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Returns true if the match at `bytes[start..end]` is not embedded inside
/// a larger identifier-like token. Treats ASCII letters/digits/`_` as
/// identifier characters. Hyphens are intentionally NOT treated as identifier
/// characters so that matches like `v-cel` (which contain a hyphen) still
/// work when surrounded by non-identifier characters.
fn is_boundary_match(bytes: &[u8], start: usize, end: usize) -> bool {
    let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
    let after_ok = end >= bytes.len() || !is_ident_byte(bytes[end]);
    before_ok && after_ok
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn strict_rejects_legacy_terms() {
        let r = check_terms("language: v-cel", ValidationMode::Strict);
        assert!(!r.passed);
        assert!(!r.errors.is_empty());
    }

    #[test]
    fn compat_warns_but_passes() {
        let r = check_terms("activity_type: VRule", ValidationMode::Compat);
        assert!(r.passed);
        assert!(!r.warnings.is_empty());
    }

    #[test]
    fn canonical_terms_pass_strict() {
        let r = check_terms(
            "language: vil-expr\nactivity_type: Rule",
            ValidationMode::Strict,
        );
        assert!(r.passed);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn vxapp_detected_in_strict() {
        let r = check_terms("kind: VxApp", ValidationMode::Strict);
        assert!(!r.passed);
    }

    // ── lint_text tests ──────────────────────────────────────────────────

    #[test]
    fn lint_text_detects_v_cel() {
        let text = "language: v-cel";
        let v = lint_text(text, CanonicalMode::Strict);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].legacy_term, "v-cel");
        assert_eq!(v[0].suggested_replacement, "vil-expr");
        assert_eq!(v[0].severity, CanonicalSeverity::Error);
        let (s, e) = v[0].span;
        assert_eq!(&text[s..e], "v-cel");
    }

    #[test]
    fn lint_text_detects_vrule() {
        let text = "activity_type: VRule";
        let v = lint_text(text, CanonicalMode::Strict);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].legacy_term, "VRule");
        let (s, e) = v[0].span;
        assert_eq!(&text[s..e], "VRule");
    }

    #[test]
    fn lint_text_detects_vxapp() {
        let text = "kind: VxApp";
        let v = lint_text(text, CanonicalMode::Strict);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].legacy_term, "VxApp");
        assert_eq!(v[0].suggested_replacement, "VilServer");
    }

    #[test]
    fn lint_text_clean_input_has_no_violations() {
        let v = lint_text(
            "language: vil-expr\nactivity_type: Rule\nkind: VilServer",
            CanonicalMode::Strict,
        );
        assert!(v.is_empty());
    }

    #[test]
    fn lint_text_strict_vs_compat_severity() {
        let text = "kind: VxApp and language: v-cel";
        let strict = lint_text(text, CanonicalMode::Strict);
        let compat = lint_text(text, CanonicalMode::Compat);
        assert_eq!(strict.len(), compat.len());
        assert!(
            strict
                .iter()
                .all(|v| v.severity == CanonicalSeverity::Error)
        );
        assert!(
            compat
                .iter()
                .all(|v| v.severity == CanonicalSeverity::Warning)
        );
    }

    #[test]
    fn lint_text_span_slices_back_to_term() {
        let text = "prefix VRule middle v-cel suffix VxApp end";
        let v = lint_text(text, CanonicalMode::Strict);
        assert_eq!(v.len(), 3);
        for vio in &v {
            let (s, e) = vio.span;
            assert_eq!(&text[s..e], vio.legacy_term);
        }
    }

    #[test]
    fn lint_text_respects_identifier_boundaries() {
        // VRuleset contains VRule, but VRule is embedded in a larger identifier
        // → should NOT match. Same for v-celery (v-cel followed by `ery`) and
        // VxAppConfig (VxApp followed by `Config`).
        let text = "VRuleset v-celery VxAppConfig";
        let v = lint_text(text, CanonicalMode::Strict);
        assert!(
            v.is_empty(),
            "expected no violations for identifier-embedded terms, got {v:?}"
        );
    }

    #[test]
    fn lint_text_handles_multiple_occurrences() {
        let text = "VRule one\nVRule two\nVRule three";
        let v = lint_text(text, CanonicalMode::Strict);
        assert_eq!(v.len(), 3);
        // Spans must be monotonically non-decreasing (sorted by start)
        for w in v.windows(2) {
            assert!(w[0].span.0 < w[1].span.0);
        }
    }

    #[test]
    fn lint_text_term_at_start_and_end() {
        let start_text = "VRule at the start";
        let v = lint_text(start_text, CanonicalMode::Strict);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].span, (0, 5));

        let end_text = "ends with VxApp";
        let v = lint_text(end_text, CanonicalMode::Strict);
        assert_eq!(v.len(), 1);
        let (s, e) = v[0].span;
        assert_eq!(&end_text[s..e], "VxApp");
        assert_eq!(e, end_text.len());
    }
}
