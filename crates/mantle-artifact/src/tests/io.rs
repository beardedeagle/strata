use super::support::*;

#[test]
fn write_artifact_rejects_invalid_artifacts_before_writing() {
    let dir = unique_test_dir("invalid-artifact-write");
    let path = dir.join("bad.mta");
    let mut artifact = valid_artifact();
    artifact.format = "invalid-format".to_string();

    let err = write_artifact(&path, &artifact).expect_err("invalid artifact should fail");

    assert!(err.to_string().contains("unsupported artifact format"));
    assert!(!path.exists(), "invalid artifact must not be written");
    assert!(
        !dir.exists(),
        "invalid artifact must not create parent dirs"
    );
}

#[test]
fn write_artifact_accepts_current_directory_output_path() {
    let path = unique_current_dir_artifact_path("artifact-current-dir");
    let artifact = valid_artifact();

    write_artifact(&path, &artifact).expect("current-directory artifact write should succeed");

    let decoded = read_artifact(&path).expect("written artifact should decode");
    assert_eq!(decoded, artifact);

    fs::remove_file(path).expect("test artifact should be removed");
}

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

#[test]
fn read_artifact_rejects_oversized_file() {
    let path = unique_current_dir_artifact_path("artifact-too-large");
    fs::write(&path, vec![b'a'; MAX_ARTIFACT_BYTES + 1])
        .expect("oversized test file should be written");

    let err = read_artifact(&path).expect_err("oversized artifact file should fail");

    assert!(err.to_string().contains("is too large"));

    fs::remove_file(path).expect("test artifact should be removed");
}

#[test]
fn read_artifact_rejects_directory_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-directory");
    fs::create_dir_all(&path).expect("test artifact dir should be created");

    let err = read_artifact(&path).expect_err("directory artifact path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_dir(path).expect("test artifact dir should be removed");
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
