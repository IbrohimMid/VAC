//! Secret Detection
//!
//! Detects sensitive information like API keys, credentials, tokens, and PII.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Types of secrets that can be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretType {
    AwsAccessKey,
    AwsSecretKey,
    BearerToken,
    GitHubToken,
    GitHubFineGrainedToken,
    GitLabToken,
    SlackToken,
    StripeSecretKey,
    OpenAiKey,
    AnthropicKey,
    GoogleApiKey,
    Jwt,
    PemPrivateKey,
    SshPrivateKey,
    GenericSecret,
}

/// PII categories that should be redacted but are not secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PiiType {
    Email,
    Url,
    IpAddress,
}

/// A finding detected by the secret detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DetectionKind {
    Secret(SecretType),
    Pii(PiiType),
}

/// A detected secret or PII value.
#[derive(Debug, Clone)]
pub struct DetectedSecret {
    pub kind: DetectionKind,
    pub value: String,
    pub start: usize,
    pub end: usize,
}

impl DetectedSecret {
    fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

#[derive(Debug, Clone)]
struct PatternRule {
    kind: DetectionKind,
    regex: Regex,
    capture_group: usize,
}

impl PatternRule {
    #[allow(clippy::unwrap_used)] // Compile-time-constant regexes; panic at init is intentional.
    fn new(kind: DetectionKind, pattern: &str, capture_group: usize) -> Self {
        Self {
            kind,
            regex: Regex::new(pattern).unwrap(),
            capture_group,
        }
    }

    fn push_matches(&self, text: &str, out: &mut Vec<DetectedSecret>) {
        if self.capture_group == 0 {
            for mat in self.regex.find_iter(text) {
                out.push(DetectedSecret {
                    kind: self.kind,
                    value: mat.as_str().to_string(),
                    start: mat.start(),
                    end: mat.end(),
                });
            }
            return;
        }

        for caps in self.regex.captures_iter(text) {
            let Some(mat) = caps.get(self.capture_group) else {
                continue;
            };
            out.push(DetectedSecret {
                kind: self.kind,
                value: mat.as_str().to_string(),
                start: mat.start(),
                end: mat.end(),
            });
        }
    }
}

/// Secret detector using structured rules plus entropy-based generic detection.
pub struct SecretDetector {
    patterns: Vec<PatternRule>,
}

impl Default for SecretDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretDetector {
    pub fn new() -> Self {
        let patterns = vec![
            PatternRule::new(
                DetectionKind::Secret(SecretType::AwsAccessKey),
                r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::AwsSecretKey),
                r#"(?i)\b(?:aws[_-]?secret(?:[_-]?access)?[_-]?key|secret[_-]?access[_-]?key)\b\s*[:=]\s*['"]?([A-Za-z0-9/+=]{40})['"]?"#,
                1,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::GitHubToken),
                r"\bgh[opsur]_[A-Za-z0-9]{20,}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::GitHubFineGrainedToken),
                r"\bgithub_pat_[A-Za-z0-9_]{20,}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::GitLabToken),
                r"\bglpat-[A-Za-z0-9_-]{20,}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::SlackToken),
                r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::StripeSecretKey),
                r"\bsk_(?:live|test)_[A-Za-z0-9]{20,}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::OpenAiKey),
                r"\bsk-[A-Za-z0-9]{20,}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::AnthropicKey),
                r"\bsk-ant-[A-Za-z0-9-]{20,}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::GoogleApiKey),
                r"\bAIza[0-9A-Za-z\-_]{35}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::BearerToken),
                r#"(?i)\bbearer\s+([A-Za-z0-9._\-]{20,})\b"#,
                1,
            ),
            PatternRule::new(
                DetectionKind::Secret(SecretType::Jwt),
                r"\b[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Pii(PiiType::Email),
                r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b",
                0,
            ),
            PatternRule::new(
                DetectionKind::Pii(PiiType::Url),
                r#"https?://[^\s<>"']+"#,
                0,
            ),
            PatternRule::new(
                DetectionKind::Pii(PiiType::IpAddress),
                r"\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b",
                0,
            ),
        ];

        Self { patterns }
    }

    /// Detect all secrets and PII in text.
    pub fn detect(&self, text: &str) -> Vec<DetectedSecret> {
        let mut findings = Vec::new();

        for pattern in &self.patterns {
            pattern.push_matches(text, &mut findings);
        }

        detect_jwt_candidates(text, &mut findings);
        detect_private_key_blocks(text, &mut findings);
        detect_high_entropy_tokens(text, &mut findings);

        dedup_overlapping(findings)
    }

    /// Check if text contains any secrets or PII.
    pub fn contains_secrets(&self, text: &str) -> bool {
        !self.detect(text).is_empty()
    }
}

/// Global detector instance.
static DETECTOR: OnceLock<SecretDetector> = OnceLock::new();

/// Get global detector instance.
pub fn get_detector() -> &'static SecretDetector {
    DETECTOR.get_or_init(SecretDetector::new)
}

#[allow(clippy::unwrap_used)] // Compile-time-constant regex in OnceLock; panic at init is intentional.
fn detect_jwt_candidates(text: &str, out: &mut Vec<DetectedSecret>) {
    static JWT_RE: OnceLock<Regex> = OnceLock::new();
    let regex = JWT_RE.get_or_init(|| {
        Regex::new(r"\b[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b").unwrap()
    });

    for mat in regex.find_iter(text) {
        let token = mat.as_str();
        if !looks_like_jwt(token) {
            continue;
        }
        out.push(DetectedSecret {
            kind: DetectionKind::Secret(SecretType::Jwt),
            value: token.to_string(),
            start: mat.start(),
            end: mat.end(),
        });
    }
}

fn detect_private_key_blocks(text: &str, out: &mut Vec<DetectedSecret>) {
    let mut search_from = 0;
    let begin_marker = "-----BEGIN ";
    let end_marker_suffix = "-----END ";
    let line_suffix = "-----";

    while let Some(begin_rel) = text[search_from..].find(begin_marker) {
        let begin = search_from + begin_rel;
        let label_start = begin + begin_marker.len();
        let Some(label_end_rel) = text[label_start..].find(line_suffix) else {
            break;
        };
        let label_end = label_start + label_end_rel;
        let label = &text[label_start..label_end];
        if !label.contains("PRIVATE KEY") {
            search_from = label_end;
            continue;
        }

        let end_marker = format!("{end_marker_suffix}{label}{line_suffix}");
        let scan_from = label_end;
        let Some(end_rel) = text[scan_from..].find(&end_marker) else {
            search_from = label_end;
            continue;
        };
        let end = scan_from + end_rel + end_marker.len();
        let kind = if label.contains("OPENSSH PRIVATE KEY") {
            DetectionKind::Secret(SecretType::SshPrivateKey)
        } else {
            DetectionKind::Secret(SecretType::PemPrivateKey)
        };

        out.push(DetectedSecret {
            kind,
            value: text[begin..end].to_string(),
            start: begin,
            end,
        });

        search_from = end;
    }
}

#[allow(clippy::unwrap_used)] // Compile-time-constant regex in OnceLock; panic at init is intentional.
fn detect_high_entropy_tokens(text: &str, out: &mut Vec<DetectedSecret>) {
    static CANDIDATE_RE: OnceLock<Regex> = OnceLock::new();
    let regex = CANDIDATE_RE.get_or_init(|| Regex::new(r"\b[A-Za-z0-9+/=_-]{20,}\b").unwrap());

    for mat in regex.find_iter(text) {
        let candidate = mat.as_str();
        if looks_benign(candidate) {
            continue;
        }

        let entropy = shannon_entropy(candidate);
        if entropy < 4.5 {
            continue;
        }

        out.push(DetectedSecret {
            kind: DetectionKind::Secret(SecretType::GenericSecret),
            value: candidate.to_string(),
            start: mat.start(),
            end: mat.end(),
        });
    }
}

fn looks_like_jwt(token: &str) -> bool {
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature)) = (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if parts.next().is_some() || header.is_empty() || payload.is_empty() || signature.is_empty() {
        return false;
    }

    let decode = |segment: &str| URL_SAFE_NO_PAD.decode(segment.as_bytes()).ok();
    let Some(header_bytes) = decode(header) else {
        return false;
    };
    let Some(payload_bytes) = decode(payload) else {
        return false;
    };

    let header_text = String::from_utf8_lossy(&header_bytes);
    let payload_text = String::from_utf8_lossy(&payload_bytes);
    header_text.trim_start().starts_with('{') && payload_text.trim_start().starts_with('{')
}

fn looks_benign(candidate: &str) -> bool {
    if candidate.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if is_uuid(candidate) {
        return true;
    }
    if is_hex_digest(candidate) {
        return true;
    }
    if is_version_like(candidate) {
        return true;
    }
    if candidate.len() < 24 {
        return true;
    }
    if character_classes(candidate) < 2 {
        return true;
    }
    false
}

fn character_classes(candidate: &str) -> usize {
    let mut classes = 0;
    if candidate.chars().any(|c| c.is_ascii_lowercase()) {
        classes += 1;
    }
    if candidate.chars().any(|c| c.is_ascii_uppercase()) {
        classes += 1;
    }
    if candidate.chars().any(|c| c.is_ascii_digit()) {
        classes += 1;
    }
    if candidate
        .chars()
        .any(|c| matches!(c, '+' | '/' | '=' | '_' | '-'))
    {
        classes += 1;
    }
    classes
}

fn is_uuid(candidate: &str) -> bool {
    let bytes = candidate.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (idx, ch) in bytes.iter().enumerate() {
        match idx {
            8 | 13 | 18 | 23 => {
                if *ch != b'-' {
                    return false;
                }
            }
            _ => {
                if !ch.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

fn is_hex_digest(candidate: &str) -> bool {
    let len = candidate.len();
    len >= 24 && candidate.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_version_like(candidate: &str) -> bool {
    let mut parts = candidate.split('.');
    let mut seen = 0usize;
    for part in parts.by_ref() {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        seen += 1;
    }
    seen >= 3
}

fn shannon_entropy(candidate: &str) -> f64 {
    let len = candidate.chars().count() as f64;
    if len == 0.0 {
        return 0.0;
    }

    let mut counts = HashMap::new();
    for ch in candidate.chars() {
        *counts.entry(ch).or_insert(0usize) += 1;
    }

    counts
        .values()
        .map(|count| {
            let p = *count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

fn kind_score(kind: &DetectionKind) -> u8 {
    match kind {
        DetectionKind::Secret(SecretType::GenericSecret) => 100,
        DetectionKind::Secret(SecretType::BearerToken) => 5,
        DetectionKind::Secret(SecretType::Jwt) => 4,
        DetectionKind::Pii(_) => 50,
        DetectionKind::Secret(_) => 0,
    }
}

fn dedup_overlapping(mut findings: Vec<DetectedSecret>) -> Vec<DetectedSecret> {
    if findings.is_empty() {
        return findings;
    }

    findings.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| b.end.cmp(&a.end))
            .then_with(|| kind_score(&a.kind).cmp(&kind_score(&b.kind)))
    });

    let mut out = Vec::with_capacity(findings.len());
    let mut group: Vec<DetectedSecret> = Vec::new();
    let mut group_end = 0usize;

    for finding in findings {
        if group.is_empty() {
            group_end = finding.end;
            group.push(finding);
            continue;
        }

        if finding.start < group_end {
            group_end = group_end.max(finding.end);
            group.push(finding);
        } else {
            out.push(pick_best(&mut group));
            group.clear();
            group_end = finding.end;
            group.push(finding);
        }
    }

    if !group.is_empty() {
        out.push(pick_best(&mut group));
    }

    out.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.end.cmp(&b.end)));
    out
}

fn pick_best(group: &mut Vec<DetectedSecret>) -> DetectedSecret {
    let mut best_idx = 0usize;
    for idx in 1..group.len() {
        let candidate = &group[idx];
        let best = &group[best_idx];
        let candidate_score = kind_score(&candidate.kind);
        let best_score = kind_score(&best.kind);
        let candidate_len = candidate.len();
        let best_len = best.len();
        if candidate_score < best_score
            || (candidate_score == best_score && candidate_len > best_len)
            || (candidate_score == best_score
                && candidate_len == best_len
                && candidate.start < best.start)
        {
            best_idx = idx;
        }
    }
    group.swap_remove(best_idx)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    fn jwt_token(idx: usize) -> String {
        let header =
            URL_SAFE_NO_PAD.encode(format!(r#"{{"alg":"HS256","typ":"JWT","kid":"{}"}}"#, idx));
        let payload =
            URL_SAFE_NO_PAD.encode(format!(r#"{{"sub":"user-{}","scope":"repo:write"}}"#, idx));
        let signature = URL_SAFE_NO_PAD.encode(format!("signature-{idx}"));
        format!("{header}.{payload}.{signature}")
    }

    #[test]
    fn pii_are_classified_separately() {
        let detector = SecretDetector::new();
        let text = "Contact user@example.com at https://example.com or 10.0.0.1";
        let findings = detector.detect(text);

        assert!(
            findings
                .iter()
                .any(|f| matches!(f.kind, DetectionKind::Pii(PiiType::Email)))
        );
        assert!(
            findings
                .iter()
                .any(|f| matches!(f.kind, DetectionKind::Pii(PiiType::Url)))
        );
        assert!(
            findings
                .iter()
                .any(|f| matches!(f.kind, DetectionKind::Pii(PiiType::IpAddress)))
        );
        assert!(
            findings
                .iter()
                .all(|f| !matches!(f.kind, DetectionKind::Secret(SecretType::GenericSecret)))
        );
    }

    #[test]
    fn detects_private_keys_and_jwts_without_overlap_duplicates() {
        let detector = SecretDetector::new();
        let text = format!(
            "Authorization: Bearer {}\n{}\n",
            jwt_token(1),
            "-----BEGIN OPENSSH PRIVATE KEY-----\nssh-private-key-body\n-----END OPENSSH PRIVATE KEY-----"
        );

        let findings = detector.detect(&text);
        let secret_count = findings
            .iter()
            .filter(|f| matches!(f.kind, DetectionKind::Secret(_)))
            .count();

        assert_eq!(secret_count, 2);
        assert!(
            findings
                .iter()
                .any(|f| matches!(f.kind, DetectionKind::Secret(SecretType::Jwt)))
        );
        assert!(
            findings
                .iter()
                .any(|f| matches!(f.kind, DetectionKind::Secret(SecretType::SshPrivateKey)))
        );
        assert!(findings.windows(2).all(|pair| pair[0].end <= pair[1].start));
    }

    #[test]
    fn adversarial_secret_corpus_hits_true_positives_and_avoids_false_positives() {
        let detector = SecretDetector::new();
        let mut positives = Vec::new();

        for idx in 0..7 {
            positives.push(format!("ghp_{}{}", "A".repeat(35), idx));
            positives.push(format!("github_pat_{}{}", "a".repeat(22), idx));
            positives.push(format!("xoxb-{}{}", "A".repeat(24), idx));
            positives.push(format!("sk-{}{}", "B".repeat(24), idx));
            positives.push(format!("sk-ant-{}{}", "C".repeat(24), idx));
            positives.push(format!("AIza{}", "D".repeat(35)));
            positives.push(format!(
                "AKIA{}{}",
                "E".repeat(15),
                char::from(b'A' + idx as u8)
            ));
            positives.push(format!("sk_live_{}{}", "F".repeat(24), idx));
        }

        for idx in 0..10 {
            positives.push(jwt_token(idx));
        }

        let mut true_positives = 0usize;
        let mut false_negatives = 0usize;
        for sample in &positives {
            let findings = detector.detect(sample);
            if findings
                .iter()
                .any(|f| matches!(f.kind, DetectionKind::Secret(_)))
            {
                true_positives += 1;
            } else {
                false_negatives += 1;
            }
        }

        let mut negatives = Vec::new();
        for idx in 0..25 {
            negatives.push(format!("v{}.{}.{}", idx, idx + 1, idx + 2));
            negatives.push(format!("https://example.com/v{}", idx));
            negatives.push(format!("user{}@example.com", idx));
            negatives.push(format!("Release candidate {} for build {}", idx, idx + 1));
        }

        let mut false_positive_hits = 0usize;
        for sample in &negatives {
            let findings = detector.detect(sample);
            if findings
                .iter()
                .any(|f| matches!(f.kind, DetectionKind::Secret(_)))
            {
                false_positive_hits += 1;
            }
        }

        let precision = true_positives as f64 / (true_positives + false_positive_hits) as f64;
        let recall = true_positives as f64 / (true_positives + false_negatives) as f64;

        assert_eq!(true_positives, positives.len());
        assert_eq!(false_negatives, 0);
        assert_eq!(false_positive_hits, 0);
        assert!(
            precision >= 0.9,
            "expected precision >= 0.9, got {precision:.3}"
        );
        assert!(recall >= 0.95, "expected recall >= 0.95, got {recall:.3}");
    }

    #[test]
    fn secret_substitution_uses_detected_ranges() {
        let mut sub = super::super::secret_substitution::SecretSubstitution::new();
        let original = "API_KEY=sk_test_1234567890abcdef contact user@example.com";
        let substituted = sub.substitute(original);
        assert!(substituted.contains("[SECRET_"));
        assert!(!substituted.contains("sk_test_1234567890abcdef"));
        assert!(!substituted.contains("user@example.com"));

        let restored = sub.restore(&substituted);
        assert!(restored.contains("sk_test_1234567890abcdef"));
        assert!(restored.contains("user@example.com"));
    }

    #[test]
    fn adversarial_mutant_tests() {
        let detector = SecretDetector::new();

        // contains_secrets
        assert!(detector.contains_secrets("sk-12345678901234567890"));
        assert!(!detector.contains_secrets("just some normal text without secrets"));

        // PatternRule push_matches & capture_group logic
        let mut findings = Vec::new();
        let rule_no_capture = PatternRule::new(
            DetectionKind::Pii(PiiType::Email),
            r"\b[a-z]+@[a-z]+\.[a-z]+\b",
            0,
        );
        rule_no_capture.push_matches("test test@test.com test", &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].value, "test@test.com");

        let mut findings = Vec::new();
        let rule_capture = PatternRule::new(
            DetectionKind::Secret(SecretType::BearerToken),
            r"(?i)\bbearer\s+([A-Za-z0-9._\-]{20,})\b",
            1,
        );
        rule_capture.push_matches("Bearer 12345678901234567890 abc", &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].value, "12345678901234567890");

        // looks_like_jwt branches
        // Valid JWT
        let valid_jwt = jwt_token(99);
        assert!(looks_like_jwt(&valid_jwt));
        // Missing parts
        assert!(!looks_like_jwt("header.payload"));
        // Extra parts
        assert!(!looks_like_jwt("header.payload.signature.extra"));
        // Empty parts
        assert!(!looks_like_jwt(".payload.signature"));
        assert!(!looks_like_jwt("header..signature"));
        assert!(!looks_like_jwt("header.payload."));
        // Invalid base64
        assert!(!looks_like_jwt("invalid!header.payload.signature"));
        assert!(!looks_like_jwt(&format!(
            "{}.invalid!payload.signature",
            URL_SAFE_NO_PAD.encode(b"{}")
        )));
        // Valid base64 but not JSON {
        assert!(!looks_like_jwt(&format!(
            "{}.{}.signature",
            URL_SAFE_NO_PAD.encode(b"notjson"),
            URL_SAFE_NO_PAD.encode(b"{}")
        )));
        assert!(!looks_like_jwt(&format!(
            "{}.{}.signature",
            URL_SAFE_NO_PAD.encode(b"{}"),
            URL_SAFE_NO_PAD.encode(b"notjson")
        )));

        // detect_private_key_blocks branches
        // Missing end marker
        let no_end = "-----BEGIN RSA PRIVATE KEY-----\nbody\n";
        let mut f = Vec::new();
        detect_private_key_blocks(no_end, &mut f);
        assert!(f.is_empty());

        // Missing line suffix after BEGIN
        let no_suffix = "-----BEGIN RSA PRIVATE KEY body";
        let mut f = Vec::new();
        detect_private_key_blocks(no_suffix, &mut f);
        assert!(f.is_empty());

        // Not a private key
        let not_private = "-----BEGIN CERTIFICATE-----\nbody\n-----END CERTIFICATE-----";
        let mut f = Vec::new();
        detect_private_key_blocks(not_private, &mut f);
        assert!(f.is_empty());

        // PEM vs SSH
        let pem_key = "-----BEGIN RSA PRIVATE KEY-----\nbody\n-----END RSA PRIVATE KEY-----";
        let mut f = Vec::new();
        detect_private_key_blocks(pem_key, &mut f);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].kind, DetectionKind::Secret(SecretType::PemPrivateKey));

        let ssh_key =
            "-----BEGIN OPENSSH PRIVATE KEY-----\nbody\n-----END OPENSSH PRIVATE KEY-----";
        let mut f = Vec::new();
        detect_private_key_blocks(ssh_key, &mut f);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].kind, DetectionKind::Secret(SecretType::SshPrivateKey));

        // detect_high_entropy_tokens & looks_benign branches
        // Benign: all digits
        assert!(looks_benign("123456789012345678901234"));
        // Benign: uuid
        assert!(looks_benign("123e4567-e89b-12d3-a456-426614174000"));
        // Benign: hex digest
        assert!(looks_benign("abcdef1234567890abcdef12"));
        // Benign: version like
        assert!(looks_benign("1.2.3.45678901234567890123456"));
        // Benign: len < 24
        assert!(looks_benign("short_string"));
        // Benign: character classes < 2 (e.g. only lowercase)
        assert!(looks_benign("aaaaaaaaaaaaaaaaaaaaaaaaa"));
        // Not benign, but low entropy (e.g. many repeated chars)
        let mut f = Vec::new();
        detect_high_entropy_tokens("AaAaAaAaAaAaAaAaAaAaAaAa", &mut f);
        assert!(f.is_empty()); // Entropy < 4.5

        // High entropy, not benign -> detected
        let mut f = Vec::new();
        // A string with 24 distinct characters
        detect_high_entropy_tokens("AbCdEfGhIjKlMnOpQrStUvWx", &mut f);
        assert_eq!(f.len(), 1);

        // is_uuid branches
        assert!(!is_uuid("123e4567-e89b-12d3-a456-42661417400")); // len != 36
        assert!(!is_uuid("123e4567-e89b-12d3-a456X426614174000")); // wrong dash
        assert!(!is_uuid("123e4567-e89b-12d3-a456-42661417400g")); // non hex

        // is_hex_digest branches
        assert!(!is_hex_digest("abcdef1234567890abcdef1")); // < 24
        assert!(!is_hex_digest("abcdef1234567890abcdef1g")); // non hex

        // is_version_like branches
        assert!(!is_version_like("1.2.a")); // non digit
        assert!(!is_version_like("1.2.")); // empty part
        assert!(!is_version_like("1.2")); // < 3 parts

        // character_classes branches
        assert_eq!(character_classes("aA1+"), 4);
        assert_eq!(character_classes("a"), 1);

        // shannon_entropy branches
        assert_eq!(shannon_entropy(""), 0.0);

        // kind_score & dedup_overlapping
        let s1 = DetectedSecret {
            kind: DetectionKind::Secret(SecretType::GenericSecret), // score 100
            value: "1".to_string(),
            start: 0,
            end: 10,
        };
        let s2 = DetectedSecret {
            kind: DetectionKind::Pii(PiiType::Email), // score 50
            value: "2".to_string(),
            start: 0,
            end: 10,
        };
        let s3 = DetectedSecret {
            kind: DetectionKind::Secret(SecretType::Jwt), // score 4
            value: "3".to_string(),
            start: 0,
            end: 10,
        };
        let s4 = DetectedSecret {
            kind: DetectionKind::Secret(SecretType::BearerToken), // score 5
            value: "4".to_string(),
            start: 5,
            end: 15,
        };
        let s5 = DetectedSecret {
            kind: DetectionKind::Secret(SecretType::AwsAccessKey), // score 0
            value: "5".to_string(),
            start: 20,
            end: 30,
        };
        let findings = vec![s1.clone(), s2.clone(), s3.clone(), s4.clone(), s5.clone()];
        let deduped = dedup_overlapping(findings);
        // group 1: s1, s2, s3, s4. Best is s1 (score 100, wait. pick_best looks for min score?
        // Let's check pick_best logic:
        // `if candidate_score < best_score ...` => smaller score is better.
        // Jwt is 4, BearerToken is 5, PII is 50, Generic is 100, AwsAccessKey is 0.
        // Among s1(100), s2(50), s3(4), s4(5), min score is s3 (4).
        // Then s5(0) is separate.
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].kind, DetectionKind::Secret(SecretType::Jwt));
        assert_eq!(
            deduped[1].kind,
            DetectionKind::Secret(SecretType::AwsAccessKey)
        );

        // Test pick_best tie-breakers (same score, diff length)
        let s_short = DetectedSecret {
            kind: DetectionKind::Secret(SecretType::AwsAccessKey),
            value: "short".to_string(),
            start: 0,
            end: 5,
        };
        let s_long = DetectedSecret {
            kind: DetectionKind::Secret(SecretType::AwsAccessKey),
            value: "longer".to_string(),
            start: 0,
            end: 6,
        };
        let deduped2 = dedup_overlapping(vec![s_short, s_long.clone()]);
        assert_eq!(deduped2.len(), 1);
        assert_eq!(deduped2[0].end, 6); // prefers longer

        // Test pick_best tie-breakers (same score, same length, diff start)
        let s_late = DetectedSecret {
            kind: DetectionKind::Secret(SecretType::AwsAccessKey),
            value: "same1".to_string(),
            start: 1,
            end: 6,
        };
        let s_early = DetectedSecret {
            kind: DetectionKind::Secret(SecretType::AwsAccessKey),
            value: "same2".to_string(),
            start: 0,
            end: 5,
        };
        // Dedup overlapping will put them in same group if start < group_end
        let deduped3 = dedup_overlapping(vec![s_early.clone(), s_late]);
        assert_eq!(deduped3.len(), 1);
        assert_eq!(deduped3[0].start, 0); // prefers earlier

        // DetectedSecret len method
        assert_eq!(s_long.len(), 6);
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn detector_does_not_panic_on_arbitrary_strings(ref s in "\\PC*") {
            let detector = SecretDetector::new();
            // Just ensure it does not panic and finishes
            let _ = detector.detect(s);
            let _ = detector.contains_secrets(s);
        }
    }
}
