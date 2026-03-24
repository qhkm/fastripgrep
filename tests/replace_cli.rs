use std::fs;
use std::process::{Command, Output};

use tempfile::TempDir;

fn run_frg<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_frg"))
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .unwrap()
}

fn output_text(output: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn test_replace_preview_works_without_index() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "hello world\n").unwrap();

    let output = run_frg(["replace", "hello", "hi", dir.path().to_str().unwrap()]);
    let (stdout, stderr) = output_text(&output);

    assert!(
        output.status.success(),
        "replace should succeed on an unindexed tree\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("a.txt:"), "stdout:\n{}", stdout);
    assert!(stdout.contains("- hello world"), "stdout:\n{}", stdout);
    assert!(stdout.contains("+ hi world"), "stdout:\n{}", stdout);
    assert!(
        stderr.contains("Preview: 1 lines changed in 1 files. Use --write to apply."),
        "stderr:\n{}",
        stderr
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "hello world\n");
    assert!(
        !dir.path().join(".frg").exists(),
        "replace preview should not require an index"
    );
}

#[test]
fn test_replace_write_supports_multiline_patterns_with_index_present() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "foo\nbar\nbaz\n").unwrap();
    fastripgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();

    let output = run_frg([
        "replace",
        "(?s)foo\nbar",
        "qux",
        dir.path().to_str().unwrap(),
        "--write",
    ]);
    let (stdout, stderr) = output_text(&output);

    assert!(
        output.status.success(),
        "multiline replace should succeed when an index is present\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("a.txt: 1 replacements"), "stdout:\n{}", stdout);
    assert!(
        stderr.contains("Wrote 1 replacements in 1 files."),
        "stderr:\n{}",
        stderr
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "qux\nbaz\n");
}

#[test]
fn test_replace_write_supports_literal_mode() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "obj.value = obj.value + 1\n").unwrap();

    let output = run_frg([
        "replace",
        "-F",
        "obj.value",
        "new.value",
        dir.path().to_str().unwrap(),
        "--write",
    ]);
    let (stdout, stderr) = output_text(&output);

    assert!(
        output.status.success(),
        "literal replace should succeed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("a.txt: 2 replacements"), "stdout:\n{}", stdout);
    assert!(
        stderr.contains("Wrote 2 replacements in 1 files."),
        "stderr:\n{}",
        stderr
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "new.value = new.value + 1\n"
    );
}

#[test]
fn test_replace_write_supports_capture_groups() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "fn hello() {}\nfn world() {}\n").unwrap();

    let output = run_frg([
        "replace",
        r"fn (\w+)\(\)",
        "fn renamed_$1()",
        dir.path().to_str().unwrap(),
        "--write",
    ]);
    let (stdout, stderr) = output_text(&output);

    assert!(
        output.status.success(),
        "capture-group replace should succeed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("a.txt: 2 replacements"), "stdout:\n{}", stdout);
    assert!(
        stderr.contains("Wrote 2 replacements in 1 files."),
        "stderr:\n{}",
        stderr
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "fn renamed_hello() {}\nfn renamed_world() {}\n"
    );
}
