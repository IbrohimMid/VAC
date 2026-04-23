//! Agent-strategy boundary (Trae-style ablation surface).
//!
//! The existing `SwarmOrchestrator` is monolithic — every run follows the
//! same tool-selection policy. This module carves out an `AgentStrategy`
//! trait plus two reference implementations so researchers can compare
//! policies without forking the orchestrator.
//!
//! Current status: scaffolding. The orchestrator does not yet consume the
//! trait (that integration lands in a follow-up). Callers who want to
//! experiment can use the impls directly in tests or custom drivers.
//!
//! Contract:
//! - `choose_next` is pure with respect to `StrategyContext`
//! - implementations must be `Send + Sync` so they can be stored behind an
//!   `Arc<dyn AgentStrategy>`

use std::fmt::Debug;

/// Input to the strategy's decision.
#[derive(Debug, Clone)]
pub struct StrategyContext<'a> {
    /// Recent user messages, most-recent last.
    pub recent_user_messages: &'a [String],
    /// Names of tools available to the agent at this step.
    pub available_tools: &'a [String],
    /// Token budget remaining for this turn.
    pub budget_tokens: u64,
    /// Number of tool calls made this turn so far.
    pub tool_calls_this_turn: u32,
}

/// Output of the strategy's decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyAction {
    /// Invoke a tool by name (args supplied by caller based on tool schema).
    ToolCall(String),
    /// Ask the user a clarifying question instead of speculating.
    AskUser(String),
    /// Finish the turn.
    Finish,
}

pub trait AgentStrategy: Debug + Send + Sync {
    fn name(&self) -> &str;
    fn choose_next(&self, ctx: &StrategyContext<'_>) -> StrategyAction;
}

/// Baseline: reflects current SwarmOrchestrator behaviour — call tools
/// liberally, finish when no more tools are obvious, ask user only on
/// ambiguous input.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultStrategy;

impl AgentStrategy for DefaultStrategy {
    fn name(&self) -> &str {
        "default"
    }

    fn choose_next(&self, ctx: &StrategyContext<'_>) -> StrategyAction {
        if ctx.budget_tokens == 0 {
            return StrategyAction::Finish;
        }
        if ctx.tool_calls_this_turn >= 8 {
            return StrategyAction::Finish;
        }
        if let Some(tool) = ctx.available_tools.first() {
            return StrategyAction::ToolCall(tool.clone());
        }
        StrategyAction::Finish
    }
}

/// Conservative: prefers AskUser over speculative tool calls, and caps
/// tools-per-turn more aggressively. Intended for high-stakes or
/// low-confidence contexts.
#[derive(Debug, Default, Clone, Copy)]
pub struct ConservativeStrategy;

impl AgentStrategy for ConservativeStrategy {
    fn name(&self) -> &str {
        "conservative"
    }

    fn choose_next(&self, ctx: &StrategyContext<'_>) -> StrategyAction {
        if ctx.budget_tokens == 0 {
            return StrategyAction::Finish;
        }
        if ctx.tool_calls_this_turn >= 3 {
            return StrategyAction::Finish;
        }
        // If the last user message ended in a question mark, prefer asking
        // back for disambiguation before acting.
        if let Some(last) = ctx.recent_user_messages.last() {
            if last.trim_end().ends_with('?') && ctx.tool_calls_this_turn == 0 {
                return StrategyAction::AskUser(
                    "Confirm which option to take before I proceed.".to_string(),
                );
            }
        }
        if let Some(tool) = ctx.available_tools.first() {
            return StrategyAction::ToolCall(tool.clone());
        }
        StrategyAction::Finish
    }
}

/// Resolve a strategy by config name. Unknown names fall back to default.
pub fn strategy_from_name(name: &str) -> Box<dyn AgentStrategy> {
    match name {
        "conservative" => Box::new(ConservativeStrategy),
        _ => Box::new(DefaultStrategy),
    }
}

/// Advisory consult at the tool-executor entry. Emits a `tracing::warn!`
/// when the configured strategy disagrees with the LLM's chosen tool
/// batch, but does not block or alter execution (strategies are
/// advisory until enforcement-mode is a formal product decision).
///
/// - `tools_this_turn` is the count BEFORE this batch is added.
/// - `available_tools` are the names the LLM just asked to invoke.
pub fn advise(
    strategy: &dyn AgentStrategy,
    recent_user_messages: &[String],
    available_tools: &[String],
    budget_tokens: u64,
    tools_this_turn: u32,
) -> StrategyAction {
    let ctx = StrategyContext {
        recent_user_messages,
        available_tools,
        budget_tokens,
        tool_calls_this_turn: tools_this_turn,
    };
    let verdict = strategy.choose_next(&ctx);
    match &verdict {
        StrategyAction::Finish => {
            tracing::warn!(
                strategy = strategy.name(),
                tools_requested = available_tools.len(),
                "strategy suggests Finish but LLM wants tool calls — proceeding advisory"
            );
        }
        StrategyAction::AskUser(_) => {
            tracing::warn!(
                strategy = strategy.name(),
                tools_requested = available_tools.len(),
                "strategy suggests AskUser but LLM chose direct tool call — advisory only"
            );
        }
        StrategyAction::ToolCall(name) => {
            if !available_tools.iter().any(|t| t == name) {
                tracing::info!(
                    strategy = strategy.name(),
                    suggested = %name,
                    "strategy would have picked a different tool"
                );
            }
        }
    }
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(
        msgs: &'a [String],
        tools: &'a [String],
        budget: u64,
        calls: u32,
    ) -> StrategyContext<'a> {
        StrategyContext {
            recent_user_messages: msgs,
            available_tools: tools,
            budget_tokens: budget,
            tool_calls_this_turn: calls,
        }
    }

    #[test]
    fn default_picks_first_tool_within_budget() {
        let tools = vec!["read_file".to_string(), "bash".to_string()];
        let msgs: Vec<String> = vec![];
        let action = DefaultStrategy.choose_next(&ctx(&msgs, &tools, 1000, 0));
        assert_eq!(action, StrategyAction::ToolCall("read_file".into()));
    }

    #[test]
    fn default_finishes_when_budget_zero() {
        let tools = vec!["read_file".to_string()];
        let msgs: Vec<String> = vec![];
        let action = DefaultStrategy.choose_next(&ctx(&msgs, &tools, 0, 0));
        assert_eq!(action, StrategyAction::Finish);
    }

    #[test]
    fn conservative_prefers_askuser_on_question() {
        let tools = vec!["bash".to_string()];
        let msgs = vec!["What should I do next?".to_string()];
        let action = ConservativeStrategy.choose_next(&ctx(&msgs, &tools, 1000, 0));
        matches!(action, StrategyAction::AskUser(_))
            .then_some(())
            .expect("expected AskUser");
    }

    #[test]
    fn conservative_caps_tool_calls_tighter_than_default() {
        let tools = vec!["bash".to_string()];
        let msgs: Vec<String> = vec![];
        let default = DefaultStrategy.choose_next(&ctx(&msgs, &tools, 1000, 4));
        let conservative = ConservativeStrategy.choose_next(&ctx(&msgs, &tools, 1000, 4));
        // Default still calls tools at 4; conservative caps at 3.
        assert!(matches!(default, StrategyAction::ToolCall(_)));
        assert_eq!(conservative, StrategyAction::Finish);
    }

    #[test]
    fn strategy_from_name_resolves_both_impls() {
        assert_eq!(strategy_from_name("default").name(), "default");
        assert_eq!(strategy_from_name("conservative").name(), "conservative");
        // Unknown falls back to default.
        assert_eq!(strategy_from_name("alien").name(), "default");
    }
}
