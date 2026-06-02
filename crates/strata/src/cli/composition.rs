use std::path::{Path, PathBuf};

use crate::language::{
    COMPONENT_COMPOSITION_ARTIFACT_EXTENSION, ComponentCompositionAdmissionResult,
    ComponentCompositionArtifactAdmitFormat, MAX_COMPONENT_COMPOSITION_ARTIFACT_BYTES,
    admit_component_composition_artifact, render_component_composition_admission_summary,
    render_component_composition_artifact,
};

use super::{Error, Result, check_source_path, print_summary, required_path};

pub(super) fn command(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("build") => build(args),
        Some("admit") => admit(args),
        Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        Some(other) => Err(Error::new(format!(
            "unknown strata composition command {other:?}"
        ))),
        None => {
            print_usage();
            Err(Error::new("missing strata composition command"))
        }
    }
}

fn build(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    let path = required_path(
        args.next(),
        "strata composition build <path.str> [--composition <name>] [--output <path.json>]",
    )?;
    let mut composition_name = None;
    let mut output = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--composition" => {
                if composition_name.is_some() {
                    return Err(Error::new("duplicate --composition argument"));
                }
                composition_name = Some(args.next().ok_or_else(|| {
                    Error::new(
                        "missing --composition value; usage: strata composition build <path.str> --composition <name>",
                    )
                })?);
            }
            "--output" => {
                if output.is_some() {
                    return Err(Error::new("duplicate --output argument"));
                }
                output = Some(required_path(
                    args.next(),
                    "strata composition build <path.str> --output <path.json>",
                )?);
            }
            other => return Err(Error::new(format!("unexpected argument {other:?}"))),
        }
    }
    let (checked, source_hash) = check_source_path(&path)?;
    let artifact = render_component_composition_artifact(
        &checked,
        &path.display().to_string(),
        &source_hash,
        composition_name.as_deref(),
    )?;
    let artifact_path =
        output.unwrap_or(default_artifact_path(&path, composition_name.as_deref())?);
    mantle_artifact::write_text_artifact(&artifact_path, &artifact)?;
    println!(
        "strata: built composition {} -> {}",
        path.display(),
        artifact_path.display()
    );
    Ok(())
}

fn admit(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    let path = required_path(
        args.next(),
        "strata composition admit <path.json> [--format text|json]",
    )?;
    let format = artifact_admit_format_from_args(
        args,
        "strata composition admit <path.json> [--format text|json]",
    )?;
    let text =
        mantle_artifact::read_text_artifact(&path, MAX_COMPONENT_COMPOSITION_ARTIFACT_BYTES)?;
    let summary = admit_component_composition_artifact(&text)?;
    let rendered = render_component_composition_admission_summary(
        &summary,
        &path.display().to_string(),
        format,
    );
    print_summary(&rendered);
    if summary.admission_result != ComponentCompositionAdmissionResult::Admitted {
        return Err(Error::new(
            "component composition artifact admission rejected",
        ));
    }
    Ok(())
}

fn default_artifact_path(source_path: &Path, composition_name: Option<&str>) -> Result<PathBuf> {
    let stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            Error::new(format!(
                "source path {} has no UTF-8 file stem",
                source_path.display()
            ))
        })?;
    let artifact_stem = match composition_name {
        Some(name) => format!("{stem}.{name}"),
        None => stem.to_string(),
    };
    Ok(Path::new("target").join("strata").join(format!(
        "{artifact_stem}.{COMPONENT_COMPOSITION_ARTIFACT_EXTENSION}"
    )))
}

pub(super) fn artifact_admit_format_from_args(
    args: impl IntoIterator<Item = String>,
    usage: &str,
) -> Result<ComponentCompositionArtifactAdmitFormat> {
    let mut format = ComponentCompositionArtifactAdmitFormat::Text;
    let mut format_seen = false;
    let mut rest = args.into_iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--format" => {
                if format_seen {
                    return Err(Error::new("duplicate --format argument"));
                }
                format_seen = true;
                let value = rest
                    .next()
                    .ok_or_else(|| Error::new(format!("missing --format value; usage: {usage}")))?;
                format = match value.as_str() {
                    "text" => ComponentCompositionArtifactAdmitFormat::Text,
                    "json" => ComponentCompositionArtifactAdmitFormat::Json,
                    _ => {
                        return Err(Error::new(format!(
                            "unsupported --format value {value:?}; expected text or json"
                        )));
                    }
                };
            }
            other => return Err(Error::new(format!("unexpected argument {other:?}"))),
        }
    }
    Ok(format)
}

fn print_usage() {
    println!("usage:");
    println!("  strata composition build <path.str> [--composition <name>] [--output <path.json>]");
    println!("  strata composition admit <path.json> [--format text|json]");
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEST_ARTIFACT_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn build_rejects_symlink_output_path_without_touching_target() {
        use std::os::unix::fs::symlink;

        let target = unique_composition_artifact_path("output-symlink-target");
        let link = unique_composition_artifact_path("output-symlink-link");
        fs::create_dir_all(target.parent().expect("test target should have a parent"))
            .expect("test artifact directory should be created");
        fs::write(&target, "unchanged").expect("test symlink target should be written");
        symlink(&target, &link).expect("test output symlink should be created");

        let err = build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            link.display().to_string(),
        ])
        .expect_err("composition artifact output symlink should fail closed");

        assert_non_regular_artifact_error(&err);
        assert_eq!(
            fs::read_to_string(&target).expect("symlink target should remain readable"),
            "unchanged",
            "composition build must not write through a symlink output path"
        );

        fs::remove_file(link).expect("test output symlink should be removed");
        fs::remove_file(target).expect("test symlink target should be removed");
    }

    #[test]
    fn admit_rejects_symlink_input_path() {
        use std::os::unix::fs::symlink;

        let artifact = unique_composition_artifact_path("input-artifact");
        let link = unique_composition_artifact_path("input-symlink");
        build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            artifact.display().to_string(),
        ])
        .expect("composition artifact should build for input symlink test");
        symlink(&artifact, &link).expect("test input symlink should be created");

        let err = admit([link.display().to_string()])
            .expect_err("composition artifact input symlink should fail closed");

        assert_non_regular_artifact_error(&err);

        fs::remove_file(link).expect("test input symlink should be removed");
        fs::remove_file(artifact).expect("test composition artifact should be removed");
    }

    #[test]
    fn admit_exits_nonzero_for_rejected_artifact() {
        let artifact = unique_composition_artifact_path("rejected-artifact");
        build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            artifact.display().to_string(),
        ])
        .expect("composition artifact should build for rejected-admission test");
        let rejected = fs::read_to_string(&artifact)
            .expect("composition artifact should be readable")
            .replace(
                "\"binding_result\":\"admitted\",\"rejection_reason\":\"\"",
                "\"binding_result\":\"rejected\",\"rejection_reason\":\"forged rejection\"",
            )
            .replace(
                "\"admission_result\":\"admitted\"",
                "\"admission_result\":\"rejected\"",
            );
        mantle_artifact::write_text_artifact(&artifact, &rejected)
            .expect("rejected composition artifact should be written");

        let err = admit([artifact.display().to_string()])
            .expect_err("rejected composition artifact should fail the CLI gate");

        assert!(
            err.to_string()
                .contains("component composition artifact admission rejected"),
            "unexpected rejected artifact diagnostic: {err}"
        );

        fs::remove_file(artifact).expect("test composition artifact should be removed");
    }

    #[test]
    fn build_rejects_multi_composition_source_without_selector() {
        let source = write_selector_source("missing-selector");

        let err = build([source.display().to_string()])
            .expect_err("multi-composition source should require an explicit selector");

        assert!(
            err.to_string().contains("pass --composition <name>"),
            "unexpected multi-composition diagnostic: {err}"
        );

        fs::remove_file(source).expect("test source should be removed");
    }

    #[test]
    fn build_rejects_unknown_composition_selector() {
        let source = write_selector_source("unknown-selector");

        let err = build([
            source.display().to_string(),
            "--composition".to_string(),
            "MissingComposition".to_string(),
        ])
        .expect_err("unknown composition selector should fail closed");

        assert!(
            err.to_string()
                .contains("source program declares no composition MissingComposition"),
            "unexpected selector diagnostic: {err}"
        );

        fs::remove_file(source).expect("test source should be removed");
    }

    #[test]
    fn build_selector_writes_composition_specific_default_artifact() {
        let source = write_selector_source("selected-composition");
        let stem = source
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("test source should have a UTF-8 stem");
        let artifact = Path::new("target").join("strata").join(format!(
            "{stem}.AltComposition.{COMPONENT_COMPOSITION_ARTIFACT_EXTENSION}"
        ));

        build([
            source.display().to_string(),
            "--composition".to_string(),
            "AltComposition".to_string(),
        ])
        .expect("selected composition should build");

        let text = fs::read_to_string(&artifact)
            .expect("selected default composition artifact should be written");
        assert!(text.contains("\"composition_name\":\"AltComposition\""));
        assert!(text.contains("\"composition_id\":1"));

        fs::remove_file(artifact).expect("selected composition artifact should be removed");
        fs::remove_file(source).expect("test source should be removed");
    }

    fn assert_non_regular_artifact_error(err: &Error) {
        let Error::Artifact(artifact_err) = err else {
            panic!("expected secure artifact I/O error, got {err}");
        };
        let message = artifact_err.to_string();
        assert!(
            message.contains("is not a regular file")
                || message.contains("must not include symbolic link component"),
            "expected non-regular artifact path diagnostic, got {message}"
        );
    }

    fn unique_composition_artifact_path(name: &str) -> PathBuf {
        let index = TEST_ARTIFACT_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        Path::new("target").join(format!(
            "strata-artifact-{name}-{}-{index}.component-composition.json",
            std::process::id()
        ))
    }

    fn write_selector_source(name: &str) -> PathBuf {
        let index = TEST_ARTIFACT_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = Path::new("target").join(format!(
            "strata-selector-source-{name}-{}-{index}.str",
            std::process::id()
        ));
        fs::create_dir_all(path.parent().expect("test source should have a parent"))
            .expect("test source directory should be created");
        fs::write(&path, selector_source()).expect("test selector source should be written");
        path
    }

    fn example_source_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/component_composition_main.str")
    }

    fn selector_source() -> &'static str {
        r#"module composition_selector_test;

record MainState;
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Work }
protocol MainProtocol message MainMsg requires Cap<ProtocolBoundary<MainProtocol>>;
protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port MainPort protocol MainProtocol target Main requires Cap<PortConnect<MainPort>>;
port WorkerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerPort>>;
component MainComponent exports MainPort imports WorkerPort requires Cap<ComponentExport<MainComponent>>;
component WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;
composition AppComposition {
    instance main component MainComponent;
    instance worker component WorkerComponent;
    bind main imports WorkerPort -> worker exports WorkerPort;
}
composition AltComposition {
    instance alt_main component MainComponent;
    instance alt_worker component WorkerComponent;
    bind alt_main imports WorkerPort -> alt_worker exports WorkerPort;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;
    authority connect_worker: Cap<PortConnect<WorkerPort>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker via WorkerPort Work;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#
    }
}
