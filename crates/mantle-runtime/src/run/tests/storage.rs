use super::support::*;

#[test]
fn loaded_program_stores_large_process_ref_tables_without_runtime_instance_maps() {
    let artifact = artifact_with_large_unbound_process_ref_table();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(&program, &executable, &mut host, RunLimits::default());

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
