pub async fn wait_for_record(
    &self,
    tool_call_id: &str,
    timeout: std::time::Duration,
) -> ApprovalResult<Option<ApprovalRecord>> {
    let notify = self.record_notifier(tool_call_id);
    let notified = notify.notified();

    if let Some(record) = self.load(tool_call_id)? {
        self.record_notifiers
            .lock()
            .expect("approval notifier cache poisoned")
            .remove(tool_call_id);
        return Ok(Some(record));
    }

    let _ = tokio::time::timeout(timeout, notified).await;
    self.record_notifiers
        .lock()
        .expect("approval notifier cache poisoned")
        .remove(tool_call_id);
    self.load(tool_call_id)
}
