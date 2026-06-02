use std::env;
use std::path::PathBuf;

use mantle_artifact::{
    AuthoritySummaryFormat, Error, Result, read_artifact, render_artifact_authority_summary,
};

use crate::feature_declaration::validate_artifact_runtime_requirements;
use crate::{
    ProcessStatus, RunLimits, RuntimeFeatureDeclarationFormat, SpawnAuthorityPolicy,
    render_runtime_feature_declaration, run_artifact_path_with_limits,
};

const MANTLE_RUN_USAGE: &str =
    "mantle run <artifact.mta> [--deny-spawn-authority] [--max-runtime-processes N]";

pub fn mantle_main<I>(args: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    match args.next().as_deref() {
        Some("run") => {
            let path = required_path(args.next(), MANTLE_RUN_USAGE)?;
            let limits = run_limits_from_args(args)?;
            let report = run_artifact_path_with_limits(&path, limits)?;
            println!("mantle: loaded {}", report.artifact_path.display());
            for spawned in &report.spawned_processes {
                println!("mantle: spawned {} pid={}", spawned.process, spawned.pid);
            }
            for delivery in &report.delivered_messages {
                println!(
                    "mantle: delivered {} to {}",
                    delivery.message, delivery.process
                );
            }
            for output in &report.emitted_outputs {
                println!("{output}");
            }
            for process in &report.processes {
                match process.status {
                    ProcessStatus::Running => {
                        println!("mantle: process {} remains running", process.process);
                    }
                    ProcessStatus::Stopped => {
                        println!("mantle: stopped {} normally", process.process);
                    }
                    ProcessStatus::Failed => {
                        println!("mantle: failed {} abnormally", process.process);
                    }
                }
            }
            println!("mantle: trace {}", report.trace_path.display());
            Ok(())
        }
        Some("inspect-authority") => {
            let path = required_path(
                args.next(),
                "mantle inspect-authority <artifact.mta> [--format text|json]",
            )?;
            let format = authority_summary_format_from_args(
                args,
                "mantle inspect-authority <artifact.mta> [--format text|json]",
            )?;
            let artifact = read_artifact(&path)?;
            let summary =
                render_artifact_authority_summary(&artifact, &path.display().to_string(), format)?;
            print_summary(&summary);
            Ok(())
        }
        Some("feature-declaration") => {
            let format = runtime_feature_declaration_format_from_args(
                args,
                "mantle feature-declaration [--format text|json]",
            )?;
            let declaration = render_runtime_feature_declaration(format);
            print_summary(&declaration);
            Ok(())
        }
        Some("admit") => {
            let path = required_path(
                args.next(),
                "mantle admit <artifact.mta> [--format text|json]",
            )?;
            let format = runtime_admission_format_from_args(
                args,
                "mantle admit <artifact.mta> [--format text|json]",
            )?;
            let artifact = read_artifact(&path)?;
            validate_artifact_runtime_requirements(&artifact)?;
            let admission =
                render_runtime_admission(&artifact, &path.display().to_string(), format);
            print_summary(&admission);
            Ok(())
        }
        Some("--help") | Some("-h") => {
            print_mantle_usage();
            Ok(())
        }
        Some(other) => Err(Error::new(format!("unknown mantle command {other:?}"))),
        None => {
            print_mantle_usage();
            Err(Error::new("missing mantle command"))
        }
    }
}

fn required_path(value: Option<String>, usage: &str) -> Result<PathBuf> {
    value
        .map(PathBuf::from)
        .ok_or_else(|| Error::new(format!("missing path; usage: {usage}")))
}

fn run_limits_from_args(args: impl IntoIterator<Item = String>) -> Result<RunLimits> {
    let mut limits = RunLimits::default();
    let mut max_runtime_processes_seen = false;
    let mut rest = args.into_iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--deny-spawn-authority" => {
                limits.spawn_authority_policy = SpawnAuthorityPolicy::DenyDeclared;
            }
            "--max-runtime-processes" => {
                if max_runtime_processes_seen {
                    return Err(Error::new("duplicate --max-runtime-processes argument"));
                }
                max_runtime_processes_seen = true;
                let value =
                    required_positive_integer_flag_value(&mut rest, "--max-runtime-processes")?;
                limits.max_runtime_processes =
                    parse_positive_usize_flag("--max-runtime-processes", &value)?;
            }
            other => return Err(Error::new(format!("unexpected argument: {other}"))),
        }
    }
    limits.validate()?;
    Ok(limits)
}

fn required_positive_integer_flag_value(
    rest: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String> {
    let value = rest.next().ok_or_else(|| missing_flag_value_error(flag))?;
    if value.starts_with("--") {
        return Err(missing_flag_value_error(flag));
    }
    Ok(value)
}

fn missing_flag_value_error(flag: &str) -> Error {
    Error::new(format!("missing {flag} value; usage: {MANTLE_RUN_USAGE}"))
}

fn parse_positive_usize_flag(flag: &str, value: &str) -> Result<usize> {
    let parsed = value.parse::<usize>().map_err(|_| {
        Error::new(format!(
            "invalid {flag} value {value:?}; expected a positive integer fitting usize"
        ))
    })?;
    if parsed == 0 {
        return Err(Error::new(format!("{flag} must be greater than zero")));
    }
    Ok(parsed)
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
            other => return Err(Error::new(format!("unexpected argument: {other}"))),
        }
    }
    Ok(format)
}

fn runtime_feature_declaration_format_from_args(
    args: impl IntoIterator<Item = String>,
    usage: &str,
) -> Result<RuntimeFeatureDeclarationFormat> {
    let mut format = RuntimeFeatureDeclarationFormat::Text;
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
                    "text" => RuntimeFeatureDeclarationFormat::Text,
                    "json" => RuntimeFeatureDeclarationFormat::Json,
                    _ => {
                        return Err(Error::new(format!(
                            "unsupported --format value {value:?}; expected text or json"
                        )));
                    }
                };
            }
            other => return Err(Error::new(format!("unexpected argument: {other}"))),
        }
    }
    Ok(format)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeAdmissionFormat {
    Text,
    Json,
}

fn runtime_admission_format_from_args(
    args: impl IntoIterator<Item = String>,
    usage: &str,
) -> Result<RuntimeAdmissionFormat> {
    let mut format = RuntimeAdmissionFormat::Text;
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
                    "text" => RuntimeAdmissionFormat::Text,
                    "json" => RuntimeAdmissionFormat::Json,
                    _ => {
                        return Err(Error::new(format!(
                            "unsupported --format value {value:?}; expected text or json"
                        )));
                    }
                };
            }
            other => return Err(Error::new(format!("unexpected argument: {other}"))),
        }
    }
    Ok(format)
}

fn render_runtime_admission(
    artifact: &mantle_artifact::MantleArtifact,
    subject: &str,
    format: RuntimeAdmissionFormat,
) -> String {
    match format {
        RuntimeAdmissionFormat::Text => render_runtime_admission_text(artifact, subject),
        RuntimeAdmissionFormat::Json => render_runtime_admission_json(artifact, subject),
    }
}

fn render_runtime_admission_text(
    artifact: &mantle_artifact::MantleArtifact,
    subject: &str,
) -> String {
    let mut out = String::new();
    out.push_str("mantle runtime admission accepted ");
    out.push_str(subject);
    out.push('\n');
    out.push_str("format: ");
    out.push_str(artifact.format.as_ref());
    out.push('\n');
    out.push_str("schema_version: ");
    out.push_str(artifact.schema_version.as_ref());
    out.push('\n');
    out.push_str("source_language: ");
    out.push_str(artifact.source_language.as_ref());
    out.push('\n');
    out.push_str("features:\n");
    for feature in &artifact.target_requirements.features {
        out.push_str("  - ");
        out.push_str(feature.as_str());
        out.push('\n');
    }
    out
}

fn render_runtime_admission_json(
    artifact: &mantle_artifact::MantleArtifact,
    subject: &str,
) -> String {
    let mut out = String::new();
    out.push_str("{\"admitted\":true,\"target\":\"");
    push_json_string_body(&mut out, subject);
    out.push_str("\",\"format\":\"");
    push_json_string_body(&mut out, artifact.format.as_ref());
    out.push_str("\",\"schema_version\":\"");
    push_json_string_body(&mut out, artifact.schema_version.as_ref());
    out.push_str("\",\"source_language\":\"");
    push_json_string_body(&mut out, artifact.source_language.as_ref());
    out.push_str("\",\"features\":[");
    for (index, feature) in artifact.target_requirements.features.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        push_json_string_body(&mut out, feature.as_str());
        out.push('"');
    }
    out.push_str("]}");
    out
}

fn push_json_string_body(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => push_json_control_escape(out, c),
            c => out.push(c),
        }
    }
}

fn push_json_control_escape(out: &mut String, ch: char) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let value = ch as usize;
    out.push_str("\\u00");
    out.push(char::from(HEX[(value >> 4) & 0x0f]));
    out.push(char::from(HEX[value & 0x0f]));
}

fn print_summary(summary: &str) {
    print!("{summary}");
    if !summary.ends_with('\n') {
        println!();
    }
}

fn print_mantle_usage() {
    println!("usage:");
    println!("  {MANTLE_RUN_USAGE}");
    println!("  mantle inspect-authority <path.mta> [--format text|json]");
    println!("  mantle feature-declaration [--format text|json]");
    println!("  mantle admit <path.mta> [--format text|json]");
}

pub fn run_mantle_from_env() -> Result<()> {
    mantle_main(env::args())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_limits_parser_accepts_runtime_process_limit() {
        let limits = run_limits_from_args([
            "--max-runtime-processes".to_string(),
            "1".to_string(),
            "--deny-spawn-authority".to_string(),
        ])
        .expect("run limits should parse");

        assert_eq!(limits.max_runtime_processes, 1);
        assert_eq!(
            limits.spawn_authority_policy,
            SpawnAuthorityPolicy::DenyDeclared
        );
    }

    #[test]
    fn run_limits_parser_rejects_missing_runtime_process_limit_value() {
        let err = run_limits_from_args(["--max-runtime-processes".to_string()])
            .expect_err("missing runtime process limit should fail");

        assert!(
            err.to_string()
                .contains("missing --max-runtime-processes value")
        );
    }

    #[test]
    fn run_limits_parser_rejects_flag_token_as_missing_runtime_process_limit_value() {
        let err = run_limits_from_args([
            "--max-runtime-processes".to_string(),
            "--deny-spawn-authority".to_string(),
        ])
        .expect_err("flag token cannot stand in for runtime process limit");

        assert!(
            err.to_string()
                .contains("missing --max-runtime-processes value")
        );
    }

    #[test]
    fn run_limits_parser_rejects_duplicate_runtime_process_limit() {
        let err = run_limits_from_args([
            "--max-runtime-processes".to_string(),
            "1".to_string(),
            "--max-runtime-processes".to_string(),
            "2".to_string(),
        ])
        .expect_err("duplicate runtime process limit should fail");

        assert!(
            err.to_string()
                .contains("duplicate --max-runtime-processes argument")
        );
    }

    #[test]
    fn run_limits_parser_rejects_zero_runtime_process_limit() {
        let err = run_limits_from_args(["--max-runtime-processes".to_string(), "0".to_string()])
            .expect_err("zero runtime process limit should fail");

        assert!(
            err.to_string()
                .contains("--max-runtime-processes must be greater than zero")
        );
    }

    #[test]
    fn run_limits_parser_rejects_invalid_runtime_process_limit() {
        let err = run_limits_from_args(["--max-runtime-processes".to_string(), "many".to_string()])
            .expect_err("invalid runtime process limit should fail");

        assert!(
            err.to_string()
                .contains("invalid --max-runtime-processes value \"many\"")
        );
    }

    #[test]
    fn run_limits_parser_rejects_overflowed_runtime_process_limit() {
        let err = run_limits_from_args([
            "--max-runtime-processes".to_string(),
            "184467440737095516160".to_string(),
        ])
        .expect_err("overflowed runtime process limit should fail");

        assert!(
            err.to_string()
                .contains("invalid --max-runtime-processes value")
        );
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
    fn runtime_feature_declaration_format_parser_accepts_json() {
        let format = runtime_feature_declaration_format_from_args(
            ["--format".to_string(), "json".to_string()],
            "usage",
        )
        .expect("json format should parse");

        assert_eq!(format, RuntimeFeatureDeclarationFormat::Json);
    }

    #[test]
    fn runtime_feature_declaration_format_parser_rejects_unknown_format() {
        let err = runtime_feature_declaration_format_from_args(
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
    fn runtime_admission_format_parser_accepts_json() {
        let format = runtime_admission_format_from_args(
            ["--format".to_string(), "json".to_string()],
            "usage",
        )
        .expect("json format should parse");

        assert_eq!(format, RuntimeAdmissionFormat::Json);
    }

    #[test]
    fn runtime_admission_json_escapes_control_chars_precisely() {
        let mut out = String::new();

        push_json_string_body(&mut out, "artifact\u{0001}\u{001f}\n");

        assert_eq!(out, "artifact\\u0001\\u001f\\n");
    }
}
