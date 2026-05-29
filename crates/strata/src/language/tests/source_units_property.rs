use super::super::{
    SourceProgram, SourceUnit, SourceUnitId, check_source_program, lower_to_artifact,
};

#[test]
fn property_generated_dependency_graphs_cover_order_cycles_duplicates_and_resolution() {
    for chain_len in 1..=6 {
        assert_chain_order_and_cycle_rejection(chain_len);
    }
    assert_generated_duplicate_imports_rejected();
    assert_generated_duplicate_modules_rejected();
    assert_generated_resolution_is_unit_order_stable();
}

fn assert_chain_order_and_cycle_rejection(chain_len: usize) {
    let mut sources = Vec::new();
    sources.push(generated_root_source(chain_len, false));
    for index in 0..chain_len {
        sources.push(generated_leaf_source(index, chain_len, false));
    }
    let program =
        source_program_from_strings(sources).expect("generated acyclic graph should be accepted");
    assert_eq!(program.units().len(), chain_len + 1);
    assert_eq!(
        program.dependency_order().last().copied(),
        Some(SourceUnitId::from_index(0).unwrap()),
        "root should be last after dependency-first ordering"
    );

    let mut cyclic_sources = Vec::new();
    cyclic_sources.push(generated_root_source(chain_len, true));
    for index in 0..chain_len {
        cyclic_sources.push(generated_leaf_source(index, chain_len, true));
    }
    let err = source_program_from_strings(cyclic_sources)
        .expect_err("generated cycle should be rejected");
    assert!(err.to_string().contains("import cycle"));
}

fn assert_generated_duplicate_imports_rejected() {
    let err = source_program_from_strings(vec![
        r#"module generated_root;
import leaf0;
import leaf0;
record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#
        .to_string(),
        generated_leaf_source(0, 1, false),
    ])
    .expect_err("generated duplicate import should be rejected");
    assert!(
        err.to_string()
            .contains("imports module leaf0 more than once")
    );
}

fn assert_generated_duplicate_modules_rejected() {
    let err = source_program_from_strings(vec![
        generated_root_source(1, false),
        generated_leaf_source(0, 1, false),
        generated_leaf_source(0, 1, false),
    ])
    .expect_err("generated duplicate module should be rejected");
    assert!(err.to_string().contains("duplicate module identity leaf0"));
}

fn assert_generated_resolution_is_unit_order_stable() {
    let canonical = source_program_from_array([
        stable_root_source(),
        stable_left_source(),
        stable_right_source(),
    ])
    .expect("canonical stable source graph should build");
    let reordered = SourceProgram::new(
        SourceUnitId::from_index(1).unwrap(),
        vec![
            SourceUnit::parse(
                SourceUnitId::from_index(0).unwrap(),
                stable_right_source().into(),
            )
            .unwrap(),
            SourceUnit::parse(
                SourceUnitId::from_index(1).unwrap(),
                stable_root_source().into(),
            )
            .unwrap(),
            SourceUnit::parse(
                SourceUnitId::from_index(2).unwrap(),
                stable_left_source().into(),
            )
            .unwrap(),
        ],
    )
    .expect("reordered stable source graph should build");

    let canonical_artifact = checked_artifact(canonical);
    let reordered_artifact = checked_artifact(reordered);
    assert_eq!(
        canonical_artifact.encode(),
        reordered_artifact.encode(),
        "artifact semantics should be stable across incidental source unit ids"
    );
}

fn checked_artifact(program: SourceProgram) -> mantle_artifact::MantleArtifact {
    let checked = check_source_program(program).expect("generated source graph should check");
    lower_to_artifact(&checked, "stable-source-order").expect("generated source graph should lower")
}

fn source_program_from_array<const N: usize>(
    sources: [&str; N],
) -> crate::language::Result<SourceProgram> {
    source_program_from_strings(
        sources
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    )
}

fn source_program_from_strings(sources: Vec<String>) -> crate::language::Result<SourceProgram> {
    let units = sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| SourceUnit::parse(SourceUnitId::from_index(index)?, source))
        .collect::<crate::language::Result<Vec<_>>>()?;
    SourceProgram::new(SourceUnitId::from_index(0)?, units)
}

fn generated_root_source(chain_len: usize, cyclic: bool) -> String {
    format!(
        r#"module generated_root_{chain_len}_{cyclic};
import leaf0;
record MainState;
enum MainMsg {{ Start }}
proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det {{ return MainState; }}
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

fn generated_leaf_source(index: usize, chain_len: usize, cyclic: bool) -> String {
    let next_import = if index + 1 < chain_len {
        format!("import leaf{};\n", index + 1)
    } else if cyclic {
        format!("import generated_root_{chain_len}_{cyclic};\n")
    } else {
        String::new()
    };
    format!(
        r#"module leaf{index};
{next_import}record Leaf{index};
"#
    )
}

fn stable_root_source() -> &'static str {
    r#"module stable_root;
import stable_left;
import stable_right;

record MainState {
    left: LeftBox,
    right: RightBox,
}
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det {
        return MainState { left: LeftBox, right: RightBox };
    }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#
}

fn stable_left_source() -> &'static str {
    r#"module stable_left;

record LeftBox;
"#
}

fn stable_right_source() -> &'static str {
    r#"module stable_right;

record RightBox;
"#
}
