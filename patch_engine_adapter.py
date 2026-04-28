import re

content = open("crates/vac_tui_runtime/src/runner/engine_adapter.rs").read()

# Add vac_session_engine::notify_hooks::fire_notification_hook for Aborted and Finished
content = re.sub(
    r'SubmitChunk::Finished \{ usage: snap \} => \{',
    r'SubmitChunk::Finished { usage: snap } => {\n                                vac_session_engine::notify_hooks::fire_notification_hook(project_root.clone(), vac_session_engine::HookEvent::TurnFinished, None);',
    content
)

content = re.sub(
    r'SubmitChunk::Aborted \{ reason \} => \{',
    r'SubmitChunk::Aborted { reason } => {\n                                vac_session_engine::notify_hooks::fire_notification_hook(project_root.clone(), vac_session_engine::HookEvent::TaskFailed, Some(reason.clone()));',
    content
)

# For ApprovalRequired, it is handled in translate(update) which returns None for it.
# We need to catch it in the forwarder loop:
forwarder_pattern = r'''        let translator = tokio::spawn\(async move \{
            while let Some\(update\) = rt_rx\.recv\(\)\.await \{
                if let Some\(ev\) = translate\(update\) \{'''

forwarder_repl = r'''        let pr = self.project_root.clone();
        let translator = tokio::spawn(async move {
            while let Some(update) = rt_rx.recv().await {
                if let RuntimeUpdate::ApprovalRequired { ref tool_name, .. } = update {
                    vac_session_engine::notify_hooks::fire_notification_hook(
                        pr.clone(), 
                        vac_session_engine::HookEvent::ApprovalRequired, 
                        Some(tool_name.clone())
                    );
                }
                if let Some(ev) = translate(update) {'''

content = content.replace(forwarder_pattern, forwarder_repl)

# We need to pass project_root to complete()
# In VacEngineAdapter struct:
content = content.replace('engine: Arc<Mutex<VacEngine>>,', 'engine: Arc<Mutex<VacEngine>>,\n    project_root: std::path::PathBuf,')

# In new / with_event_forward / with_result_tx
content = content.replace('pub fn new(engine: Arc<Mutex<VacEngine>>) -> Self {', 'pub fn new(engine: Arc<Mutex<VacEngine>>, project_root: std::path::PathBuf) -> Self {')
content = content.replace('engine,\n            event_forward: None,', 'engine,\n            project_root,\n            event_forward: None,')

content = content.replace('pub fn with_event_forward(\n        engine: Arc<Mutex<VacEngine>>,\n        tx: mpsc::UnboundedSender<SubmitEvent>,\n    ) -> Self {', 'pub fn with_event_forward(\n        engine: Arc<Mutex<VacEngine>>,\n        tx: mpsc::UnboundedSender<SubmitEvent>,\n        project_root: std::path::PathBuf,\n    ) -> Self {')
content = content.replace('engine,\n            event_forward: Some(tx),', 'engine,\n            project_root,\n            event_forward: Some(tx),')

content = content.replace('pub fn with_result_tx(\n        engine: Arc<Mutex<VacEngine>>,\n        result_tx: oneshot::Sender<TaskResult>,\n    ) -> Self {', 'pub fn with_result_tx(\n        engine: Arc<Mutex<VacEngine>>,\n        result_tx: oneshot::Sender<TaskResult>,\n        project_root: std::path::PathBuf,\n    ) -> Self {')
content = content.replace('engine,\n            event_forward: None,\n            result_tx:', 'engine,\n            project_root,\n            event_forward: None,\n            result_tx:')

# Update call sites of VacEngineAdapter
content = content.replace('let adapter = Arc::new(VacEngineAdapter::with_event_forward(\n        engine_arc,\n        adapter_tx,\n    ));', 'let adapter = Arc::new(VacEngineAdapter::with_event_forward(\n        engine_arc,\n        adapter_tx,\n        project_root.clone(),\n    ));')

open("crates/vac_tui_runtime/src/runner/engine_adapter.rs", "w").write(content)
