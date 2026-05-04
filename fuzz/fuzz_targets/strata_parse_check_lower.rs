#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(checked) = strata::language::check_source(source) else {
        return;
    };

    strata::language::lower_to_artifact(&checked, source)
        .expect("checked source should lower to a valid artifact");
});
