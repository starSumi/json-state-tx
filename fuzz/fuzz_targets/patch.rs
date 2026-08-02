#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok((mut document, patch)) = serde_json::from_slice::<(serde_json::Value, json_patch::Patch)>(data) else {
        return;
    };
    let _ = json_patch::patch(&mut document, &patch);
    let _ = serde_json::to_vec(&document);
});

