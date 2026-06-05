use std::path::{Path, PathBuf};

use crate::language::{
    AUTHORITY_EFFECT_ARTIFACT_EXTENSION, AuthorityEffectAdmissionResult,
    AuthorityEffectArtifactAdmitFormat, MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES,
    RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION, RuntimeSpawnAuthorityPolicy,
    admit_authority_effect_artifact, render_authority_effect_admission_summary,
    render_authority_effect_artifact, render_runtime_authority_effect_binding,
};

use super::{Error, Result, check_source_path, print_summary, required_path};

pub(super) fn command(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("build") => build(args),
        Some("admit") => admit(args),
        Some("bind-runtime") => bind_runtime(args),
        Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        Some(other) => Err(Error::new(format!(
            "unknown strata authority-effects command {other:?}"
        ))),
        None => {
            print_usage();
            Err(Error::new("missing strata authority-effects command"))
        }
    }
}

fn build(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    let path = required_path(
        args.next(),
        "strata authority-effects build <path.str> [--output <path.json>]",
    )?;
    let mut output = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => {
                if output.is_some() {
                    return Err(Error::new("duplicate --output argument"));
                }
                output = Some(required_path(
                    args.next(),
                    "strata authority-effects build <path.str> --output <path.json>",
                )?);
            }
            other => return Err(Error::new(format!("unexpected argument {other:?}"))),
        }
    }
    let (checked, source_hash) = check_source_path(&path)?;
    let artifact =
        render_authority_effect_artifact(&checked, &path.display().to_string(), &source_hash)?;
    let artifact_path = output.unwrap_or(default_artifact_path(&path)?);
    mantle_artifact::write_text_artifact(&artifact_path, &artifact)?;
    println!(
        "strata: built authority/effect {} -> {}",
        path.display(),
        artifact_path.display()
    );
    Ok(())
}

fn admit(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    let path = required_path(
        args.next(),
        "strata authority-effects admit <path.json> [--format text|json]",
    )?;
    let format = artifact_admit_format_from_args(
        args,
        "strata authority-effects admit <path.json> [--format text|json]",
    )?;
    let text = mantle_artifact::read_text_artifact(&path, MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES)?;
    let summary = admit_authority_effect_artifact(&text)?;
    let rendered =
        render_authority_effect_admission_summary(&summary, &path.display().to_string(), format);
    print_summary(&rendered);
    if summary.admission_result != AuthorityEffectAdmissionResult::Admitted {
        return Err(Error::new("authority/effect artifact admission rejected"));
    }
    Ok(())
}

fn bind_runtime(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    let authority_effect_path = required_path(
        args.next(),
        "strata authority-effects bind-runtime <authority-effect.json> <artifact.mta> [--deny-spawn-authority] [--output <path.json>]",
    )?;
    let artifact_path = required_path(
        args.next(),
        "strata authority-effects bind-runtime <authority-effect.json> <artifact.mta> [--deny-spawn-authority] [--output <path.json>]",
    )?;
    let mut output = None;
    let mut spawn_policy = RuntimeSpawnAuthorityPolicy::AdmitDeclared;
    let mut spawn_policy_seen = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--deny-spawn-authority" => {
                if spawn_policy_seen {
                    return Err(Error::new("duplicate spawn authority policy argument"));
                }
                spawn_policy_seen = true;
                spawn_policy = RuntimeSpawnAuthorityPolicy::DenyDeclared;
            }
            "--output" => {
                if output.is_some() {
                    return Err(Error::new("duplicate --output argument"));
                }
                output = Some(required_path(
                    args.next(),
                    "strata authority-effects bind-runtime <authority-effect.json> <artifact.mta> --output <path.json>",
                )?);
            }
            other => return Err(Error::new(format!("unexpected argument {other:?}"))),
        }
    }

    let authority_effect_text = mantle_artifact::read_text_artifact(
        &authority_effect_path,
        MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES,
    )?;
    let artifact = mantle_artifact::read_artifact(&artifact_path)?;
    let binding =
        render_runtime_authority_effect_binding(&authority_effect_text, &artifact, spawn_policy)?;
    let binding_path = output.unwrap_or(default_runtime_binding_path(&artifact_path)?);
    mantle_artifact::write_text_artifact(&binding_path, &binding)?;
    println!(
        "strata: bound authority/effect {} to runtime artifact {} -> {}",
        authority_effect_path.display(),
        artifact_path.display(),
        binding_path.display()
    );
    Ok(())
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
        .join(format!("{stem}.{AUTHORITY_EFFECT_ARTIFACT_EXTENSION}")))
}

fn default_runtime_binding_path(artifact_path: &Path) -> Result<PathBuf> {
    let stem = artifact_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            Error::new(format!(
                "artifact path {} has no UTF-8 file stem",
                artifact_path.display()
            ))
        })?;
    Ok(Path::new("target").join("strata").join(format!(
        "{stem}.{RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION}"
    )))
}

pub(super) fn artifact_admit_format_from_args(
    args: impl IntoIterator<Item = String>,
    usage: &str,
) -> Result<AuthorityEffectArtifactAdmitFormat> {
    let mut format = AuthorityEffectArtifactAdmitFormat::Text;
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
                    "text" => AuthorityEffectArtifactAdmitFormat::Text,
                    "json" => AuthorityEffectArtifactAdmitFormat::Json,
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
    println!("  strata authority-effects build <path.str> [--output <path.json>]");
    println!("  strata authority-effects admit <path.json> [--format text|json]");
    println!(
        "  strata authority-effects bind-runtime <authority-effect.json> <artifact.mta> [--deny-spawn-authority] [--output <path.json>]"
    );
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEST_ARTIFACT_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn build_rejects_symlink_output_path_without_touching_target() {
        use std::os::unix::fs::symlink;

        let target = unique_artifact_path("output-target");
        let link = unique_artifact_path("output-link");
        fs::remove_file(&link).ok();
        fs::remove_file(&target).ok();
        fs::create_dir_all(target.parent().expect("test target should have a parent"))
            .expect("test artifact directory should be created");
        fs::write(&target, "unchanged").expect("test symlink target should be written");
        symlink(&target, &link).expect("test output symlink should be created");

        let err = build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            link.display().to_string(),
        ])
        .expect_err("authority/effect output symlink should fail closed");

        assert_non_regular_artifact_error(&err);
        assert_eq!(
            fs::read_to_string(&target).expect("symlink target should remain readable"),
            "unchanged"
        );

        fs::remove_file(link).expect("test output symlink should be removed");
        fs::remove_file(target).expect("test target should be removed");
    }

    #[test]
    fn admit_rejects_symlink_input_path() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("input-artifact");
        let link = unique_artifact_path("input-link");
        build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            authority_effect.display().to_string(),
        ])
        .expect("authority/effect artifact should build for input symlink test");
        symlink(&authority_effect, &link).expect("test input symlink should be created");

        let err = admit([link.display().to_string()])
            .expect_err("authority/effect input symlink should fail closed");

        assert_non_regular_artifact_error(&err);

        fs::remove_file(link).expect("test input symlink should be removed");
        fs::remove_file(authority_effect)
            .expect("test authority/effect artifact should be removed");
    }

    #[test]
    fn bind_runtime_parses_deny_policy_and_default_output_path() {
        let authority_effect = unique_artifact_path("authority-effect");
        let runtime_artifact = unique_runtime_artifact_path("runtime");
        let binding = default_runtime_binding_path(&runtime_artifact)
            .expect("test runtime artifact should have a binding path");
        fs::remove_file(&binding).ok();

        build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            authority_effect.display().to_string(),
        ])
        .expect("authority/effect artifact should build");
        write_runtime_artifact(&runtime_artifact);

        bind_runtime([
            authority_effect.display().to_string(),
            runtime_artifact.display().to_string(),
            "--deny-spawn-authority".to_string(),
        ])
        .expect("authority/effect binding should build");

        let text = fs::read_to_string(&binding).expect("binding should be readable");
        assert!(text.contains("\"schema_id\":\"mantle.runtime_authority_effect_binding\""));
        assert!(text.contains("\"spawn_authority_policy\":\"deny_declared\""));

        fs::remove_file(binding).expect("test binding should be removed");
        fs::remove_file(authority_effect)
            .expect("test authority/effect artifact should be removed");
        fs::remove_file(runtime_artifact).expect("test runtime artifact should be removed");
    }

    #[test]
    fn bind_runtime_rejects_symlink_authority_effect_input_path() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("bind-input-artifact");
        let runtime_artifact = unique_runtime_artifact_path("bind-input-runtime");
        let binding = unique_path(
            "bind-input-binding",
            RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION,
        );
        let link = unique_artifact_path("bind-input-link");
        build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            authority_effect.display().to_string(),
        ])
        .expect("authority/effect artifact should build for bind-runtime input symlink test");
        write_runtime_artifact(&runtime_artifact);
        symlink(&authority_effect, &link).expect("test input symlink should be created");

        let err = bind_runtime([
            link.display().to_string(),
            runtime_artifact.display().to_string(),
            "--output".to_string(),
            binding.display().to_string(),
        ])
        .expect_err("bind-runtime authority/effect input symlink should fail closed");

        assert_non_regular_artifact_error(&err);
        assert!(
            !binding.exists(),
            "failed authority/effect bind-runtime must not leave {}",
            binding.display()
        );

        fs::remove_file(link).expect("test input symlink should be removed");
        fs::remove_file(authority_effect)
            .expect("test authority/effect artifact should be removed");
        fs::remove_file(runtime_artifact).expect("test runtime artifact should be removed");
    }

    #[test]
    fn bind_runtime_rejects_symlink_runtime_artifact_input_path() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("bind-runtime-artifact");
        let runtime_artifact = unique_runtime_artifact_path("bind-runtime-target");
        let runtime_link = unique_runtime_artifact_path("bind-runtime-link");
        let binding = unique_path(
            "bind-runtime-binding",
            RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION,
        );
        build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            authority_effect.display().to_string(),
        ])
        .expect("authority/effect artifact should build for runtime symlink test");
        write_runtime_artifact(&runtime_artifact);
        symlink(&runtime_artifact, &runtime_link)
            .expect("test runtime artifact symlink should be created");

        let err = bind_runtime([
            authority_effect.display().to_string(),
            runtime_link.display().to_string(),
            "--output".to_string(),
            binding.display().to_string(),
        ])
        .expect_err("bind-runtime runtime artifact symlink should fail closed");

        assert_non_regular_artifact_error(&err);
        assert!(
            !binding.exists(),
            "failed authority/effect bind-runtime must not leave {}",
            binding.display()
        );

        fs::remove_file(runtime_link).expect("test runtime symlink should be removed");
        fs::remove_file(authority_effect)
            .expect("test authority/effect artifact should be removed");
        fs::remove_file(runtime_artifact).expect("test runtime artifact should be removed");
    }

    #[test]
    fn bind_runtime_rejects_symlink_output_path_without_touching_target() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("authority-effect-symlink");
        let runtime_artifact = unique_runtime_artifact_path("runtime-symlink");
        let target = unique_artifact_path("binding-target");
        let link = unique_artifact_path("binding-link");
        fs::remove_file(&link).ok();
        fs::remove_file(&target).ok();
        fs::create_dir_all(target.parent().expect("test target should have a parent"))
            .expect("test artifact directory should be created");
        fs::write(&target, "unchanged").expect("test symlink target should be written");
        symlink(&target, &link).expect("test output symlink should be created");

        build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            authority_effect.display().to_string(),
        ])
        .expect("authority/effect artifact should build");
        write_runtime_artifact(&runtime_artifact);

        let err = bind_runtime([
            authority_effect.display().to_string(),
            runtime_artifact.display().to_string(),
            "--output".to_string(),
            link.display().to_string(),
        ])
        .expect_err("authority/effect runtime binding output symlink should fail closed");

        assert_non_regular_artifact_error(&err);
        assert_eq!(
            fs::read_to_string(&target).expect("symlink target should remain readable"),
            "unchanged"
        );

        fs::remove_file(link).expect("test output symlink should be removed");
        fs::remove_file(target).expect("test target should be removed");
        fs::remove_file(authority_effect)
            .expect("test authority/effect artifact should be removed");
        fs::remove_file(runtime_artifact).expect("test runtime artifact should be removed");
    }

    #[test]
    fn admit_format_parser_rejects_duplicate_format() {
        let err = artifact_admit_format_from_args(
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

    fn example_source_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/effect_outcome_spawn_denied.str")
    }

    fn write_runtime_artifact(path: &Path) {
        let (checked, source_hash) = check_source_path(&example_source_path())
            .expect("example source should check for test runtime artifact");
        let artifact = crate::language::lower_to_artifact_with_source_hash(&checked, source_hash)
            .expect("example source should lower for test runtime artifact");
        mantle_artifact::write_artifact(path, &artifact)
            .expect("test runtime artifact should be written");
    }

    fn unique_artifact_path(label: &str) -> PathBuf {
        unique_path(label, AUTHORITY_EFFECT_ARTIFACT_EXTENSION)
    }

    fn unique_runtime_artifact_path(label: &str) -> PathBuf {
        unique_path(label, "mta")
    }

    fn unique_path(label: &str, extension: &str) -> PathBuf {
        let unique = TEST_ARTIFACT_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        Path::new("target")
            .join("strata-tests")
            .join(format!("authority-effect-cli-{label}-{unique}.{extension}"))
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
}
