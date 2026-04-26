use async_trait::async_trait;
use vac_session_engine::{EngineResult, LlmAdapter, LlmRequest, LlmResponse, ToolCallRequest};

pub struct ToolEmittingAdapter {
    pub tool_calls: Vec<ToolCallRequest>,
}

#[async_trait]
impl LlmAdapter for ToolEmittingAdapter {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
        Ok(LlmResponse {
            provider: "test-provider".into(),
            model: "test-model".into(),
            content: "I want tools".into(),
            input_tokens: 1,
            output_tokens: 1,
            tool_calls: self.tool_calls.clone(),
        })
    }
}

pub fn call(id: &str, name: &str, args: serde_json::Value) -> ToolCallRequest {
    ToolCallRequest {
        id: id.into(),
        name: name.into(),
        arguments: args,
        reason: None,
        estimated_tokens: 0,
    }
}
