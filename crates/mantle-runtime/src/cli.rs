use std::env;
use std::path::PathBuf;

use mantle_artifact::{Error, Result};

use crate::{run_artifact_path, ProcessStatus};

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

fn print_mantle_usage() {
    println!("usage:");
    println!("  mantle run <path.mta>");
}

pub fn run_mantle_from_env() -> Result<()> {
    mantle_main(env::args())
}
