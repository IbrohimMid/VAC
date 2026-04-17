#![no_main]

use libfuzzer_sys::fuzz_target;
use vac_core::bundle::{BundleImportOptions, parse_bundle_from_bytes_with_options};

fuzz_target!(|data: &[u8]| {
    let options = BundleImportOptions::default();
    let _ = parse_bundle_from_bytes_with_options(data, &options);
});
