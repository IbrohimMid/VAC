//! Slash command registry + dispatch.
//!
//! The TUI already has ad-hoc slash parsing, but it's TUI-coupled. We
//! want headless (`vac run`) and remote (bridge) to share the same
//! slash semantics. This module owns the dispatch; drivers register
//! their commands at engine construction.
//!
//! Slash commands are distinct from [`vac_tool_core::ToolSpec`] tools:
//! tools are LLM-facing (invoked by model output), slash commands are
//! operator/agent-facing (starts with `/`, intercepted before LLM).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::EngineResult;

/// Parsed view of a slash invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashInvocation {
    pub command: String,
    pub args: String,
}

impl SlashInvocation {
    /// Parse a raw input line like "/foo bar baz" into command `foo`
    /// and args `bar baz`. Returns `None` if the input doesn't start
    /// with `/`.
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim_start();
        let rest = trimmed.strip_prefix('/')?;
        let mut it = rest.splitn(2, char::is_whitespace);
        let command = it.next()?.to_string();
        if command.is_empty() {
            return None;
        }
        let args = it.next().unwrap_or("").trim().to_string();
        Some(Self { command, args })
    }
}

/// Result payload returned by a slash handler. Gets embedded in
/// [`SubmitEvent::SlashHandled`] for UIs to render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashResult {
    /// Terse operator-facing line (shown in transcript + footer).
    pub summary: String,
    /// Structured payload for drivers that want to render richer
    /// output. Optional.
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Handler invoked when the corresponding slash command fires.
#[async_trait]
pub trait SlashCommand: Send + Sync {
    /// Command name without the leading `/`. Must be unique within a
    /// processor.
    fn name(&self) -> &str;
    /// One-line description for `/help`.
    fn description(&self) -> &str;
    /// Execute. Gets the raw args string; individual handlers parse.
    async fn handle(&self, args: &str) -> EngineResult<SlashResult>;
}

/// Registry + dispatcher. Drivers register at construction, engine
/// calls `dispatch` before routing to the LLM.
pub struct SlashProcessor {
    commands: HashMap<String, Arc<dyn SlashCommand>>,
}

impl SlashProcessor {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn register(&mut self, cmd: Arc<dyn SlashCommand>) {
        self.commands.insert(cmd.name().to_string(), cmd);
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }

    pub fn list(&self) -> Vec<(&str, &str)> {
        let mut out: Vec<_> = self
            .commands
            .values()
            .map(|c| (c.name(), c.description()))
            .collect();
        out.sort_by_key(|(n, _)| *n);
        out
    }

    /// Check whether `input` is a slash invocation; returns the parsed
    /// form without dispatching. `None` = regular user message.
    pub fn detect(&self, input: &str) -> Option<SlashInvocation> {
        SlashInvocation::parse(input)
    }

    /// Dispatch a parsed invocation. Returns `Ok(None)` if the command
    /// is unknown (callers may fall back to treating the input as a
    /// regular message with the `/` preserved). Returns `Ok(Some(r))`
    /// on success.
    pub async fn dispatch(
        &self,
        inv: &SlashInvocation,
    ) -> EngineResult<Option<SlashResult>> {
        match self.commands.get(&inv.command) {
            Some(handler) => Ok(Some(handler.handle(&inv.args).await?)),
            None => Ok(None),
        }
    }
}

impl Default for SlashProcessor {
    fn default() -> Self {
        Self::new()
    }
}

// ── Bundled default slash commands ────────────────────────────────

/// `/help` — list every registered slash command.
pub struct HelpCommand {
    processor_snapshot: Vec<(String, String)>,
}

impl HelpCommand {
    pub fn new(processor: &SlashProcessor) -> Self {
        let snapshot = processor
            .list()
            .into_iter()
            .map(|(n, d)| (n.to_string(), d.to_string()))
            .collect();
        Self { processor_snapshot: snapshot }
    }
}

#[async_trait]
impl SlashCommand for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }
    fn description(&self) -> &str {
        "List available slash commands."
    }
    async fn handle(&self, _args: &str) -> EngineResult<SlashResult> {
        let lines: Vec<String> = self
            .processor_snapshot
            .iter()
            .map(|(n, d)| format!("/{n} — {d}"))
            .collect();
        Ok(SlashResult {
            summary: format!("{} slash commands", lines.len()),
            payload: serde_json::json!({ "commands": lines }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_slash_extracts_command_and_args() {
        let inv = SlashInvocation::parse("/plan review the refactor").unwrap();
        assert_eq!(inv.command, "plan");
        assert_eq!(inv.args, "review the refactor");
    }

    #[test]
    fn parse_slash_without_args() {
        let inv = SlashInvocation::parse("/help").unwrap();
        assert_eq!(inv.command, "help");
        assert_eq!(inv.args, "");
    }

    #[test]
    fn parse_slash_ignores_non_slash() {
        assert!(SlashInvocation::parse("not a slash").is_none());
        assert!(SlashInvocation::parse("").is_none());
        assert!(SlashInvocation::parse("/").is_none());
    }

    #[test]
    fn parse_slash_trims_leading_whitespace() {
        let inv = SlashInvocation::parse("   /x y").unwrap();
        assert_eq!(inv.command, "x");
        assert_eq!(inv.args, "y");
    }

    struct EchoCommand;
    #[async_trait]
    impl SlashCommand for EchoCommand {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "echoes args"
        }
        async fn handle(&self, args: &str) -> EngineResult<SlashResult> {
            Ok(SlashResult {
                summary: args.to_string(),
                payload: serde_json::json!({ "echoed": args }),
            })
        }
    }

    #[tokio::test]
    async fn dispatch_returns_none_for_unknown() {
        let p = SlashProcessor::new();
        let out = p
            .dispatch(&SlashInvocation::parse("/unknown x").unwrap())
            .await
            .unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn dispatch_runs_registered_handler() {
        let mut p = SlashProcessor::new();
        p.register(Arc::new(EchoCommand));
        let out = p
            .dispatch(&SlashInvocation::parse("/echo hi").unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out.summary, "hi");
        assert_eq!(out.payload["echoed"], "hi");
    }

    #[tokio::test]
    async fn help_lists_registered_commands() {
        let mut p = SlashProcessor::new();
        p.register(Arc::new(EchoCommand));
        let help = HelpCommand::new(&p);
        let out = help.handle("").await.unwrap();
        assert!(out.summary.contains("slash"));
        assert_eq!(
            out.payload["commands"].as_array().unwrap().len(),
            1
        );
    }
}
