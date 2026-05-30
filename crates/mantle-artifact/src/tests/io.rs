use super::support::*;

#[test]
fn write_artifact_rejects_invalid_artifacts_before_writing() {
    let dir = unique_test_dir("invalid-artifact-write");
    let path = dir.join("bad.mta");
    let mut artifact = valid_artifact();
    artifact.format = "invalid-format".into();

    let err = write_artifact(&path, &artifact).expect_err("invalid artifact should fail");

    assert!(err.to_string().contains("unsupported artifact format"));
    assert!(!path.exists(), "invalid artifact must not be written");
    assert!(
        !dir.exists(),
        "invalid artifact must not create parent dirs"
    );
}

#[cfg(any(unix, windows))]
#[test]
fn write_artifact_accepts_current_directory_output_path() {
    let path = unique_current_dir_artifact_path("artifact-current-dir");
    let artifact = valid_artifact();

    write_artifact(&path, &artifact).expect("current-directory artifact write should succeed");

    let decoded = read_artifact(&path).expect("written artifact should decode");
    assert_eq!(decoded, artifact);

    fs::remove_file(path).expect("test artifact should be removed");
}

#[cfg(any(unix, windows))]
#[test]
fn write_artifact_rejects_directory_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-write-directory");
    fs::create_dir_all(&path).expect("test artifact dir should be created");
    let artifact = valid_artifact();

    let err =
        write_artifact(&path, &artifact).expect_err("directory artifact output path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_dir(path).expect("test artifact dir should be removed");
}

#[cfg(unix)]
#[test]
fn write_artifact_rejects_fifo_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-write-fifo");
    create_fifo(&path);
    let artifact = valid_artifact();

    let err = write_artifact(&path, &artifact).expect_err("fifo artifact output path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_file(path).expect("test fifo should be removed");
}

#[cfg(unix)]
#[test]
fn write_artifact_rejects_symlink_output_path_without_touching_target() {
    use std::os::unix::fs::symlink;

    let target = unique_current_dir_artifact_path("artifact-write-symlink-target");
    let link = unique_current_dir_artifact_path("artifact-write-symlink-link");
    fs::write(&target, "unchanged").expect("test symlink target should be written");
    symlink(&target, &link).expect("test symlink should be created");
    let artifact = valid_artifact();

    let err = write_artifact(&link, &artifact).expect_err("symlink output path should fail");

    assert!(err.to_string().contains("symbolic link component"));
    assert_eq!(
        fs::read_to_string(&target).expect("target should be readable"),
        "unchanged"
    );

    fs::remove_file(link).expect("test symlink should be removed");
    fs::remove_file(target).expect("test target should be removed");
}

#[cfg(unix)]
#[test]
fn write_artifact_rejects_symlink_parent_path() {
    use std::os::unix::fs::symlink;

    let real_dir = unique_test_dir("artifact-write-real-parent");
    let link_dir = unique_test_dir("artifact-write-link-parent");
    fs::create_dir_all(&real_dir).expect("test real dir should be created");
    symlink(&real_dir, &link_dir).expect("test parent symlink should be created");
    let output = link_dir.join("out.mta");
    let artifact = valid_artifact();

    let err = write_artifact(&output, &artifact).expect_err("symlink parent path should fail");

    assert!(err.to_string().contains("symbolic link component"));
    assert!(
        !real_dir.join("out.mta").exists(),
        "symlink parent must not receive artifact output"
    );

    fs::remove_file(link_dir).expect("test parent symlink should be removed");
    fs::remove_dir(real_dir).expect("test real dir should be removed");
}

#[cfg(any(unix, windows))]
#[test]
fn read_artifact_rejects_oversized_file() {
    let path = unique_current_dir_artifact_path("artifact-too-large");
    fs::write(&path, vec![b'a'; MAX_ARTIFACT_BYTES + 1])
        .expect("oversized test file should be written");

    let err = read_artifact(&path).expect_err("oversized artifact file should fail");

    assert!(err.to_string().contains("is too large"));

    fs::remove_file(path).expect("test artifact should be removed");
}

#[cfg(any(unix, windows))]
#[test]
fn read_artifact_rejects_directory_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-directory");
    fs::create_dir_all(&path).expect("test artifact dir should be created");

    let err = read_artifact(&path).expect_err("directory artifact path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_dir(path).expect("test artifact dir should be removed");
}

#[cfg(all(not(unix), not(windows)))]
#[test]
fn artifact_io_fails_closed_without_secure_file_identity_support() {
    let output = unique_current_dir_artifact_path("artifact-unsupported-output");
    let artifact = valid_artifact();

    let write_err =
        write_artifact(&output, &artifact).expect_err("unsupported artifact write should fail");

    assert!(
        write_err
            .to_string()
            .contains("secure file identity support")
    );
    assert!(!output.exists(), "unsupported write must not create output");

    let input = unique_current_dir_artifact_path("artifact-unsupported-input");
    fs::write(&input, artifact.encode()).expect("test artifact should be written");

    let read_err = read_artifact(&input).expect_err("unsupported artifact read should fail");

    assert!(
        read_err
            .to_string()
            .contains("secure file identity support")
    );
    fs::remove_file(input).expect("test artifact should be removed");
}

#[cfg(unix)]
#[test]
fn read_artifact_rejects_fifo_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-fifo");
    create_fifo(&path);

    let err = read_artifact(&path).expect_err("fifo artifact path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_file(path).expect("test fifo should be removed");
}

#[cfg(unix)]
#[test]
fn read_artifact_rejects_symlink_path() {
    use std::os::unix::fs::symlink;

    let target = unique_current_dir_artifact_path("artifact-read-symlink-target");
    let link = unique_current_dir_artifact_path("artifact-read-symlink-link");
    write_artifact(&target, &valid_artifact()).expect("target artifact should be written");
    symlink(&target, &link).expect("test symlink should be created");

    let err = read_artifact(&link).expect_err("symlink artifact path should fail");

    assert!(err.to_string().contains("symbolic link component"));

    fs::remove_file(link).expect("test symlink should be removed");
    fs::remove_file(target).expect("test target should be removed");
}
