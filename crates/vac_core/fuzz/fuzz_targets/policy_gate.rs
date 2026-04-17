#![no_main]

use libfuzzer_sys::fuzz_target;
use vac_core::policy_gate::classify_shell_command;

fuzz_target!(|data: &[u8]| {
    let command = String::from_utf8_lossy(data);
    let _ = classify_shell_command(&command);
});
