#![no_main]

use libfuzzer_sys::fuzz_target;
use vac_tools::mcp::{JsonRpcRequest, JsonRpcResponse};

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _: Result<JsonRpcResponse, _> = serde_json::from_str(s);
        let _: Result<JsonRpcRequest, _> = serde_json::from_str(s);
    }
});
