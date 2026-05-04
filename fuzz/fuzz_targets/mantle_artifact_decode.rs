#![no_main]

use libfuzzer_sys::fuzz_target;
use mantle_artifact::MantleArtifact;

fuzz_target!(|data: &[u8]| {
    let Ok(contents) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(artifact) = MantleArtifact::decode(contents) else {
        return;
    };

    let encoded = artifact.encode();
    let decoded =
        MantleArtifact::decode(&encoded).expect("encoded artifact should decode successfully");
    assert_eq!(decoded, artifact);
});
