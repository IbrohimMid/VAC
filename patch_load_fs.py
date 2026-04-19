import re

def patch_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()
    
    # Insert load_from_fs
    if "pub fn load_from_fs" not in content:
        load_pattern = r'    pub fn load\(&self, tool_call_id: &str\) -> ApprovalResult<Option<ApprovalRecord>> \{'
        load_repl = """    pub fn load_from_fs(&self, tool_call_id: &str) -> ApprovalResult<Option<ApprovalRecord>> {
        let path = self.approval_path(tool_call_id);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)?;
        Ok(Some(serde_json::from_str(&content)?))
    }

    pub fn load(&self, tool_call_id: &str) -> ApprovalResult<Option<ApprovalRecord>> {"""
        content = re.sub(load_pattern, load_repl, content)
    
    # Change wait_for_intent to use load_from_fs
    content = content.replace('if let Some(record) = self.load(tool_call_id)? {', 'if let Some(record) = self.load_from_fs(tool_call_id)? {')
    
    with open(filepath, 'w') as f:
        f.write(content)

patch_file('crates/vac_approvals/src/lib.rs')
