use vac_core::config::LlmConfig;

// If LlmConfig is defined in vac_core, this will compile successfully.
// If it is properly re-exported from vil_llm, this will FAIL to compile 
// due to the orphan rule (cannot implement foreign trait on foreign type).
// We WANT this to fail to compile, which is why it's a compile_fail test!
impl std::io::Read for LlmConfig {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
}

fn main() {}
