//! Retry logic with exponential backoff and Retry-After header parsing.
//! Adapted from stakpak/libs/agent-core/src/retry.rs (Apache-2.0).

use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Retry configuration.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: usize,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 2_000,
            max_backoff_ms: 30_000,
            multiplier: 2.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryDelay {
    pub delay_ms: u64,
    pub source: RetryDelaySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDelaySource {
    RetryAfterMsHeader,
    RetryAfterHeader,
    ExponentialBackoff,
}

/// Parse retry delay from provider response headers.
/// Precedence: `retry-after-ms` > `retry-after` (seconds or HTTP date).
pub fn parse_retry_delay_from_headers(
    headers: &HashMap<String, String>,
    now: DateTime<Utc>,
) -> Option<RetryDelay> {
    if let Some(raw_ms) = find_header(headers, "retry-after-ms") {
        if let Ok(delay_ms) = raw_ms.trim().parse::<u64>() {
            return Some(RetryDelay { delay_ms, source: RetryDelaySource::RetryAfterMsHeader });
        }
    }

    let raw = find_header(headers, "retry-after")?;
    let raw = raw.trim();

    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(RetryDelay {
            delay_ms: seconds.saturating_mul(1_000),
            source: RetryDelaySource::RetryAfterHeader,
        });
    }

    let date = DateTime::parse_from_rfc2822(raw).ok()?;
    let diff_ms = (date.with_timezone(&Utc) - now).num_milliseconds();
    Some(RetryDelay {
        delay_ms: diff_ms.max(0) as u64,
        source: RetryDelaySource::RetryAfterHeader,
    })
}

/// Compute exponential backoff delay for `attempt` (1-indexed).
pub fn exponential_backoff_ms(config: &RetryConfig, attempt: usize) -> u64 {
    if attempt <= 1 {
        return config.initial_backoff_ms.min(config.max_backoff_ms);
    }
    let factor = config.multiplier.powi((attempt - 1) as i32);
    let delay = (config.initial_backoff_ms as f64) * factor;
    if delay.is_nan() || delay.is_sign_negative() {
        return config.initial_backoff_ms.min(config.max_backoff_ms);
    }
    delay.min(config.max_backoff_ms as f64) as u64
}

/// Resolve retry delay: use header if present, else exponential backoff.
pub fn resolve_retry_delay_ms(
    headers: &HashMap<String, String>,
    config: &RetryConfig,
    attempt: usize,
    now: DateTime<Utc>,
) -> RetryDelay {
    parse_retry_delay_from_headers(headers, now).unwrap_or_else(|| RetryDelay {
        delay_ms: exponential_backoff_ms(config, attempt),
        source: RetryDelaySource::ExponentialBackoff,
    })
}

fn find_header<'a>(headers: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    headers.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn cfg() -> RetryConfig { RetryConfig::default() }

    #[test]
    fn retry_after_ms_takes_precedence() {
        let mut h = HashMap::new();
        h.insert("retry-after-ms".into(), "1500".into());
        h.insert("retry-after".into(), "20".into());
        let r = parse_retry_delay_from_headers(&h, Utc::now()).unwrap();
        assert_eq!(r.delay_ms, 1500);
        assert_eq!(r.source, RetryDelaySource::RetryAfterMsHeader);
    }

    #[test]
    fn retry_after_seconds() {
        let mut h = HashMap::new();
        h.insert("Retry-After".into(), "3".into());
        let r = parse_retry_delay_from_headers(&h, Utc::now()).unwrap();
        assert_eq!(r.delay_ms, 3000);
    }

    #[test]
    fn exponential_backoff_caps_at_max() {
        let c = cfg();
        assert_eq!(exponential_backoff_ms(&c, 1), 2_000);
        assert_eq!(exponential_backoff_ms(&c, 2), 4_000);
        assert_eq!(exponential_backoff_ms(&c, 3), 8_000);
        assert_eq!(exponential_backoff_ms(&c, 10), 30_000);
    }

    #[test]
    fn fallback_to_backoff_when_no_header() {
        let r = resolve_retry_delay_ms(&HashMap::new(), &cfg(), 3, Utc::now());
        assert_eq!(r.delay_ms, 8_000);
        assert_eq!(r.source, RetryDelaySource::ExponentialBackoff);
    }

    #[test]
    fn retry_after_http_date() {
        let now = Utc::now();
        let target = now + Duration::seconds(5);
        let mut h = HashMap::new();
        h.insert("retry-after".into(), target.to_rfc2822());
        let r = parse_retry_delay_from_headers(&h, now).unwrap();
        assert_eq!(r.source, RetryDelaySource::RetryAfterHeader);
        assert!(r.delay_ms >= 4_000 && r.delay_ms <= 6_000);
    }
}
