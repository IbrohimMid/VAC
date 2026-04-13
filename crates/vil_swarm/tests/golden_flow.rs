//! Golden flow integration test — end-to-end control-plane validation.
//!
//! Tests:
//! - Long run with multiple iterations
//! - Context reduction
//! - Checkpoint save
//! - Resume from checkpoint
//! - Cancellation
//! - File tracking

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use vil_swarm::run_state::{AgentRunState, RunStage};
    use vil_llm::provider::Message;

    #[test]
    fn golden_flow_checkpoint_resume() {
        // Simulate a long-running agent loop
        let mut state = AgentRunState::new(
            vec![
                Message::system("You are a helpful assistant".to_string()),
                Message::user("Create a Rust project".to_string()),
            ],
            None,
        );

        // Simulate progress
        state.iterations = 10;
        state.total_tokens = 5000;
        state.trim_boundary = 2;
        state.record_created("src/main.rs".to_string());
        state.record_modified("Cargo.toml".to_string());
        state.stage = RunStage::Coder;

        // Add some conversation history
        state.messages.push(Message::assistant("I'll create the project structure".to_string()));
        state.messages.push(Message::user("Add error handling".to_string()));

        // Save checkpoint
        let checkpoint_path = std::env::temp_dir().join("golden_flow_checkpoint.json");
        state.save_checkpoint(&checkpoint_path, None).unwrap();

        // Simulate crash/interruption - drop state
        drop(state);

        // Resume from checkpoint
        let restored = AgentRunState::from_checkpoint(&checkpoint_path).unwrap();

        // Verify state restoration
        assert_eq!(restored.iterations, 10);
        assert_eq!(restored.total_tokens, 5000);
        assert_eq!(restored.trim_boundary, 2);
        assert_eq!(restored.stage, RunStage::Coder);
        assert_eq!(restored.created_files.len(), 1);
        assert_eq!(restored.modified_files.len(), 1);
        assert!(restored.created_files.contains(&"src/main.rs".to_string()));
        assert!(restored.modified_files.contains(&"Cargo.toml".to_string()));
        assert_eq!(restored.messages.len(), 4); // system + user + assistant + user

        // Verify can continue execution
        let mut continued = restored;
        continued.iterations += 1;
        continued.total_tokens += 100;
        continued.stage = RunStage::Completed;

        // Save final checkpoint
        continued.save_checkpoint(&checkpoint_path, None).unwrap();

        // Verify final state
        let final_state = AgentRunState::from_checkpoint(&checkpoint_path).unwrap();
        assert_eq!(final_state.iterations, 11);
        assert_eq!(final_state.total_tokens, 5100);
        assert_eq!(final_state.stage, RunStage::Completed);

        // Cleanup
        std::fs::remove_file(checkpoint_path).ok();
    }

    #[test]
    fn golden_flow_cancellation() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let state = AgentRunState::new(vec![], Some(cancel.clone()));

        assert!(!state.is_cancelled());
        cancel.cancel();
        assert!(state.is_cancelled());
    }

    #[test]
    fn golden_flow_file_tracking() {
        let mut state = AgentRunState::new(vec![], None);

        // Simulate file operations
        state.record_created("src/lib.rs".to_string());
        state.record_created("src/main.rs".to_string());
        state.record_modified("Cargo.toml".to_string());
        state.record_modified("README.md".to_string());

        // Verify deduplication
        state.record_created("src/lib.rs".to_string());
        state.record_modified("Cargo.toml".to_string());

        assert_eq!(state.created_files.len(), 2);
        assert_eq!(state.modified_files.len(), 2);
    }

    #[test]
    fn golden_flow_stage_transitions() {
        let mut state = AgentRunState::new(vec![], None);

        assert_eq!(state.stage, RunStage::Planner);

        state.stage = RunStage::Coder;
        assert_eq!(state.stage, RunStage::Coder);

        state.stage = RunStage::Completed;
        assert_eq!(state.stage, RunStage::Completed);
    }
}
