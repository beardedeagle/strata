#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(trace) = std::str::from_utf8(data) {
        let _ = mantle_runtime::validate_runtime_trace_jsonl(trace);
    }
});
