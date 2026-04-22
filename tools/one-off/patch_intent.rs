use std::time::Duration;

pub async fn wait_for_intent(
    &self,
    tool_call_id: &str,
    total_timeout: std::time::Duration,
) -> ApprovalResult<Option<ApprovalIntent>> {
    let start = std::time::Instant::now();

    loop {
        // Register interest BEFORE load to avoid race condition
        let notify = self.record_notifier(tool_call_id);
        let notified = notify.notified();

        if let Some(record) = self.load(tool_call_id)? {
            if let Some(intent) = record.intent {
                // Clean up notifier
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

        let remaining = total_timeout.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            self.record_notifiers
                .lock()
                .expect("approval notifier cache poisoned")
                .remove(tool_call_id);
            return Ok(None);
        }

        let _ = tokio::time::timeout(remaining, notified).await;
    }
}
