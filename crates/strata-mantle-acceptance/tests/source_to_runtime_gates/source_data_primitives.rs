use super::support::*;
use mantle_artifact::ArtifactPrimitiveType;

const SOURCE: &str = "examples/source_contract_data_primitives.str";
const ARTIFACT: &str = "target/strata/source_contract_data_primitives.mta";
const STEM: &str = "source_contract_data_primitives";
const READY_STRING_LABEL: &str = "String(7265616479)";
const READY_BYTES_LABEL: &str = "Bytes(010262696e)";

#[test]
fn source_contract_data_primitives_check_build_run_preserves_typed_values() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8(run.stdout).expect("mantle stdout should be UTF-8");
    assert!(stdout.contains("typed String and Bytes survived Mantle"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert!(stdout.contains(
        "mantle: delivered Replace(DataBundle{label:String(7265616479),raw:Bytes(010262696e)"
    ));

    let artifact = gate.read_artifact(ARTIFACT);
    assert_primitive_shape(&artifact, "String", ArtifactPrimitiveType::String);
    assert_primitive_shape(&artifact, "Bytes", ArtifactPrimitiveType::Bytes);

    let encoded = artifact.encode();
    assert!(encoded.contains("shape=primitive"));
    assert!(encoded.contains("primitive_type=string"));
    assert!(encoded.contains("primitive_type=bytes"));
    assert!(encoded.contains(READY_STRING_LABEL));
    assert!(encoded.contains(READY_BYTES_LABEL));

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_accepted""#,
            r#""process":"Worker""#,
            r#""message":"Replace""#,
            READY_STRING_LABEL,
            READY_BYTES_LABEL,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"typed String and Bytes survived Mantle""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""result":"Stop""#,
            "WorkerState{current:DataBundle{label:String(7265616479),raw:Bytes(010262696e)",
        ],
    );
}

fn assert_primitive_shape(artifact: &MantleArtifact, label: &str, expected: ArtifactPrimitiveType) {
    let ty = value_type_id(artifact, label);
    assert!(
        matches!(
            artifact.types[ty.index()].shape.as_ref(),
            Some(ArtifactValueShape::Primitive { primitive }) if *primitive == expected
        ),
        "type {label} should be primitive {expected:?}"
    );
}
