use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

use mantle_artifact::{
    TargetRequirementsFormat, render_artifact_target_requirements, write_artifact,
};

use crate::language::{
    AuthoritySummaryFormat, CheckedProgram, CompositionAdmissionReport,
    CompositionAdmissionReportFormat, SourceProvenanceHash, check_source_program,
    lower_to_artifact_with_source_hash, render_authority_summary,
    render_composition_admission_report,
};
use crate::source_loader::{LoadedSourceProgram, load_root_source_program};

mod authority_effects;
mod composition;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Message(String),
    Language(crate::language::Error),
    SourceLoad(crate::source_loader::Error),
    Artifact(mantle_artifact::Error),
    Io(std::io::Error),
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            Self::Language(err) => write!(f, "{err}"),
            Self::SourceLoad(err) => write!(f, "{err}"),
            Self::Artifact(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Message(_) => None,
            Self::Language(err) => Some(err),
            Self::SourceLoad(err) => Some(err),
            Self::Artifact(err) => Some(err),
            Self::Io(err) => Some(err),
        }
    }
}

impl From<crate::language::Error> for Error {
    fn from(value: crate::language::Error) -> Self {
        Self::Language(value)
    }
}

impl From<crate::source_loader::Error> for Error {
    fn from(value: crate::source_loader::Error) -> Self {
        Self::SourceLoad(value)
    }
}

impl From<mantle_artifact::Error> for Error {
    fn from(value: mantle_artifact::Error) -> Self {
        Self::Artifact(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn strata_main<I>(args: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    match args.next().as_deref() {
        Some("check") => {
            let path = required_path(args.next(), "strata check <path>")?;
            ensure_no_extra_args(args)?;
            let (checked, source_hash) = check_source_path(&path)?;
            let _artifact = lower_to_artifact_with_source_hash(&checked, source_hash)?;
            let entry = checked.entry_process_label()?;
            println!(
                "strata: checked {} (module {}, entry {})",
                path.display(),
                checked.module_name(),
                entry
            );
            Ok(())
        }
        Some("build") => {
            let path = required_path(args.next(), "strata build <path> [--output <path>]")?;
            let mut output = None;
            let mut rest = args.peekable();
            while let Some(arg) = rest.next() {
                if arg == "--output" {
                    if output.is_some() {
                        return Err(Error::new("duplicate --output argument"));
                    }
                    output = Some(required_path(
                        rest.next(),
                        "strata build <path> --output <path>",
                    )?);
                } else {
                    return Err(Error::new(format!("unexpected argument {arg:?}")));
                }
            }
            let (checked, source_hash) = check_source_path(&path)?;
            let artifact = lower_to_artifact_with_source_hash(&checked, source_hash)?;
            let artifact_path = output.unwrap_or(default_artifact_path(&path)?);
            write_artifact(&artifact_path, &artifact)?;
            println!(
                "strata: built {} -> {}",
                path.display(),
                artifact_path.display()
            );
            Ok(())
        }
        Some("authority-summary") => {
            let path = required_path(
                args.next(),
                "strata authority-summary <path.str> [--format text|json]",
            )?;
            let format = authority_summary_format_from_args(
                args,
                "strata authority-summary <path.str> [--format text|json]",
            )?;
            let (checked, _) = check_source_path(&path)?;
            let summary = render_authority_summary(&checked, &path.display().to_string(), format);
            print_summary(&summary);
            Ok(())
        }
        Some("authority-effects") => authority_effects::command(args),
        Some("composition-report") => {
            let path = required_path(
                args.next(),
                "strata composition-report <path.str> [--format text|json]",
            )?;
            let format = composition_report_format_from_args(
                args,
                "strata composition-report <path.str> [--format text|json]",
            )?;
            let (checked, source_hash) = check_source_path(&path)?;
            let admission_report =
                CompositionAdmissionReport::from_checked_parts(checked, source_hash);
            let report = render_composition_admission_report(
                &admission_report,
                &path.display().to_string(),
                format,
            );
            print_summary(&report);
            Ok(())
        }
        Some("composition") => composition::command(args),
        Some("target-requirements") => {
            let path = required_path(
                args.next(),
                "strata target-requirements <path.str> [--format text|json]",
            )?;
            let format = target_requirements_format_from_args(
                args,
                "strata target-requirements <path.str> [--format text|json]",
            )?;
            let (checked, source_hash) = check_source_path(&path)?;
            let artifact = lower_to_artifact_with_source_hash(&checked, source_hash)?;
            let requirements = render_artifact_target_requirements(
                &artifact,
                &path.display().to_string(),
                format,
            )?;
            print_summary(&requirements);
            Ok(())
        }
        Some("--help") | Some("-h") => {
            print_strata_usage();
            Ok(())
        }
        Some(other) => Err(Error::new(format!("unknown strata command {other:?}"))),
        None => {
            print_strata_usage();
            Err(Error::new("missing strata command"))
        }
    }
}

fn required_path(value: Option<String>, usage: &str) -> Result<PathBuf> {
    value
        .map(PathBuf::from)
        .ok_or_else(|| Error::new(format!("missing path; usage: {usage}")))
}

fn default_artifact_path(source_path: &Path) -> Result<PathBuf> {
    let stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            Error::new(format!(
                "source path {} has no UTF-8 file stem",
                source_path.display()
            ))
        })?;
    Ok(Path::new("target")
        .join("strata")
        .join(format!("{stem}.mta")))
}

fn check_source_path(path: &Path) -> Result<(CheckedProgram, SourceProvenanceHash)> {
    let loaded = load_root_source_program(path)?;
    check_loaded_source(loaded)
}

fn check_loaded_source(
    loaded: LoadedSourceProgram,
) -> Result<(CheckedProgram, SourceProvenanceHash)> {
    let (program, source_hash) = loaded.into_parts();
    let checked = check_source_program(program)?;
    Ok((checked, source_hash))
}

fn ensure_no_extra_args(args: impl IntoIterator<Item = String>) -> Result<()> {
    let extras: Vec<String> = args.into_iter().collect();
    if extras.is_empty() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "unexpected arguments: {}",
            extras.join(" ")
        )))
    }
}

fn authority_summary_format_from_args(
    args: impl IntoIterator<Item = String>,
    usage: &str,
) -> Result<AuthoritySummaryFormat> {
    let mut format = AuthoritySummaryFormat::Text;
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
                    "text" => AuthoritySummaryFormat::Text,
                    "json" => AuthoritySummaryFormat::Json,
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

fn composition_report_format_from_args(
    args: impl IntoIterator<Item = String>,
    usage: &str,
) -> Result<CompositionAdmissionReportFormat> {
    let mut format = CompositionAdmissionReportFormat::Text;
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
                    "text" => CompositionAdmissionReportFormat::Text,
                    "json" => CompositionAdmissionReportFormat::Json,
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

fn target_requirements_format_from_args(
    args: impl IntoIterator<Item = String>,
    usage: &str,
) -> Result<TargetRequirementsFormat> {
    let mut format = TargetRequirementsFormat::Text;
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
                    "text" => TargetRequirementsFormat::Text,
                    "json" => TargetRequirementsFormat::Json,
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

fn print_summary(summary: &str) {
    print!("{summary}");
    if !summary.ends_with('\n') {
        println!();
    }
}

fn print_strata_usage() {
    println!("usage:");
    println!("  strata check <path.str>");
    println!("  strata build <path.str> [--output <path.mta>]");
    println!("  strata composition build <path.str> [--composition <name>] [--output <path.json>]");
    println!("  strata composition admit <path.json> [--format text|json]");
    println!(
        "  strata composition bind-runtime <composition.json> <artifact.mta> [--output <path.json>]"
    );
    println!("  strata authority-effects build <path.str> [--output <path.json>]");
    println!("  strata authority-effects admit <path.json> [--format text|json]");
    println!(
        "  strata authority-effects bind-runtime <authority-effect.json> <artifact.mta> [--deny-spawn-authority] [--output <path.json>]"
    );
    println!("  strata authority-summary <path.str> [--format text|json]");
    println!("  strata composition-report <path.str> [--format text|json]");
    println!("  strata target-requirements <path.str> [--format text|json]");
}

pub fn run_strata_from_env() -> Result<()> {
    strata_main(env::args())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(any(unix, windows))]
    use crate::language::MAX_SOURCE_BYTES;
    #[cfg(any(unix, windows))]
    use mantle_artifact::{MAX_ACTIONS_PER_PROCESS, MAX_OUTPUT_LITERALS};

    static TEST_SOURCE_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn strata_build_rejects_duplicate_output_argument() {
        let err = strata_main([
            "strata".to_string(),
            "build".to_string(),
            "examples/hello.str".to_string(),
            "--output".to_string(),
            "one.mta".to_string(),
            "--output".to_string(),
            "two.mta".to_string(),
        ])
        .expect_err("duplicate output should fail");

        assert!(err.to_string().contains("duplicate --output argument"));
    }

    #[test]
    fn composition_build_rejects_duplicate_output_argument() {
        let err = strata_main([
            "strata".to_string(),
            "composition".to_string(),
            "build".to_string(),
            "examples/component_composition_main.str".to_string(),
            "--output".to_string(),
            "one.json".to_string(),
            "--output".to_string(),
            "two.json".to_string(),
        ])
        .expect_err("duplicate output should fail");

        assert!(err.to_string().contains("duplicate --output argument"));
    }

    #[test]
    fn composition_build_rejects_duplicate_composition_argument() {
        let err = strata_main([
            "strata".to_string(),
            "composition".to_string(),
            "build".to_string(),
            "examples/component_composition_main.str".to_string(),
            "--composition".to_string(),
            "One".to_string(),
            "--composition".to_string(),
            "Two".to_string(),
        ])
        .expect_err("duplicate composition selector should fail");

        assert!(err.to_string().contains("duplicate --composition argument"));
    }

    #[test]
    fn authority_summary_format_parser_accepts_json() {
        let format = authority_summary_format_from_args(
            ["--format".to_string(), "json".to_string()],
            "usage",
        )
        .expect("json format should parse");

        assert_eq!(format, AuthoritySummaryFormat::Json);
    }

    #[test]
    fn authority_summary_format_parser_rejects_duplicate_format() {
        let err = authority_summary_format_from_args(
            [
                "--format".to_string(),
                "text".to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
            "usage",
        )
        .expect_err("duplicate format should fail");

        assert!(err.to_string().contains("duplicate --format argument"));
    }

    #[test]
    fn composition_report_format_parser_accepts_json() {
        let format = composition_report_format_from_args(
            ["--format".to_string(), "json".to_string()],
            "usage",
        )
        .expect("json format should parse");

        assert_eq!(format, CompositionAdmissionReportFormat::Json);
    }

    #[test]
    fn composition_report_format_parser_rejects_unknown_format() {
        let err = composition_report_format_from_args(
            ["--format".to_string(), "yaml".to_string()],
            "usage",
        )
        .expect_err("unknown format should fail");

        assert!(
            err.to_string()
                .contains("unsupported --format value \"yaml\"")
        );
    }

    #[test]
    fn composition_artifact_admit_format_parser_accepts_json() {
        let format = composition::artifact_admit_format_from_args(
            ["--format".to_string(), "json".to_string()],
            "usage",
        )
        .expect("json format should parse");

        assert_eq!(
            format,
            crate::language::ComponentCompositionArtifactAdmitFormat::Json
        );
    }

    #[test]
    fn composition_artifact_admit_format_parser_rejects_unknown_format() {
        let err = composition::artifact_admit_format_from_args(
            ["--format".to_string(), "yaml".to_string()],
            "usage",
        )
        .expect_err("unknown format should fail");

        assert!(
            err.to_string()
                .contains("unsupported --format value \"yaml\"")
        );
    }

    #[test]
    fn target_requirements_format_parser_accepts_json() {
        let format = target_requirements_format_from_args(
            ["--format".to_string(), "json".to_string()],
            "usage",
        )
        .expect("json format should parse");

        assert_eq!(format, TargetRequirementsFormat::Json);
    }

    #[test]
    fn target_requirements_format_parser_rejects_unknown_format() {
        let err = target_requirements_format_from_args(
            ["--format".to_string(), "yaml".to_string()],
            "usage",
        )
        .expect_err("unknown format should fail");

        assert!(
            err.to_string()
                .contains("unsupported --format value \"yaml\"")
        );
    }

    #[cfg(all(not(unix), not(windows)))]
    #[test]
    fn strata_check_fails_closed_without_secure_source_identity_support() {
        let path = unique_source_path("unsupported-secure-source");
        fs::write(&path, "module unsupported_secure_source;")
            .expect("test source should be written");

        let err = strata_main([
            "strata".to_string(),
            "check".to_string(),
            path.display().to_string(),
        ])
        .expect_err("check should fail before loading unsupported source path");

        let Error::SourceLoad(source_err) = err else {
            panic!("expected source loading error, got {err}");
        };
        assert!(
            source_err
                .to_string()
                .contains("source file identity cannot be checked securely")
        );

        fs::remove_file(path).expect("test source should be removed");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn strata_check_rejects_source_that_cannot_lower_to_artifact() {
        let path = unique_source_path("artifact-too-large-check");
        fs::write(&path, oversized_artifact_source())
            .expect("oversized-artifact test source should be written");

        let err = strata_main([
            "strata".to_string(),
            "check".to_string(),
            path.display().to_string(),
        ])
        .expect_err("check should fail when lowering rejects the checked source");

        assert_artifact_size_error(&err);

        fs::remove_file(path).expect("test source should be removed");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn strata_build_rejects_lowering_failure_before_writing_output() {
        let source_path = unique_source_path("artifact-too-large-build");
        let output_path = unique_artifact_path("artifact-too-large-build-output");
        fs::write(&source_path, oversized_artifact_source())
            .expect("oversized-artifact test source should be written");

        let err = strata_main([
            "strata".to_string(),
            "build".to_string(),
            source_path.display().to_string(),
            "--output".to_string(),
            output_path.display().to_string(),
        ])
        .expect_err("build should fail when lowering rejects the checked source");

        assert_artifact_size_error(&err);
        assert!(
            !output_path.exists(),
            "build must not write an artifact after lowering failure"
        );

        fs::remove_file(source_path).expect("test source should be removed");
    }

    #[cfg(any(unix, windows))]
    fn assert_artifact_size_error(err: &Error) {
        let Error::Artifact(artifact_err) = err else {
            panic!("expected artifact lowering error, got {err}");
        };
        assert!(
            artifact_err
                .to_string()
                .contains("encoded artifact exceeds maximum size")
        );
    }

    #[cfg(any(unix, windows))]
    fn oversized_artifact_source() -> String {
        let emit_count = MAX_OUTPUT_LITERALS.min(MAX_ACTIONS_PER_PROCESS);
        let output_padding = "x".repeat(190);
        let mut source = String::from(
            r#"module oversized_artifact;
record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {
"#,
        );
        for index in 0..emit_count {
            source.push_str(&format!(
                "        emit \"oversized-artifact-output-{index:04}-{output_padding}\";\n"
            ));
        }
        source.push_str(
            r#"        return Stop(state);
    }
}
"#,
        );
        assert!(
            source.len() <= MAX_SOURCE_BYTES,
            "test source must stay below the source size limit"
        );
        source
    }

    fn unique_source_path(name: &str) -> PathBuf {
        let index = TEST_SOURCE_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "strata-source-{name}-{}-{index}.str",
            std::process::id()
        ))
    }

    #[cfg(any(unix, windows))]
    fn unique_artifact_path(name: &str) -> PathBuf {
        let index = TEST_SOURCE_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "strata-artifact-{name}-{}-{index}.mta",
            std::process::id()
        ))
    }
}
