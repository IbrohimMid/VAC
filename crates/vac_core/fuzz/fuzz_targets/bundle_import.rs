#![no_main]

use libfuzzer_sys::fuzz_target;
use tempfile::tempdir;
use std::fs;
use vac_core::bundle::{import_bundle_from_path_with_options, BundleImportOptions};

fuzz_target!(|data: &[u8]| {
    // We create a temporary directory to act as our project root and bundle source
    let dir = match tempdir() {
        Ok(d) => d,
        Err(_) => return,
    };
    let root = dir.path();
    let bundle_path = root.join("fuzz.bundle.json");

    if fs::write(&bundle_path, data).is_err() {
        return;
    }

    // Try to import the bundle
    let options = BundleImportOptions {
        require_signed: false,
        overwrite_session: true,
        trust_approvals: false,
        redact_on_import: true,
    };

    // We don't care about the result (Ok or Err), only that it doesn't panic
    let _ = import_bundle_from_path_with_options(root, &bundle_path, options);
});
