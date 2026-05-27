use std::env;
use std::path::PathBuf;

use mantle_artifact::{
    AuthoritySummaryFormat, Error, Result, read_artifact, render_artifact_authority_summary,
};

use crate::{ProcessStatus, RunLimits, SpawnAuthorityPolicy, run_artifact_path_with_limits};

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
}
