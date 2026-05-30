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

pub fn mantle_main<I>(args: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    match args.next().as_deref() {
        Some("run") => {
            let path = required_path(args.next(), "mantle run <artifact.mta>")?;
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
    for arg in args {
        match arg.as_str() {
            "--deny-spawn-authority" => {
                limits.spawn_authority_policy = SpawnAuthorityPolicy::DenyDeclared;
            }
            other => return Err(Error::new(format!("unexpected argument: {other}"))),
        }
    }
    Ok(limits)
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
    println!("  mantle run <path.mta> [--deny-spawn-authority]");
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
