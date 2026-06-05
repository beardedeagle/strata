use super::support::{GateHarness, assert_trace_event};

const SOURCE: &str = "examples/effect_outcome_spawn_denied.str";
const LEXICAL_SUPERVISION_SOURCE: &str = "examples/local_supervision_restart.str";
const COMPONENT_SOURCE: &str = "examples/component_composition_main.str";
const MISMATCHED_SOURCE: &str = "examples/hello.str";

#[test]
fn accepted_authority_effect_binding_runs_with_typed_trace_evidence() {
    let paths = ScenarioPaths::new("authority_effect_accepted");
    let harness = GateHarness::new();
    harness.remove_trace(paths.trace_stem);
    harness.check(SOURCE);
    harness.build(SOURCE, paths.artifact);
    harness.authority_effect_build(SOURCE, paths.authority_effect);
    let admission = harness.authority_effect_admit(paths.authority_effect, "json");
    assert!(
        String::from_utf8_lossy(&admission.stdout).contains("\"admission_result\":\"admitted\""),
        "authority/effect admission output should prove admitted status"
    );
    harness.authority_effect_bind_runtime(
        paths.authority_effect,
        paths.artifact,
        paths.binding,
        false,
    );

    let run = harness.run_mantle_success_with_args(
        paths.artifact,
        &["--authority-effect-binding", paths.binding],
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("spawn accepted"),
        "admitted binding should leave declared spawn authority accepted"
    );
    let trace = harness.read_trace(paths.trace_stem);
    assert_trace_event(
        &trace,
        &[
            "\"event\":\"spawn_authority_checked\"",
            "\"process_id\":0",
            "\"target_process_id\":1",
            "\"spawn_site_id\":0",
            "\"authority_id\":0",
            "\"authority_result\":\"accepted\"",
        ],
    );
}

#[test]
fn denied_authority_effect_binding_denies_declared_spawn_by_typed_id() {
    let paths = ScenarioPaths::new("authority_effect_denied");
    let harness = GateHarness::new();
    harness.remove_trace(paths.trace_stem);
    harness.check(SOURCE);
    harness.build(SOURCE, paths.artifact);
    harness.authority_effect_build(SOURCE, paths.authority_effect);
    harness.authority_effect_admit(paths.authority_effect, "json");
    harness.authority_effect_bind_runtime(
        paths.authority_effect,
        paths.artifact,
        paths.binding,
        true,
    );

    let run = harness.run_mantle_success_with_args(
        paths.artifact,
        &["--authority-effect-binding", paths.binding],
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("spawn denied"),
        "denied binding should deny declared spawn authority"
    );
    let trace = harness.read_trace(paths.trace_stem);
    assert_trace_event(
        &trace,
        &[
            "\"event\":\"spawn_authority_checked\"",
            "\"process_id\":0",
            "\"target_process_id\":1",
            "\"spawn_site_id\":0",
            "\"authority_id\":0",
            "\"authority_result\":\"denied\"",
        ],
    );
    assert_trace_event(
        &trace,
        &[
            "\"event\":\"effect_outcome_bound\"",
            "\"action\":\"spawn\"",
            "\"target_process_id\":1",
            "\"spawn_site_id\":0",
            "\"outcome_result\":\"denied\"",
        ],
    );
}

#[test]
fn lexical_supervisor_child_authority_effect_binding_runs_with_typed_trace_evidence() {
    let paths = ScenarioPaths::new("authority_effect_lexical_supervisor_child");
    let harness = GateHarness::new();
    harness.remove_trace(paths.trace_stem);
    harness.check(LEXICAL_SUPERVISION_SOURCE);
    harness.build(LEXICAL_SUPERVISION_SOURCE, paths.artifact);
    harness.authority_effect_build(LEXICAL_SUPERVISION_SOURCE, paths.authority_effect);
    let admission = harness.authority_effect_admit(paths.authority_effect, "json");
    assert!(
        String::from_utf8_lossy(&admission.stdout).contains("\"admission_result\":\"admitted\""),
        "lexical supervisor authority/effect admission should prove admitted status"
    );
    harness.authority_effect_bind_runtime(
        paths.authority_effect,
        paths.artifact,
        paths.binding,
        false,
    );
    let binding = harness.read_text_artifact(paths.binding);
    assert!(
        binding.contains("\"kind\":\"lexical_supervisor_child\""),
        "runtime binding should carry the lexical supervisor-child spawn-site kind"
    );
    assert!(
        binding.contains("\"authority_id\":null"),
        "lexical supervisor-child binding should not invent dynamic authority"
    );

    harness.run_mantle_success_with_args(
        paths.artifact,
        &["--authority-effect-binding", paths.binding],
    );
    let trace = harness.read_trace(paths.trace_stem);
    assert_trace_event(
        &trace,
        &[
            "\"event\":\"supervisor_child_started\"",
            "\"supervisor_process_id\":0",
            "\"child_process_id\":1",
            "\"spawn_site_id\":0",
            "\"spawn_kind\":\"lexical_supervisor_child\"",
        ],
    );
    assert_trace_event(
        &trace,
        &[
            "\"event\":\"supervisor_restart_decision\"",
            "\"supervisor_process_id\":0",
            "\"child_process_id\":1",
            "\"decision\":\"restarted\"",
            "\"new_child_pid\":3",
        ],
    );
}

#[test]
fn component_authority_surfaces_bind_with_composition_runtime_evidence() {
    let paths = ComponentSurfacePaths::new("authority_effect_component_surfaces");
    let harness = GateHarness::new();
    harness.remove_trace(paths.trace_stem);
    let admission = build_component_authority_binding(&harness, &paths);
    assert!(
        String::from_utf8_lossy(&admission.stdout)
            .contains("\"component_authority_surface_count\":2"),
        "component authority/effect admission output should prove both component surfaces"
    );

    let binding = harness.read_text_artifact(paths.binding);
    assert!(
        binding.contains("\"component_authority_surfaces\":[{"),
        "runtime authority/effect binding should include component authority surfaces"
    );
    assert!(
        binding
            .contains("\"component_authority\":{\"kind\":\"component_export\",\"component_id\":0}"),
        "runtime binding should retain typed component-export authority ids"
    );
    assert!(
        binding.contains("\"export_port_authority\":{\"kind\":\"port_connect\",\"port_id\":0}"),
        "runtime binding should retain typed export-port authority ids"
    );
    assert!(
        binding.contains("\"import_port_authorities\":[{\"port_id\":0"),
        "runtime binding should retain typed import-port authority ids"
    );
    assert!(
        !binding.contains("\"component\":\"") && !binding.contains("\"port\":\""),
        "runtime binding must not carry source component or port labels as executable references"
    );

    let run = harness.run_mantle_success_with_args(
        paths.artifact,
        &[
            "--composition-binding",
            paths.composition_binding,
            "--authority-effect-binding",
            paths.binding,
        ],
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("composed worker handled Work"),
        "component authority/effect binding should preserve composed runtime behavior"
    );
    let trace = harness.read_trace(paths.trace_stem);
    assert_trace_event(
        &trace,
        &[
            "\"event\":\"boundary_send_checked\"",
            "\"boundary_result\":\"accepted\"",
            "\"deployment_id\":0",
            "\"composition_id\":0",
            "\"component_instance_id\":0",
        ],
    );
    assert_trace_event(
        &trace,
        &[
            "\"event\":\"program_output\"",
            "\"component_instance_id\":1",
        ],
    );
}

#[test]
fn forged_authority_effect_binding_rejects_before_trace_side_effects() {
    let paths = ScenarioPaths::new("authority_effect_forged");
    let harness = GateHarness::new();
    harness.remove_trace(paths.trace_stem);
    harness.check(SOURCE);
    harness.build(SOURCE, paths.artifact);
    harness.authority_effect_build(SOURCE, paths.authority_effect);
    harness.authority_effect_bind_runtime(
        paths.authority_effect,
        paths.artifact,
        paths.binding,
        false,
    );
    let forged = harness.read_text_artifact(paths.binding).replace(
        "{\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":1}}",
        "{\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":0}}",
    );
    harness.write_text_artifact(paths.forged_binding, &forged);

    let run = harness.run_mantle_failure_with_args(
        paths.artifact,
        &["--authority-effect-binding", paths.forged_binding],
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("descriptor does not match runtime artifact"),
        "forged binding should fail closed through typed descriptor comparison, got {stderr}"
    );
    assert!(
        !harness.trace_exists(paths.trace_stem),
        "forged authority/effect binding must fail before creating runtime trace"
    );
}

#[test]
fn forged_component_authority_surface_binding_rejects_before_trace_side_effects() {
    let paths = ComponentSurfacePaths::new("authority_effect_component_surface_forged");
    let harness = GateHarness::new();
    harness.remove_trace(paths.trace_stem);
    build_component_authority_binding(&harness, &paths);
    let forged = harness.read_text_artifact(paths.binding).replace(
        "\"component_authority\":{\"kind\":\"component_export\",\"component_id\":0}",
        "\"component_authority\":{\"kind\":\"component_export\",\"component_id\":1}",
    );
    harness.write_text_artifact(paths.forged_binding, &forged);

    let run = harness.run_mantle_failure_with_args(
        paths.artifact,
        &[
            "--composition-binding",
            paths.composition_binding,
            "--authority-effect-binding",
            paths.forged_binding,
        ],
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("authority surface does not match runtime artifact"),
        "forged component authority surface should fail closed through typed id comparison, got {stderr}"
    );
    assert!(
        !harness.trace_exists(paths.trace_stem),
        "forged component authority/effect binding must fail before creating runtime trace"
    );
}

#[test]
fn raw_authority_effect_artifact_rejects_as_mantle_binding_before_trace_side_effects() {
    let paths = ScenarioPaths::new("authority_effect_wrong_binding_kind");
    let harness = GateHarness::new();
    harness.remove_trace(paths.trace_stem);
    harness.check(SOURCE);
    harness.build(SOURCE, paths.artifact);
    harness.authority_effect_build(SOURCE, paths.authority_effect);

    let run = harness.run_mantle_failure_with_args(
        paths.artifact,
        &["--authority-effect-binding", paths.authority_effect],
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("field \"schema_id\" must be \"mantle.runtime_authority_effect_binding\""),
        "wrong authority/effect artifact kind should fail through runtime binding schema validation, got {stderr}"
    );
    assert!(
        !harness.trace_exists(paths.trace_stem),
        "raw authority/effect artifact must fail as a Mantle binding before creating runtime trace"
    );
}

#[test]
fn bind_runtime_failure_leaves_no_output_artifact() {
    let paths = ScenarioPaths::new("authority_effect_bind_failure");
    let harness = GateHarness::new();
    harness.check(SOURCE);
    harness.build(SOURCE, paths.artifact);
    harness.authority_effect_build(SOURCE, paths.authority_effect);
    harness.check(MISMATCHED_SOURCE);
    harness.build(MISMATCHED_SOURCE, paths.mismatched_artifact);

    let output = harness.authority_effect_bind_runtime_failure(
        paths.authority_effect,
        paths.mismatched_artifact,
        paths.binding,
        true,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not match"),
        "mismatched runtime bind should report identity mismatch, got {stderr}"
    );
}

#[test]
fn admit_failure_rejects_forged_authority_effect_artifact() {
    let paths = ScenarioPaths::new("authority_effect_admit_failure");
    let harness = GateHarness::new();
    harness.check(SOURCE);
    harness.authority_effect_build(SOURCE, paths.authority_effect);
    let forged = harness.read_text_artifact(paths.authority_effect).replace(
        "{\"kind\":\"spawn\",\"target_process_id\":1,\"target_process\":\"Worker\"}",
        "{\"kind\":\"spawn\",\"target_process_id\":99,\"target_process\":\"Worker\"}",
    );
    harness.write_text_artifact(paths.forged_authority_effect, &forged);

    let output = harness.authority_effect_admit_failure(paths.forged_authority_effect);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("references unknown process id 99"),
        "forged authority/effect artifact should fail through the CLI admission path, got {stderr}"
    );
}

struct ScenarioPaths {
    artifact: &'static str,
    mismatched_artifact: &'static str,
    authority_effect: &'static str,
    forged_authority_effect: &'static str,
    binding: &'static str,
    forged_binding: &'static str,
    trace_stem: &'static str,
}

impl ScenarioPaths {
    fn new(name: &'static str) -> Self {
        match name {
            "authority_effect_accepted" => Self {
                artifact: "target/strata/authority_effect_accepted.mta",
                mismatched_artifact: "target/strata/authority_effect_accepted_mismatch.mta",
                authority_effect: "target/strata/authority_effect_accepted.authority-effect.json",
                forged_authority_effect: "target/strata/authority_effect_accepted.forged-authority-effect.json",
                binding: "target/strata/authority_effect_accepted.authority-effect-binding.json",
                forged_binding: "target/strata/authority_effect_accepted.forged-binding.json",
                trace_stem: "authority_effect_accepted",
            },
            "authority_effect_denied" => Self {
                artifact: "target/strata/authority_effect_denied.mta",
                mismatched_artifact: "target/strata/authority_effect_denied_mismatch.mta",
                authority_effect: "target/strata/authority_effect_denied.authority-effect.json",
                forged_authority_effect: "target/strata/authority_effect_denied.forged-authority-effect.json",
                binding: "target/strata/authority_effect_denied.authority-effect-binding.json",
                forged_binding: "target/strata/authority_effect_denied.forged-binding.json",
                trace_stem: "authority_effect_denied",
            },
            "authority_effect_lexical_supervisor_child" => Self {
                artifact: "target/strata/authority_effect_lexical_supervisor_child.mta",
                mismatched_artifact: "target/strata/authority_effect_lexical_supervisor_child_mismatch.mta",
                authority_effect: "target/strata/authority_effect_lexical_supervisor_child.authority-effect.json",
                forged_authority_effect: "target/strata/authority_effect_lexical_supervisor_child.forged-authority-effect.json",
                binding: "target/strata/authority_effect_lexical_supervisor_child.authority-effect-binding.json",
                forged_binding: "target/strata/authority_effect_lexical_supervisor_child.forged-binding.json",
                trace_stem: "authority_effect_lexical_supervisor_child",
            },
            "authority_effect_forged" => Self {
                artifact: "target/strata/authority_effect_forged.mta",
                mismatched_artifact: "target/strata/authority_effect_forged_mismatch.mta",
                authority_effect: "target/strata/authority_effect_forged.authority-effect.json",
                forged_authority_effect: "target/strata/authority_effect_forged.forged-authority-effect.json",
                binding: "target/strata/authority_effect_forged.authority-effect-binding.json",
                forged_binding: "target/strata/authority_effect_forged.forged-binding.json",
                trace_stem: "authority_effect_forged",
            },
            "authority_effect_wrong_binding_kind" => Self {
                artifact: "target/strata/authority_effect_wrong_binding_kind.mta",
                mismatched_artifact: "target/strata/authority_effect_wrong_binding_kind_mismatch.mta",
                authority_effect: "target/strata/authority_effect_wrong_binding_kind.authority-effect.json",
                forged_authority_effect: "target/strata/authority_effect_wrong_binding_kind.forged-authority-effect.json",
                binding: "target/strata/authority_effect_wrong_binding_kind.authority-effect-binding.json",
                forged_binding: "target/strata/authority_effect_wrong_binding_kind.forged-binding.json",
                trace_stem: "authority_effect_wrong_binding_kind",
            },
            "authority_effect_bind_failure" => Self {
                artifact: "target/strata/authority_effect_bind_failure.mta",
                mismatched_artifact: "target/strata/authority_effect_bind_failure_mismatch.mta",
                authority_effect: "target/strata/authority_effect_bind_failure.authority-effect.json",
                forged_authority_effect: "target/strata/authority_effect_bind_failure.forged-authority-effect.json",
                binding: "target/strata/authority_effect_bind_failure.authority-effect-binding.json",
                forged_binding: "target/strata/authority_effect_bind_failure.forged-binding.json",
                trace_stem: "authority_effect_bind_failure",
            },
            "authority_effect_admit_failure" => Self {
                artifact: "target/strata/authority_effect_admit_failure.mta",
                mismatched_artifact: "target/strata/authority_effect_admit_failure_mismatch.mta",
                authority_effect: "target/strata/authority_effect_admit_failure.authority-effect.json",
                forged_authority_effect: "target/strata/authority_effect_admit_failure.forged-authority-effect.json",
                binding: "target/strata/authority_effect_admit_failure.authority-effect-binding.json",
                forged_binding: "target/strata/authority_effect_admit_failure.forged-binding.json",
                trace_stem: "authority_effect_admit_failure",
            },
            _ => panic!("unknown authority/effect scenario"),
        }
    }
}

struct ComponentSurfacePaths {
    artifact: &'static str,
    authority_effect: &'static str,
    binding: &'static str,
    forged_binding: &'static str,
    composition_artifact: &'static str,
    composition_binding: &'static str,
    trace_stem: &'static str,
}

impl ComponentSurfacePaths {
    fn new(name: &'static str) -> Self {
        match name {
            "authority_effect_component_surfaces" => Self {
                artifact: "target/strata/authority_effect_component_surfaces.mta",
                authority_effect: "target/strata/authority_effect_component_surfaces.authority-effect.json",
                binding: "target/strata/authority_effect_component_surfaces.authority-effect-binding.json",
                forged_binding: "target/strata/authority_effect_component_surfaces.forged-binding.json",
                composition_artifact: "target/strata/authority_effect_component_surfaces.component-composition.json",
                composition_binding: "target/strata/authority_effect_component_surfaces.deployment-composition.json",
                trace_stem: "authority_effect_component_surfaces",
            },
            "authority_effect_component_surface_forged" => Self {
                artifact: "target/strata/authority_effect_component_surface_forged.mta",
                authority_effect: "target/strata/authority_effect_component_surface_forged.authority-effect.json",
                binding: "target/strata/authority_effect_component_surface_forged.authority-effect-binding.json",
                forged_binding: "target/strata/authority_effect_component_surface_forged.forged-binding.json",
                composition_artifact: "target/strata/authority_effect_component_surface_forged.component-composition.json",
                composition_binding: "target/strata/authority_effect_component_surface_forged.deployment-composition.json",
                trace_stem: "authority_effect_component_surface_forged",
            },
            _ => panic!("unknown component authority/effect scenario"),
        }
    }
}

fn build_component_authority_binding(
    harness: &GateHarness,
    paths: &ComponentSurfacePaths,
) -> std::process::Output {
    harness.check(COMPONENT_SOURCE);
    harness.build(COMPONENT_SOURCE, paths.artifact);
    harness.composition_build(COMPONENT_SOURCE, paths.composition_artifact);
    harness.composition_admit(paths.composition_artifact, "json");
    harness.composition_bind_runtime(
        paths.composition_artifact,
        paths.artifact,
        paths.composition_binding,
    );
    harness.authority_effect_build(COMPONENT_SOURCE, paths.authority_effect);
    let admission = harness.authority_effect_admit(paths.authority_effect, "json");
    harness.authority_effect_bind_runtime(
        paths.authority_effect,
        paths.artifact,
        paths.binding,
        false,
    );
    admission
}
