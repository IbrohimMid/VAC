//! Session run checkpoint — serialize/deserialize agent message state.
//! Adapted from stakpak/libs/agent-core/src/checkpoint.rs (Apache-2.0).

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use vil_llm::provider::Message;

pub const CHECKPOINT_VERSION_V1: u16 = 1;
pub const CHECKPOINT_FORMAT_V1: &str = "vac_message_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointEnvelope {
    pub version: u16,
    pub format: String,
    pub run_id: Option<Uuid>,
    pub messages: Vec<Message>,
    pub metadata: serde_json::Value,
}

impl CheckpointEnvelope {
    pub fn new(run_id: Option<Uuid>, messages: Vec<Message>, metadata: serde_json::Value) -> Self {
        Self {
            version: CHECKPOINT_VERSION_V1,
            format: CHECKPOINT_FORMAT_V1.to_string(),
            run_id,
            messages,
            metadata,
        }
    }
}

#[derive(Debug, Error)]
pub enum CheckpointError {
    #[error("invalid checkpoint payload: {0}")]
    InvalidPayload(#[from] serde_json::Error),
    #[error("checkpoint payload is missing version")]
    MissingVersion,
    #[error("unsupported checkpoint version: {0}")]
    UnsupportedVersion(u16),
    #[error("unsupported checkpoint format: {0}")]
    UnsupportedFormat(String),
}

pub fn serialize_checkpoint(envelope: &CheckpointEnvelope) -> Result<Vec<u8>, CheckpointError> {
    serde_json::to_vec(envelope).map_err(CheckpointError::InvalidPayload)
}

pub fn deserialize_checkpoint(payload: &[u8]) -> Result<CheckpointEnvelope, CheckpointError> {
    let value: serde_json::Value = serde_json::from_slice(payload)?;

    let Some(version) = value.get("version").and_then(|v| v.as_u64()) else {
        if let Some(migrated) = migrate_legacy(value) {
            return Ok(migrated);
        }
        return Err(CheckpointError::MissingVersion);
    };

    let version = version as u16;
    if version != CHECKPOINT_VERSION_V1 {
        return Err(CheckpointError::UnsupportedVersion(version));
    }

    let envelope: CheckpointEnvelope = serde_json::from_value(value)?;

    if envelope.format != CHECKPOINT_FORMAT_V1 {
        return Err(CheckpointError::UnsupportedFormat(envelope.format));
    }

    Ok(envelope)
}

/// Migrate legacy checkpoint formats (plain messages array or object without version).
fn migrate_legacy(value: serde_json::Value) -> Option<CheckpointEnvelope> {
    if value.is_array() {
        let messages: Vec<Message> = serde_json::from_value(value).ok()?;
        return Some(CheckpointEnvelope::new(
            None,
            messages,
            serde_json::json!({"migrated_from": "legacy_messages_array"}),
        ));
    }
    let obj = value.as_object()?;
    let messages: Vec<Message> = serde_json::from_value(obj.get("messages")?.clone()).ok()?;
    let run_id = obj.get("run_id")
        .and_then(|v| serde_json::from_value::<Uuid>(v.clone()).ok());
    let metadata = obj.get("metadata").cloned().unwrap_or_else(|| serde_json::json!({}));
    Some(CheckpointEnvelope::new(run_id, messages, metadata))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vil_llm::provider::{Message, Role};

    #[test]
    fn roundtrip_v1() {
        let run_id = Some(Uuid::new_v4());
        let env = CheckpointEnvelope::new(
            run_id,
            vec![Message::user("hello")],
            serde_json::json!({"cwd": "/workspace"}),
        );
        let bytes = serialize_checkpoint(&env).unwrap();
        let parsed = deserialize_checkpoint(&bytes).unwrap();
        assert_eq!(parsed.version, CHECKPOINT_VERSION_V1);
        assert_eq!(parsed.run_id, run_id);
        assert_eq!(parsed.messages[0].content, "hello");
    }

    #[test]
    fn reject_unsupported_version() {
        let payload = serde_json::json!({
            "version": 2, "format": CHECKPOINT_FORMAT_V1,
            "run_id": null, "messages": [], "metadata": {}
        });
        let err = deserialize_checkpoint(payload.to_string().as_bytes()).unwrap_err();
        assert!(err.to_string().contains("unsupported checkpoint version: 2"));
    }

    #[test]
    fn reject_wrong_format() {
        let payload = serde_json::json!({
            "version": 1, "format": "legacy",
            "run_id": null, "messages": [], "metadata": {}
        });
        let err = deserialize_checkpoint(payload.to_string().as_bytes()).unwrap_err();
        assert!(err.to_string().contains("unsupported checkpoint format: legacy"));
    }

    #[test]
    fn migrates_legacy_messages_array() {
        let payload = serde_json::json!([
            {"role": "User", "content": "legacy", "tool_calls": []}
        ]);
        let env = deserialize_checkpoint(payload.to_string().as_bytes()).unwrap();
        assert_eq!(env.messages[0].content, "legacy");
    }
}
