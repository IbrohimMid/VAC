//! Skill registry. Parallels `vac_tools::ToolRegistry` but stays
//! here so `vac_skill` itself doesn't depend on `vac_tools`.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::{SkillError, SkillResult};
use crate::skill::{Skill, SkillContext, SkillOutcome};

/// Thread-safe registry of named skills. Cheap to clone via the outer
/// `Arc` — the `RwLock` guards only the map.
#[derive(Default)]
pub struct SkillRegistry {
    skills: RwLock<HashMap<String, Arc<dyn Skill>>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a skill. Returns `SkillError::Duplicate` if the name
    /// is already taken — registration is idempotent in the sense
    /// that you can detect the collision, not silently overwrite.
    pub async fn register(&self, skill: Arc<dyn Skill>) -> SkillResult<()> {
        let name = skill.name().to_string();
        let mut guard = self.skills.write().await;
        if guard.contains_key(&name) {
            return Err(SkillError::Duplicate(name));
        }
        guard.insert(name, skill);
        Ok(())
    }

    pub async fn get(&self, name: &str) -> Option<Arc<dyn Skill>> {
        self.skills.read().await.get(name).cloned()
    }

    pub async fn list_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.skills.read().await.keys().cloned().collect();
        names.sort();
        names
    }

    pub async fn describe_all(&self) -> Vec<SkillDescription> {
        let mut out: Vec<SkillDescription> = self
            .skills
            .read()
            .await
            .values()
            .map(|s| SkillDescription {
                name: s.name().to_string(),
                description: s.description().to_string(),
                schema: s.schema(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Dispatch — look up + run in one call. Returns `NotFound` when
    /// the name isn't registered.
    pub async fn run(
        &self,
        name: &str,
        ctx: SkillContext,
    ) -> SkillResult<SkillOutcome> {
        let skill = self
            .get(name)
            .await
            .ok_or_else(|| SkillError::NotFound(name.to_string()))?;
        skill.run(ctx).await
    }
}

/// Human-readable catalog entry — used by `vac skills list` + any
/// surface that needs to render the registry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillDescription {
    pub name: String,
    pub description: String,
    pub schema: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct Noop(&'static str);

    #[async_trait]
    impl Skill for Noop {
        fn name(&self) -> &str {
            self.0
        }
        fn description(&self) -> &str {
            "noop"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn run(&self, _ctx: SkillContext) -> SkillResult<SkillOutcome> {
            Ok(SkillOutcome::new("noop", serde_json::json!({"ok": true})))
        }
    }

    #[tokio::test]
    async fn register_and_get_roundtrip() {
        let reg = SkillRegistry::new();
        reg.register(Arc::new(Noop("x"))).await.unwrap();
        assert!(reg.get("x").await.is_some());
        assert!(reg.get("missing").await.is_none());
    }

    #[tokio::test]
    async fn duplicate_registration_errors() {
        let reg = SkillRegistry::new();
        reg.register(Arc::new(Noop("x"))).await.unwrap();
        let err = reg.register(Arc::new(Noop("x"))).await.unwrap_err();
        assert!(matches!(err, SkillError::Duplicate(_)));
    }

    #[tokio::test]
    async fn list_names_is_sorted() {
        let reg = SkillRegistry::new();
        reg.register(Arc::new(Noop("b"))).await.unwrap();
        reg.register(Arc::new(Noop("a"))).await.unwrap();
        reg.register(Arc::new(Noop("c"))).await.unwrap();
        assert_eq!(reg.list_names().await, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn describe_all_returns_schema() {
        let reg = SkillRegistry::new();
        reg.register(Arc::new(Noop("x"))).await.unwrap();
        let out = reg.describe_all().await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "x");
        assert_eq!(out[0].schema["type"], "object");
    }

    #[tokio::test]
    async fn run_dispatches_by_name() {
        let reg = SkillRegistry::new();
        reg.register(Arc::new(Noop("x"))).await.unwrap();
        let ctx = SkillContext::new(
            serde_json::json!({}),
            std::path::PathBuf::from("."),
        );
        let out = reg.run("x", ctx).await.unwrap();
        assert_eq!(out.summary, "noop");
    }

    #[tokio::test]
    async fn run_missing_name_returns_not_found() {
        let reg = SkillRegistry::new();
        let ctx = SkillContext::new(
            serde_json::json!({}),
            std::path::PathBuf::from("."),
        );
        let err = reg.run("nope", ctx).await.unwrap_err();
        assert!(matches!(err, SkillError::NotFound(_)));
    }
}
