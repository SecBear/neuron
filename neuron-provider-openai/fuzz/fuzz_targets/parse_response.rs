#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(json_str) = std::str::from_utf8(data) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
            // Just verify it doesn't panic — we don't care about the result
            let _ = neuron_provider_openai::mapping::from_api_response(&value);
        }
    }
});
