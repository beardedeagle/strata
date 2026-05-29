#![no_main]

use libfuzzer_sys::fuzz_target;
use strata::language::{
    MAX_SOURCE_PROGRAM_BYTES, MAX_SOURCE_UNIT_COUNT, SourceProgram, SourceUnit, SourceUnitId,
    check_source_program, lower_to_artifact_with_source_hash,
};

const SOURCE_UNIT_DELIMITER: &str = "\n--- strata source unit ---\n";

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_SOURCE_PROGRAM_BYTES {
        return;
    }
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };

    let mut units = Vec::new();
    for (index, source) in input.split(SOURCE_UNIT_DELIMITER).enumerate() {
        if index >= MAX_SOURCE_UNIT_COUNT {
            return;
        }
        let Ok(id) = SourceUnitId::from_index(index) else {
            return;
        };
        let Ok(unit) = SourceUnit::parse(id, source.to_owned()) else {
            return;
        };
        units.push(unit);
    }

    let Ok(root) = SourceUnitId::from_index(0) else {
        return;
    };
    let Ok(program) = SourceProgram::new(root, units) else {
        return;
    };
    let source_hash = program.source_provenance_hash();
    let Ok(checked) = check_source_program(program) else {
        return;
    };

    lower_to_artifact_with_source_hash(&checked, source_hash)
        .expect("checked source program should lower to a valid artifact");
});
