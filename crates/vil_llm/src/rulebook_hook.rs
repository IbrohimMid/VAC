//! Rulebook hook — stub for per-tool model preference injection.
//!
//! The rulebook (future unit) will supply a function that, given a tool name,
//! may return a preferred provider override. Until that unit lands this is a
//! stub with a default implementation that always returns `None`, so the
//! router falls back to its configured routing map + default provider.

use std::sync::Arc;

/// Callback signature: `tool_name -> Option<provider_name>`.
pub type PreferredModelFn = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Rulebook integration surface consulted by the router before its static map.
#[derive(Clone)]
pub struct RulebookContext {
    preferred_model: PreferredModelFn,
}

impl RulebookContext {
    /// Construct from a user-supplied preference function.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&str) -> Option<String> + Send + Sync + 'static,
    {
        Self {
            preferred_model: Arc::new(f),
        }
    }

    /// Default: no preferences. Router should treat this as "no override".
    pub fn empty() -> Self {
        Self::new(|_tool| None)
    }

    /// Ask the rulebook for a provider override for the given tool.
    pub fn preferred_model(&self, tool_name: &str) -> Option<String> {
        (self.preferred_model)(tool_name)
    }
}

impl Default for RulebookContext {
    fn default() -> Self {
        Self::empty()
    }
}

impl std::fmt::Debug for RulebookContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RulebookContext").finish_non_exhaustive()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn default_returns_none_for_any_tool() {
        let ctx = RulebookContext::default();
        assert!(ctx.preferred_model("anything").is_none());
        assert!(ctx.preferred_model("Grep").is_none());
    }

    #[test]
    fn custom_function_is_consulted() {
        let ctx = RulebookContext::new(|tool| {
            if tool == "Plan" {
                Some("anthropic".to_string())
            } else {
                None
            }
        });
        assert_eq!(ctx.preferred_model("Plan"), Some("anthropic".into()));
        assert_eq!(ctx.preferred_model("Grep"), None);
    }
}
