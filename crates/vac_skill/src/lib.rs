//! W3.1 — `vac_skill`: first-class composition unit for agent workflows.
//!
//! A skill is a named, schema-validated workflow the agent invokes as
//! a single tool call. Concretely, each skill is:
//!
//! - A stable `name` + human-readable `description`.
//! - A JSON Schema for its input so the model can be guided by the
//!   same contract the runtime validates against.
//! - An async `run(SkillContext) -> SkillOutcome` method.
//!
//! Skills compose *tools* (via the caller — they are handed a
//! `SkillContext` that carries whatever services the skill needs);
//! they are **not** a second tool system. Every skill eventually
//! surfaces to the model as one entry in a `ToolSpec` registered by
//! `vac_tools::builtin::skill_tool::SkillTool`.
//!
//! Bundled skills (W3.2) live in [`bundled`]. The [`SkillRegistry`]
//! is the lookup surface [`SkillTool`] uses to dispatch by name.

pub mod bundled;
pub mod error;
pub mod registry;
pub mod skill;

pub use error::{SkillError, SkillResult};
pub use registry::SkillRegistry;
pub use skill::{Skill, SkillContext, SkillOutcome};
