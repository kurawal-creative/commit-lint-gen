use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "clg-it-{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_clg"))
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("GROQ_API_KEY")
        .env_remove("apiKey")
        .output()
        .expect("failed to run clg binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn version_and_help_document_commands() {
    let dir = temp_dir("version");
    let out = run(&dir, &["--version"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains(env!("CARGO_PKG_VERSION")));

    let out = run(&dir, &["--help"]);
    assert!(out.status.success());
    let help = stdout(&out);
    for command in [
        "generate",
        "lint",
        "init",
        "uninstall",
        "audit",
        "config",
        "doctor",
        "update",
    ] {
        assert!(help.contains(command), "help missing {command}");
    }

    let out = run(&dir, &["generate", "--help"]);
    assert!(stdout(&out).contains("--yes"));
    assert!(stdout(&out).contains("--heuristic"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn lint_reports_valid_and_invalid() {
    let dir = temp_dir("lint");
    let out = run(&dir, &["lint", "feat: add user login"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Valid commit message"));

    let out = run(&dir, &["lint", "wip"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("[format]"));

    let out = run(&dir, &["lint", "feat: Add endpoint."]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("description-case"));
    assert!(stderr(&out).contains("description-period"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn init_and_uninstall_manage_the_hook() {
    let dir = temp_dir("hook");
    std::fs::create_dir_all(dir.join(".git")).unwrap();

    let out = run(&dir, &["init"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let hook = dir.join(".git/hooks/commit-msg");
    assert!(hook.exists());
    let content = std::fs::read_to_string(&hook).unwrap();
    assert!(content.contains("commit-lint-gen hook"));
    assert!(content.contains("clg lint"));

    let out = run(&dir, &["uninstall"]);
    assert!(out.status.success());
    assert!(!hook.exists());

    let out = run(&dir, &["uninstall"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("No git hook found"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn init_fails_outside_git_repository() {
    let dir = temp_dir("notgit");
    let out = run(&dir, &["init"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("Not a git repository"));
    std::fs::remove_dir_all(dir).ok();
}
