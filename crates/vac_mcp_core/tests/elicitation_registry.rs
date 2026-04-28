//! G1c integration — exercise `McpElicitationRegistry` end-to-end
//! via the public surface (attach → dispatch).

use std::sync::Arc;

use vac_mcp_core::{
    ElicitationRequest, ElicitationResult, McpElicitationRegistry, UnsupportedElicitationHandler,
};

#[tokio::test]
async fn registry_without_handler_falls_back_to_unsupported() {
    let registry = McpElicitationRegistry::new();
    assert!(!registry.has_handler());
    let result = registry
        .dispatch(ElicitationRequest::Confirm {
            prompt: "proceed?".into(),
        })
        .await
        .expect("dispatch ok");
    assert_eq!(result, ElicitationResult::Cancelled);
}

#[tokio::test]
async fn registry_with_unsupported_handler_returns_cancelled() {
    let mut registry = McpElicitationRegistry::new();
    registry.attach_elicitation_handler(Arc::new(UnsupportedElicitationHandler));
    assert!(registry.has_handler());
    let result = registry
        .dispatch(ElicitationRequest::OpenUrl {
            url: "https://example.test/auth".into(),
            prompt: None,
        })
        .await
        .expect("dispatch ok");
    assert_eq!(result, ElicitationResult::Cancelled);
}
