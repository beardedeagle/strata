use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_SOURCE_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(unix)]
const MINIMAL_ROOT_SOURCE: &str = r#"module root;

record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

#[cfg(unix)]
const ROOT_IMPORT_SHARED_SOURCE: &str = r#"module root;
import shared;

record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

#[cfg(not(unix))]
#[test]
fn source_loading_fails_closed_without_secure_file_identity() {
    let path = unique_source_path("unsupported-target");

    let err = read_source_file(&path).expect_err("unsupported source loading should fail closed");

    assert!(
        err.to_string().contains("source loading is unsupported"),
        "{err}"
    );
}

#[cfg(unix)]
#[test]
fn read_source_file_rejects_oversized_source() {
    let path = unique_source_path("oversized");
    fs::write(&path, vec![b'a'; MAX_SOURCE_BYTES + 1])
        .expect("oversized test source should be written");

    let err = read_source_file(&path).expect_err("oversized source should fail");

    assert!(err.to_string().contains("exceeds maximum size"));

    fs::remove_file(path).expect("test source should be removed");
}

#[cfg(unix)]
#[test]
fn read_source_file_rejects_non_utf8_source() {
    let path = unique_source_path("non-utf8");
    fs::write(&path, [0xff]).expect("non-UTF-8 test source should be written");

    let err = read_source_file(&path).expect_err("non-UTF-8 source should fail");

    assert!(err.to_string().contains("is not valid UTF-8"));

    fs::remove_file(path).expect("test source should be removed");
}

#[cfg(unix)]
#[test]
fn read_source_file_rejects_directory_source() {
    let path = unique_source_path("directory");
    fs::create_dir_all(&path).expect("test source dir should be created");

    let err = read_source_file(&path).expect_err("directory source should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_dir(path).expect("test source dir should be removed");
}

#[cfg(unix)]
#[test]
fn read_source_file_rejects_fifo_source_before_opening() {
    let path = unique_source_path("fifo");
    create_fifo(&path);

    let err = read_source_file(&path).expect_err("fifo source should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_file(path).expect("test fifo should be removed");
}

#[cfg(unix)]
#[test]
fn read_source_file_rejects_symlink_source_before_opening() {
    let target = unique_source_path("symlink-target");
    let link = unique_source_path("symlink-link");
    fs::write(&target, "module symlink_target;").expect("test source should be written");
    std::os::unix::fs::symlink(&target, &link).expect("test source symlink should be created");

    let err = read_source_file(&link).expect_err("symlink source should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_file(link).expect("test symlink should be removed");
    fs::remove_file(target).expect("test source should be removed");
}

#[cfg(unix)]
#[test]
fn load_root_source_program_rejects_symlinked_root_source() {
    let dir = unique_source_path("root-symlink-dir");
    fs::create_dir_all(&dir).expect("test source dir should be created");
    write_source(&dir, "root", MINIMAL_ROOT_SOURCE);
    let link = dir.join("linked-root.str");
    std::os::unix::fs::symlink(dir.join("root.str"), &link)
        .expect("test root symlink should be created");

    let err = load_root_source_program(&link).expect_err("symlinked root should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_dir_all(dir).expect("test source dir should be removed");
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
#[test]
fn open_source_file_handle_does_not_block_on_fifo_source() {
    let path = unique_source_path("fifo-handle");
    create_fifo(&path);

    let file = open_source_file_handle(&path).expect("fifo open should not block");
    let metadata = file.metadata().expect("fifo metadata should be available");

    assert!(!metadata.is_file());

    fs::remove_file(path).expect("test fifo should be removed");
}

#[cfg(unix)]
#[test]
fn load_root_source_program_resolves_transitive_sibling_imports() {
    let dir = unique_source_path("imports-dir");
    fs::create_dir_all(&dir).expect("test source dir should be created");
    write_source(&dir, "root", ROOT_IMPORT_SHARED_SOURCE);
    write_source(
        &dir,
        "shared",
        r#"module shared;

record SharedValue;
"#,
    );

    let loaded =
        load_root_source_program(&dir.join("root.str")).expect("root source program should load");
    let (program, _) = loaded.into_parts();

    assert_eq!(program.units().len(), 2);
    assert_eq!(program.dependencies().len(), 1);
    assert_eq!(
        program.dependency_order()[0],
        SourceUnitId::from_index(1).unwrap()
    );
    assert_eq!(program.root_unit().module().name.as_str(), "root");

    fs::remove_dir_all(dir).expect("test source dir should be removed");
}

#[cfg(unix)]
#[test]
fn load_root_source_program_rejects_missing_import_file() {
    let dir = unique_source_path("missing-import-dir");
    fs::create_dir_all(&dir).expect("test source dir should be created");
    let root_source = ROOT_IMPORT_SHARED_SOURCE.replace("import shared;", "import missing;");
    write_source(&dir, "root", &root_source);

    let err = load_root_source_program(&dir.join("root.str"))
        .expect_err("missing import source should fail");

    assert!(err.to_string().contains("missing.str"));

    fs::remove_dir_all(dir).expect("test source dir should be removed");
}

#[cfg(unix)]
#[test]
fn load_root_source_program_rejects_duplicate_module_identity() {
    let dir = unique_source_path("duplicate-module-dir");
    fs::create_dir_all(&dir).expect("test source dir should be created");
    write_source(
        &dir,
        "root",
        r#"module shared;
import shared;

record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#,
    );
    write_source(
        &dir,
        "shared",
        r#"module shared;

record SharedValue;
"#,
    );

    let err = load_root_source_program(&dir.join("root.str"))
        .expect_err("duplicate module source should fail");

    assert!(err.to_string().contains("duplicate module identity shared"));

    fs::remove_dir_all(dir).expect("test source dir should be removed");
}

#[cfg(unix)]
#[test]
fn load_root_source_program_rejects_symlinked_import_escape() {
    let dir = unique_source_path("import-symlink-dir");
    let outside = unique_source_path("import-symlink-outside");
    fs::create_dir_all(&dir).expect("test source dir should be created");
    fs::create_dir_all(&outside).expect("outside test source dir should be created");
    write_source(&dir, "root", ROOT_IMPORT_SHARED_SOURCE);
    write_source(
        &outside,
        "shared",
        r#"module shared;

record SharedValue;
"#,
    );
    std::os::unix::fs::symlink(outside.join("shared.str"), dir.join("shared.str"))
        .expect("test import symlink should be created");

    let err = load_root_source_program(&dir.join("root.str"))
        .expect_err("symlinked import escape should fail");

    assert!(
        err.to_string()
            .contains("resolved outside source directory")
    );

    fs::remove_dir_all(dir).expect("test source dir should be removed");
    fs::remove_dir_all(outside).expect("outside test source dir should be removed");
}

#[cfg(unix)]
#[test]
fn load_root_source_program_rejects_symlinked_sibling_import() {
    let dir = unique_source_path("import-sibling-symlink-dir");
    fs::create_dir_all(&dir).expect("test source dir should be created");
    write_source(&dir, "root", ROOT_IMPORT_SHARED_SOURCE);
    write_source(
        &dir,
        "shared-target",
        r#"module shared;

record SharedValue;
"#,
    );
    std::os::unix::fs::symlink(dir.join("shared-target.str"), dir.join("shared.str"))
        .expect("test import symlink should be created");

    let err = load_root_source_program(&dir.join("root.str"))
        .expect_err("symlinked sibling import should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_dir_all(dir).expect("test source dir should be removed");
}

#[cfg(unix)]
fn write_source(dir: &Path, stem: &str, source: &str) {
    fs::write(dir.join(format!("{stem}.str")), source).expect("test source should be written");
}

#[cfg(unix)]
fn create_fifo(path: &Path) {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;

    mkfifo(path, Mode::S_IRUSR | Mode::S_IWUSR).expect("test fifo should be created");
}

fn unique_source_path(label: &str) -> PathBuf {
    let id = TEST_SOURCE_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("strata-{label}-{}-{id}", std::process::id()))
}
