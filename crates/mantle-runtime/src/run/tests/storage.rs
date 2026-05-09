use super::support::*;

#[test]
fn loaded_program_stores_large_process_ref_tables_without_runtime_instance_maps() {
    let artifact = artifact_with_large_unbound_process_ref_table();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(
        &program,
        &mut host,
        DEFAULT_MAX_RUNTIME_PROCESSES,
        DEFAULT_MAX_TRACE_BYTES,
        DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
    );

    let pid = run
        .spawn_process(ProcessId::new(0), None)
        .expect("entry process should spawn");

    assert_eq!(pid, RuntimeProcessId::FIRST);
    assert_eq!(
        program
            .process(ProcessId::new(0))
            .expect("entry process should load")
            .process_refs
            .len(),
        MAX_PROCESS_REFS_PER_PROCESS
    );
}
