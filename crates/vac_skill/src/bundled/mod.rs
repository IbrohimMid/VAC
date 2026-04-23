//! W3.2 — bundled skills that ship with VAC out of the box.
//!
//! Each submodule defines exactly one [`Skill`] implementation plus
//! its unit tests. [`register_bundled`] installs every one into a
//! given [`SkillRegistry`].

use std::sync::Arc;

use crate::error::SkillResult;
use crate::registry::SkillRegistry;

pub mod batch;
pub mod loop_skill;
pub mod remember;
pub mod simplify;
pub mod stuck;
pub mod verify;

/// Register every bundled skill into `registry`. Returns the first
/// registration error (typically a duplicate name when the caller
/// bundles + re-registers in the same process).
pub async fn register_bundled(registry: &SkillRegistry) -> SkillResult<()> {
    registry.register(Arc::new(batch::BatchSkill)).await?;
    registry.register(Arc::new(loop_skill::LoopSkill)).await?;
    registry.register(Arc::new(remember::RememberSkill)).await?;
    registry.register(Arc::new(simplify::SimplifySkill)).await?;
    registry.register(Arc::new(stuck::StuckSkill)).await?;
    registry.register(Arc::new(verify::VerifySkill)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_bundled_installs_all_six() {
        let reg = SkillRegistry::new();
        register_bundled(&reg).await.unwrap();
        let names = reg.list_names().await;
        assert_eq!(
            names,
            vec!["batch", "loop", "remember", "simplify", "stuck", "verify"]
        );
    }

    #[tokio::test]
    async fn register_bundled_twice_errors_on_duplicate() {
        let reg = SkillRegistry::new();
        register_bundled(&reg).await.unwrap();
        let err = register_bundled(&reg).await.unwrap_err();
        assert!(matches!(err, crate::SkillError::Duplicate(_)));
    }
}
