pub async fn wait_for_intent(
    &self,
    tool_call_id: &str,
    total_timeout: std::time::Duration,
) -> ApprovalResult<Option<ApprovalIntent>> {
    let deadline = tokio::time::Instant::now() + total_timeout;
    let slice = std::time::Duration::from_millis(500);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }

        let notify = self.record_notifier(tool_call_id);
        let notified = notify.notified();

        if let Some(record) = self.load(tool_call_id)? {
            if let Some(intent) = record.intent {
                self.record_notifiers
                    .lock()
                    .expect("approval notifier cache poisoned")
                    .remove(tool_call_id);
                return Ok(Some(intent));
            }
            if record.state != ApprovalState::Pending {
                self.record_notifiers
                    .lock()
                    .expect("approval notifier cache poisoned")
                    .remove(tool_call_id);
                return Err(ApprovalError::Task(format!(
                    "Approval is not pending (tool_call_id={}, state={:?})",
                    tool_call_id, record.state
                )));
            }
        }

        let wait_span = remaining.min(slice);
        let _ = tokio::time::timeout(wait_span, notified).await;
    }
}
