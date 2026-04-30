use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::artifact::{default_artifact_path, write_artifact, MantleArtifact};
use crate::language::check_source;
use crate::runtime::{run_artifact_path, ProcessStatus};
use crate::{Error, Result};

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
            let source = fs::read_to_string(&path)?;
            let checked = check_source(&source)?;
            println!(
                "strata: checked {} (module {}, entry {})",
                path.display(),
                checked.module.name,
                checked.entry_process
            );
            Ok(())
        }
        Some("build") => {
            let path = required_path(args.next(), "strata build <path> [--output <path>]")?;
            let mut output = None;
            let mut rest = args.peekable();
            while let Some(arg) = rest.next() {
                if arg == "--output" {
                    output = Some(required_path(
                        rest.next(),
                        "strata build <path> --output <path>",
                    )?);
                } else {
                    return Err(Error::new(format!("unexpected argument {arg:?}")));
                }
            }
            let source = fs::read_to_string(&path)?;
            let checked = check_source(&source)?;
            let artifact = MantleArtifact::from_checked(&checked, &source)?;
            let artifact_path = output.unwrap_or(default_artifact_path(&path)?);
            write_artifact(&artifact_path, &artifact)?;
            println!(
                "strata: built {} -> {}",
                path.display(),
                artifact_path.display()
            );
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

pub fn mantle_main<I>(args: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    match args.next().as_deref() {
        Some("run") => {
            let path = required_path(args.next(), "mantle run <artifact.mta>")?;
            ensure_no_extra_args(args)?;
            let report = run_artifact_path(&path)?;
            println!("mantle: loaded {}", report.artifact_path.display());
            println!("mantle: spawned {} pid=1", report.process);
            println!("mantle: delivered {}", report.message);
            for output in &report.emitted_outputs {
                println!("{output}");
            }
            match report.status {
                ProcessStatus::Running => {
                    println!("mantle: process {} remains running", report.process);
                }
                ProcessStatus::Stopped => {
                    println!("mantle: stopped {} normally", report.process);
                }
            }
            println!("mantle: trace {}", report.trace_path.display());
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

fn print_strata_usage() {
    println!("usage:");
    println!("  strata check <path.str>");
    println!("  strata build <path.str> [--output <path.mta>]");
}

fn print_mantle_usage() {
    println!("usage:");
    println!("  mantle run <path.mta>");
}

pub fn run_strata_from_env() -> Result<()> {
    strata_main(env::args())
}

pub fn run_mantle_from_env() -> Result<()> {
    mantle_main(env::args())
}

#[allow(dead_code)]
fn _is_path(value: &Path) -> bool {
    !value.as_os_str().is_empty()
}
