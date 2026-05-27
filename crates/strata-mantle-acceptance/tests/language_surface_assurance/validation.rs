use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::model::{Evidence, EvidenceClass};

struct CachedFile {
    path: &'static str,
    contents: String,
}

pub(crate) struct EvidenceCache {
    root: PathBuf,
    files: Vec<CachedFile>,
}

impl EvidenceCache {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: Vec::new(),
        }
    }

    pub(crate) fn verify(&mut self, evidence: &Evidence) -> Result<(), String> {
        reject_unstable_path(evidence.path)?;
        if evidence.marker.trim().is_empty() {
            return Err(format!("{} has an empty marker", evidence.path));
        }

        let contents = self.contents(evidence.path)?;
        if evidence.class == EvidenceClass::SourceToRuntimeGate {
            return verify_source_to_runtime_gate(evidence, contents);
        }

        if contents.contains(evidence.marker) {
            Ok(())
        } else {
            Err(format!(
                "{} missing {} marker {:?}",
                evidence.path,
                evidence.class.as_str(),
                evidence.marker
            ))
        }
    }

    fn contents(&mut self, path: &'static str) -> Result<&str, String> {
        if let Some(index) = self.files.iter().position(|file| file.path == path) {
            return Ok(&self.files[index].contents);
        }

        let full_path = self.root.join(path);
        let contents = fs::read_to_string(&full_path)
            .map_err(|err| format!("failed to read {}: {err}", full_path.display()))?;
        self.files.push(CachedFile { path, contents });
        let index = self.files.len() - 1;
        Ok(&self.files[index].contents)
    }
}

fn reject_unstable_path(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    if path.as_os_str().is_empty() {
        return Err("evidence path must not be empty".to_string());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(format!(
                    "evidence path {} must stay inside the workspace with normal relative components",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn verify_source_to_runtime_gate(evidence: &Evidence, contents: &str) -> Result<(), String> {
    let path = evidence.path;
    if path == "Justfile" {
        return verify_justfile_source_to_runtime_gate(evidence.marker, contents);
    }

    Err(format!(
        "source-to-runtime evidence path {path:?} must point at Justfile check/build/run commands"
    ))
}

fn verify_justfile_source_to_runtime_gate(source_path: &str, contents: &str) -> Result<(), String> {
    reject_unstable_path(source_path)?;

    let stem = source_path
        .strip_prefix("examples/")
        .and_then(|path| path.strip_suffix(".str"))
        .ok_or_else(|| {
            format!(
                "Justfile source-to-runtime marker {source_path:?} must name an examples/*.str source"
            )
        })?;
    if stem.contains('/') {
        return Err(format!(
            "Justfile source-to-runtime marker {source_path:?} must name a flat examples/*.str source"
        ));
    }

    let lines: Vec<&str> = source_to_runtime_success_recipe_lines(contents)?.collect();
    if looped_source_to_runtime_gate_covers(&lines, stem) {
        return Ok(());
    }

    let artifact_path = format!("target/strata/{stem}.mta");
    let required_commands = [
        format!(
            "cargo +{{{{stable_toolchain}}}} run -p strata --bin strata -- check {source_path}"
        ),
        format!(
            "cargo +{{{{stable_toolchain}}}} run -p strata --bin strata -- build {source_path}"
        ),
        format!(
            "cargo +{{{{stable_toolchain}}}} run -p mantle-runtime --bin mantle -- run {artifact_path}"
        ),
    ];
    let mut seen = [false; 3];

    for line in lines {
        let command = line.trim_start();
        if command.is_empty() || command.starts_with('#') {
            continue;
        }

        for (seen_command, required_command) in seen.iter_mut().zip(required_commands.iter()) {
            if command == required_command {
                *seen_command = true;
            }
        }
    }

    for (was_seen, command) in seen.into_iter().zip(required_commands) {
        if !was_seen {
            return Err(format!(
                "Justfile source-to-runtime marker {source_path:?} is missing executable command {command:?}"
            ));
        }
    }

    Ok(())
}

fn looped_source_to_runtime_gate_covers(lines: &[&str], stem: &str) -> bool {
    active_lines(lines).any(|line| line == "cargo_run=(cargo +{{stable_toolchain}} run)")
        && active_lines(lines).any(|line| line == r#""${cargo_run[@]}" -p strata --bin strata -- check "examples/${example}.str""#)
        && active_lines(lines).any(|line| line == r#""${cargo_run[@]}" -p strata --bin strata -- build "examples/${example}.str""#)
        && active_lines(lines).any(|line| line == r#""${cargo_run[@]}" -p mantle-runtime --bin mantle -- run "target/strata/${example}.mta""#)
        && active_example_stems(lines).any(|example| example == stem)
}

fn active_lines<'a>(lines: &'a [&str]) -> impl Iterator<Item = &'a str> {
    lines.iter().filter_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn active_example_stems<'a>(lines: &'a [&str]) -> impl Iterator<Item = &'a str> {
    active_lines(lines)
        .scan(false, |in_examples, line| {
            if line == "examples=(" {
                *in_examples = true;
                return Some(None);
            }
            if !*in_examples {
                return Some(None);
            }
            if line == ")" {
                *in_examples = false;
                return Some(None);
            }
            Some(Some(line))
        })
        .flatten()
}

fn source_to_runtime_success_recipe_lines(
    contents: &str,
) -> Result<impl Iterator<Item = &str>, String> {
    let mut lines = contents.lines();
    for line in lines.by_ref() {
        if line.starts_with("source-to-runtime-success-gates:") {
            return Ok(lines.take_while(|line| {
                line.trim().is_empty() || line.starts_with(' ') || line.starts_with('\t')
            }));
        }
    }

    Err("Justfile is missing source-to-runtime-success-gates recipe".to_string())
}

#[cfg(test)]
mod tests {
    use super::verify_justfile_source_to_runtime_gate;

    const ACTIVE_HELLO_GATE: &str = r#"source-to-runtime-success-gates: build
    cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/hello.str
    cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/hello.str
    cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/hello.mta

source-to-runtime-failure-gates: build
"#;

    const COMMENTED_HELLO_GATE: &str = r#"source-to-runtime-success-gates: build
    # cargo +{{stable_toolchain}} run -p strata --bin strata -- check examples/hello.str
    # cargo +{{stable_toolchain}} run -p strata --bin strata -- build examples/hello.str
    # cargo +{{stable_toolchain}} run -p mantle-runtime --bin mantle -- run target/strata/hello.mta

source-to-runtime-failure-gates: build
"#;

    const LOOPED_HELLO_GATE: &str = r#"source-to-runtime-success-gates: build
    examples=(
        hello
    )

    cargo_run=(cargo +{{stable_toolchain}} run)
    for example in "${examples[@]}"; do
        "${cargo_run[@]}" -p strata --bin strata -- check "examples/${example}.str"
        "${cargo_run[@]}" -p strata --bin strata -- build "examples/${example}.str"
        "${cargo_run[@]}" -p mantle-runtime --bin mantle -- run "target/strata/${example}.mta"
    done

source-to-runtime-failure-gates: build
"#;

    const LOOPED_COMMENTED_HELLO_GATE: &str = r#"source-to-runtime-success-gates: build
    examples=(
        # hello
    )

    cargo_run=(cargo +{{stable_toolchain}} run)
    for example in "${examples[@]}"; do
        "${cargo_run[@]}" -p strata --bin strata -- check "examples/${example}.str"
        "${cargo_run[@]}" -p strata --bin strata -- build "examples/${example}.str"
        "${cargo_run[@]}" -p mantle-runtime --bin mantle -- run "target/strata/${example}.mta"
    done

source-to-runtime-failure-gates: build
"#;

    #[test]
    fn source_to_runtime_gate_accepts_active_recipe_commands() {
        verify_justfile_source_to_runtime_gate("examples/hello.str", ACTIVE_HELLO_GATE)
            .expect("active Justfile commands should satisfy gate evidence");
    }

    #[test]
    fn source_to_runtime_gate_accepts_looped_recipe_commands() {
        verify_justfile_source_to_runtime_gate("examples/hello.str", LOOPED_HELLO_GATE)
            .expect("active Justfile loop should satisfy gate evidence");
    }

    #[test]
    fn source_to_runtime_gate_rejects_commented_recipe_text() {
        let err =
            verify_justfile_source_to_runtime_gate("examples/hello.str", COMMENTED_HELLO_GATE)
                .expect_err("commented Justfile commands must not satisfy gate evidence");
        assert!(
            err.contains("missing executable command"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn source_to_runtime_gate_rejects_commented_loop_example() {
        let err = verify_justfile_source_to_runtime_gate(
            "examples/hello.str",
            LOOPED_COMMENTED_HELLO_GATE,
        )
        .expect_err("commented loop example must not satisfy gate evidence");
        assert!(
            err.contains("missing executable command"),
            "unexpected error: {err}"
        );
    }
}
