//! F7.4 — Token estimation (pre-flight).
//!
//! Gives the UI a cheap "will this fit?" answer before paying a
//! full LLM round-trip. We deliberately avoid pulling `tiktoken-rs`
//! (heavy BPE data tables + ~10 MB binary growth). The estimator
//! uses a character + word blend that is accurate to within ~15%
//! for Latin-script prompts — plenty for a budget warning.
//!
//! Real providers plug in their own tokenizer at call time; this
//! module is only for the operator-facing pre-flight warning.

/// Heuristic constants. 1 token ≈ 4 characters or 0.75 words for
/// English-like text (Anthropic/OpenAI published ranges). We blend
/// the two and take the max to avoid undercounting.
const CHARS_PER_TOKEN: f32 = 4.0;
const WORDS_PER_TOKEN: f32 = 0.75;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenEstimate {
    pub tokens: u32,
    pub chars: u32,
    pub words: u32,
}

impl TokenEstimate {
    pub fn new(text: &str) -> Self {
        let chars = text.chars().count() as u32;
        let words = text.split_whitespace().count() as u32;
        let by_chars = (chars as f32 / CHARS_PER_TOKEN).ceil() as u32;
        let by_words = (words as f32 / WORDS_PER_TOKEN).ceil() as u32;
        Self {
            tokens: by_chars.max(by_words),
            chars,
            words,
        }
    }
}

/// Budget classification. Drivers render this as a colored footer
/// pip (green/yellow/red) next to the token count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BudgetClass {
    /// < 50% of the configured context window.
    Safe,
    /// 50–80% — still usable but worth trimming.
    Warn,
    /// > 80% — high risk of overflow, suggest /compact.
    Danger,
}

impl BudgetClass {
    pub fn classify(estimate: TokenEstimate, context_window: u32) -> Self {
        if context_window == 0 {
            return Self::Warn;
        }
        let pct = estimate.tokens as f32 / context_window as f32;
        if pct < 0.5 {
            Self::Safe
        } else if pct < 0.8 {
            Self::Warn
        } else {
            Self::Danger
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_is_zero_tokens() {
        let e = TokenEstimate::new("");
        assert_eq!(e.tokens, 0);
        assert_eq!(e.chars, 0);
        assert_eq!(e.words, 0);
    }

    #[test]
    fn short_prompt_estimates_nonzero() {
        let e = TokenEstimate::new("hello world");
        assert!(e.tokens > 0);
        assert_eq!(e.words, 2);
        assert_eq!(e.chars, 11);
    }

    #[test]
    fn classify_thresholds_are_monotone() {
        let small = TokenEstimate {
            tokens: 100,
            chars: 400,
            words: 75,
        };
        let mid = TokenEstimate {
            tokens: 1000,
            chars: 4000,
            words: 750,
        };
        let big = TokenEstimate {
            tokens: 1800,
            chars: 7200,
            words: 1350,
        };
        assert_eq!(BudgetClass::classify(small, 2000), BudgetClass::Safe);
        assert_eq!(BudgetClass::classify(mid, 2000), BudgetClass::Warn);
        assert_eq!(BudgetClass::classify(big, 2000), BudgetClass::Danger);
    }

    #[test]
    fn zero_context_window_degrades_to_warn() {
        let e = TokenEstimate::new("x");
        assert_eq!(BudgetClass::classify(e, 0), BudgetClass::Warn);
    }

    #[test]
    fn long_latin_prompt_is_in_expected_token_band() {
        // "The quick brown fox" × 100 → ~1900 chars, ~500 words.
        // Expected tokens: ~475 (chars/4) vs ~667 (words/0.75) → 667.
        let text = "The quick brown fox ".repeat(100);
        let e = TokenEstimate::new(&text);
        assert!(e.tokens >= 450 && e.tokens <= 800, "got {}", e.tokens);
    }
}
