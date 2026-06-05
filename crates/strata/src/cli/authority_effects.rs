use std::path::{Path, PathBuf};

use crate::language::{
    AUTHORITY_EFFECT_ARTIFACT_EXTENSION, AUTHORITY_POLICY_ARTIFACT_EXTENSION,
    AuthorityEffectAdmissionResult, AuthorityEffectArtifactAdmitFormat,
    AuthorityPolicyAdmissionResult, AuthorityPolicyBuildOptions, AuthorityPolicyDecision,
    MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES, MAX_AUTHORITY_POLICY_ARTIFACT_BYTES,
    RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION, admit_authority_effect_artifact,
    admit_authority_policy_artifact, render_authority_effect_admission_summary,
    render_authority_effect_artifact, render_authority_policy_admission_summary,
    render_authority_policy_artifact, render_runtime_authority_effect_binding,
};

use super::{Error, Result, check_source_path, print_summary, required_path};

pub(super) fn command(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("build") => build(args),
        Some("admit") => admit(args),
        Some("policy") => policy(args),
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
        "strata authority-effects bind-runtime <authority-effect.json> <authority-policy.json> <artifact.mta> [--output <path.json>]",
    )?;
    let authority_policy_path = required_path(
        args.next(),
        "strata authority-effects bind-runtime <authority-effect.json> <authority-policy.json> <artifact.mta> [--output <path.json>]",
    )?;
    let artifact_path = required_path(
        args.next(),
        "strata authority-effects bind-runtime <authority-effect.json> <authority-policy.json> <artifact.mta> [--output <path.json>]",
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
                    "strata authority-effects bind-runtime <authority-effect.json> <authority-policy.json> <artifact.mta> --output <path.json>",
                )?);
            }
            other => return Err(Error::new(format!("unexpected argument {other:?}"))),
        }
    }

    let authority_effect_text = mantle_artifact::read_text_artifact(
        &authority_effect_path,
        MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES,
    )?;
    let authority_policy_text = mantle_artifact::read_text_artifact(
        &authority_policy_path,
        MAX_AUTHORITY_POLICY_ARTIFACT_BYTES,
    )?;
    let artifact = mantle_artifact::read_artifact(&artifact_path)?;
    let binding = render_runtime_authority_effect_binding(
        &authority_effect_text,
        &authority_policy_text,
        &artifact,
    )?;
    let binding_path = output.unwrap_or(default_runtime_binding_path(&artifact_path)?);
    mantle_artifact::write_text_artifact(&binding_path, &binding)?;
    println!(
        "strata: bound authority/effect {} and policy {} to runtime artifact {} -> {}",
        authority_effect_path.display(),
        authority_policy_path.display(),
        artifact_path.display(),
        binding_path.display()
    );
    Ok(())
}

fn policy(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("build") => policy_build(args),
        Some("admit") => policy_admit(args),
        Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        Some(other) => Err(Error::new(format!(
            "unknown strata authority-effects policy command {other:?}"
        ))),
        None => Err(Error::new(
            "missing strata authority-effects policy command",
        )),
    }
}

fn policy_build(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    let authority_effect_path = required_path(
        args.next(),
        "strata authority-effects policy build <authority-effect.json> [--deny-spawn-authority] [--deny-port-authority] [--output <path.json>]",
    )?;
    let mut output = None;
    let mut options = AuthorityPolicyBuildOptions::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--deny-spawn-authority" => {
                if options.spawn_authority_decision == AuthorityPolicyDecision::Deny {
                    return Err(Error::new("duplicate --deny-spawn-authority argument"));
                }
                options.spawn_authority_decision = AuthorityPolicyDecision::Deny;
            }
            "--deny-port-authority" => {
                if options.port_authority_decision == AuthorityPolicyDecision::Deny {
                    return Err(Error::new("duplicate --deny-port-authority argument"));
                }
                options.port_authority_decision = AuthorityPolicyDecision::Deny;
            }
            "--output" => {
                if output.is_some() {
                    return Err(Error::new("duplicate --output argument"));
                }
                output = Some(required_path(
                    args.next(),
                    "strata authority-effects policy build <authority-effect.json> --output <path.json>",
                )?);
            }
            other => return Err(Error::new(format!("unexpected argument {other:?}"))),
        }
    }
    let authority_effect_text = mantle_artifact::read_text_artifact(
        &authority_effect_path,
        MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES,
    )?;
    let policy = render_authority_policy_artifact(&authority_effect_text, options)?;
    let policy_path = output.unwrap_or(default_policy_path(&authority_effect_path)?);
    mantle_artifact::write_text_artifact(&policy_path, &policy)?;
    println!(
        "strata: built authority policy {} -> {}",
        authority_effect_path.display(),
        policy_path.display()
    );
    Ok(())
}

fn policy_admit(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    let policy_path = required_path(
        args.next(),
        "strata authority-effects policy admit <authority-policy.json> <authority-effect.json> [--format text|json]",
    )?;
    let authority_effect_path = required_path(
        args.next(),
        "strata authority-effects policy admit <authority-policy.json> <authority-effect.json> [--format text|json]",
    )?;
    let format = artifact_admit_format_from_args(
        args,
        "strata authority-effects policy admit <authority-policy.json> <authority-effect.json> [--format text|json]",
    )?;
    let policy_text =
        mantle_artifact::read_text_artifact(&policy_path, MAX_AUTHORITY_POLICY_ARTIFACT_BYTES)?;
    let authority_effect_text = mantle_artifact::read_text_artifact(
        &authority_effect_path,
        MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES,
    )?;
    let summary = admit_authority_policy_artifact(&policy_text, &authority_effect_text)?;
    let rendered = render_authority_policy_admission_summary(
        &summary,
        &policy_path.display().to_string(),
        format,
    );
    print_summary(&rendered);
    if summary.admission_result != AuthorityPolicyAdmissionResult::Admitted {
        return Err(Error::new("authority policy artifact admission rejected"));
    }
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

fn default_policy_path(authority_effect_path: &Path) -> Result<PathBuf> {
    let stem = authority_effect_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            Error::new(format!(
                "authority/effect path {} has no UTF-8 file stem",
                authority_effect_path.display()
            ))
        })?
        .trim_end_matches(".authority-effect");
    Ok(Path::new("target")
        .join("strata")
        .join(format!("{stem}.{AUTHORITY_POLICY_ARTIFACT_EXTENSION}")))
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
        "  strata authority-effects policy build <authority-effect.json> [--deny-spawn-authority] [--deny-port-authority] [--output <path.json>]"
    );
    println!(
        "  strata authority-effects policy admit <authority-policy.json> <authority-effect.json> [--format text|json]"
    );
    println!(
        "  strata authority-effects bind-runtime <authority-effect.json> <authority-policy.json> <artifact.mta> [--output <path.json>]"
    );
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEST_ARTIFACT_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn policy_help_flags_are_successful() {
        policy(["--help".to_string()]).expect("policy --help should print usage");
        policy(["-h".to_string()]).expect("policy -h should print usage");
    }

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

        remove_test_files([link, target]);
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

        remove_test_files([link, authority_effect]);
    }

    #[test]
    fn policy_build_rejects_symlink_output_path_without_touching_target() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("policy-build-input");
        let target = unique_policy_path("policy-build-target");
        let link = unique_policy_path("policy-build-link");
        fs::remove_file(&link).ok();
        fs::remove_file(&target).ok();
        build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            authority_effect.display().to_string(),
        ])
        .expect("authority/effect artifact should build for policy output symlink test");
        fs::create_dir_all(target.parent().expect("test target should have a parent"))
            .expect("test policy directory should be created");
        fs::write(&target, "unchanged").expect("test symlink target should be written");
        symlink(&target, &link).expect("test policy output symlink should be created");

        let err = policy_build([
            authority_effect.display().to_string(),
            "--output".to_string(),
            link.display().to_string(),
        ])
        .expect_err("authority policy output symlink should fail closed");

        assert_non_regular_artifact_error(&err);
        assert_eq!(
            fs::read_to_string(&target).expect("symlink target should remain readable"),
            "unchanged"
        );

        remove_test_files([link, target, authority_effect]);
    }

    #[test]
    fn policy_admit_rejects_symlink_policy_input_path() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("policy-admit-authority-effect");
        let policy = unique_policy_path("policy-admit-policy");
        let link = unique_policy_path("policy-admit-policy-link");
        write_authority_effect_and_policy(&authority_effect, &policy);
        symlink(&policy, &link).expect("test policy input symlink should be created");

        let err = policy_admit([
            link.display().to_string(),
            authority_effect.display().to_string(),
        ])
        .expect_err("authority policy input symlink should fail closed");

        assert_non_regular_artifact_error(&err);

        remove_test_files([link, policy, authority_effect]);
    }

    #[test]
    fn policy_admit_rejects_symlink_authority_effect_input_path() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("policy-admit-effect");
        let policy = unique_policy_path("policy-admit-policy-effect");
        let link = unique_artifact_path("policy-admit-effect-link");
        write_authority_effect_and_policy(&authority_effect, &policy);
        symlink(&authority_effect, &link)
            .expect("test authority/effect input symlink should be created");

        let err = policy_admit([policy.display().to_string(), link.display().to_string()])
            .expect_err("authority/effect policy-admit input symlink should fail closed");

        assert_non_regular_artifact_error(&err);

        remove_test_files([link, policy, authority_effect]);
    }

    #[test]
    fn bind_runtime_parses_deny_policy_and_default_output_path() {
        let authority_effect = unique_artifact_path("authority-effect");
        let policy = unique_policy_path("authority-policy");
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
        policy_build([
            authority_effect.display().to_string(),
            "--deny-spawn-authority".to_string(),
            "--output".to_string(),
            policy.display().to_string(),
        ])
        .expect("authority policy artifact should build");
        write_runtime_artifact(&runtime_artifact);

        bind_runtime([
            authority_effect.display().to_string(),
            policy.display().to_string(),
            runtime_artifact.display().to_string(),
        ])
        .expect("authority/effect binding should build");

        let text = fs::read_to_string(&binding).expect("binding should be readable");
        assert!(text.contains("\"schema_id\":\"mantle.runtime_authority_effect_binding\""));
        assert!(text.contains("\"policy_decisions\":[{"));
        assert!(text.contains("\"decision\":\"deny\""));

        remove_test_files([binding, policy, authority_effect, runtime_artifact]);
    }

    #[test]
    fn bind_runtime_rejects_symlink_authority_effect_input_path() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("bind-input-artifact");
        let policy = unique_policy_path("bind-input-policy");
        let runtime_artifact = unique_runtime_artifact_path("bind-input-runtime");
        let binding = unique_path(
            "bind-input-binding",
            RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION,
        );
        let link = unique_artifact_path("bind-input-link");
        write_authority_effect_and_policy(&authority_effect, &policy);
        write_runtime_artifact(&runtime_artifact);
        symlink(&authority_effect, &link).expect("test input symlink should be created");

        let err = bind_runtime([
            link.display().to_string(),
            policy.display().to_string(),
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

        remove_test_files([link, policy, authority_effect, runtime_artifact]);
    }

    #[test]
    fn bind_runtime_rejects_symlink_policy_input_path() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("bind-policy-artifact");
        let policy = unique_policy_path("bind-policy-input");
        let policy_link = unique_policy_path("bind-policy-link");
        let runtime_artifact = unique_runtime_artifact_path("bind-policy-runtime");
        let binding = unique_path(
            "bind-policy-binding",
            RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION,
        );
        write_authority_effect_and_policy(&authority_effect, &policy);
        write_runtime_artifact(&runtime_artifact);
        symlink(&policy, &policy_link).expect("test policy input symlink should be created");

        let err = bind_runtime([
            authority_effect.display().to_string(),
            policy_link.display().to_string(),
            runtime_artifact.display().to_string(),
            "--output".to_string(),
            binding.display().to_string(),
        ])
        .expect_err("bind-runtime authority policy input symlink should fail closed");

        assert_non_regular_artifact_error(&err);
        assert!(
            !binding.exists(),
            "failed authority/effect bind-runtime must not leave {}",
            binding.display()
        );

        remove_test_files([policy_link, policy, authority_effect, runtime_artifact]);
    }

    #[test]
    fn bind_runtime_rejects_symlink_runtime_artifact_input_path() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("bind-runtime-artifact");
        let policy = unique_policy_path("bind-runtime-policy");
        let runtime_artifact = unique_runtime_artifact_path("bind-runtime-target");
        let runtime_link = unique_runtime_artifact_path("bind-runtime-link");
        let binding = unique_path(
            "bind-runtime-binding",
            RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION,
        );
        write_authority_effect_and_policy(&authority_effect, &policy);
        write_runtime_artifact(&runtime_artifact);
        symlink(&runtime_artifact, &runtime_link)
            .expect("test runtime artifact symlink should be created");

        let err = bind_runtime([
            authority_effect.display().to_string(),
            policy.display().to_string(),
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

        remove_test_files([runtime_link, policy, authority_effect, runtime_artifact]);
    }

    #[test]
    fn bind_runtime_rejects_symlink_output_path_without_touching_target() {
        use std::os::unix::fs::symlink;

        let authority_effect = unique_artifact_path("authority-effect-symlink");
        let policy = unique_policy_path("authority-policy-symlink");
        let runtime_artifact = unique_runtime_artifact_path("runtime-symlink");
        let target = unique_artifact_path("binding-target");
        let link = unique_artifact_path("binding-link");
        fs::remove_file(&link).ok();
        fs::remove_file(&target).ok();
        fs::create_dir_all(target.parent().expect("test target should have a parent"))
            .expect("test artifact directory should be created");
        fs::write(&target, "unchanged").expect("test symlink target should be written");
        symlink(&target, &link).expect("test output symlink should be created");

        write_authority_effect_and_policy(&authority_effect, &policy);
        write_runtime_artifact(&runtime_artifact);

        let err = bind_runtime([
            authority_effect.display().to_string(),
            policy.display().to_string(),
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

        remove_test_files([link, target, policy, authority_effect, runtime_artifact]);
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

    fn write_authority_effect_and_policy(authority_effect: &Path, policy: &Path) {
        build([
            example_source_path().display().to_string(),
            "--output".to_string(),
            authority_effect.display().to_string(),
        ])
        .expect("authority/effect artifact should build");
        policy_build([
            authority_effect.display().to_string(),
            "--output".to_string(),
            policy.display().to_string(),
        ])
        .expect("authority policy artifact should build");
    }

    fn remove_test_files<const N: usize>(paths: [PathBuf; N]) {
        for path in paths {
            fs::remove_file(path).expect("test file should be removed");
        }
    }

    fn unique_artifact_path(label: &str) -> PathBuf {
        unique_path(label, AUTHORITY_EFFECT_ARTIFACT_EXTENSION)
    }

    fn unique_runtime_artifact_path(label: &str) -> PathBuf {
        unique_path(label, "mta")
    }

    fn unique_policy_path(label: &str) -> PathBuf {
        unique_path(label, AUTHORITY_POLICY_ARTIFACT_EXTENSION)
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
