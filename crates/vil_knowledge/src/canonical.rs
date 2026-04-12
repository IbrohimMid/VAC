//! Canonical VIL term checker.
//! Validates that artifacts use canonical terminology, not VFlow-era legacy aliases.

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

#[derive(Debug, Default)]
pub struct CanonicalCheckResult {
    pub passed: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Check a string for legacy VIL terms.
/// In Strict mode, any legacy term is an error.
/// In Compat mode, legacy terms produce warnings only.
pub fn check_terms(content: &str, mode: ValidationMode) -> CanonicalCheckResult {
    let mut result = CanonicalCheckResult { passed: true, warnings: vec![], errors: vec![] };

    for (legacy, canonical) in LEGACY_TERMS {
        if content.contains(legacy) {
            let msg = format!("legacy alias '{}' detected; use '{}' instead", legacy, canonical);
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

#[cfg(test)]
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
        let r = check_terms("language: vil-expr\nactivity_type: Rule", ValidationMode::Strict);
        assert!(r.passed);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn vxapp_detected_in_strict() {
        let r = check_terms("kind: VxApp", ValidationMode::Strict);
        assert!(!r.passed);
    }
}
