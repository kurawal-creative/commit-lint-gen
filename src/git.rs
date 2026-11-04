use std::process::Command;

use crate::types::{Config, DiffProcessor};

pub fn staged_diff(config: &Config) -> Result<String, String> {
    let diff = run_git(&["diff", "--staged"])?;
    if diff.trim().is_empty() {
        return Err("Tidak ada staged changes (jalankan 'git add').".into());
    }

    // ponytail: peta file dulu (murah) biar model lihat semua scope, bukan cuma head diff
    let stat = run_git(&["diff", "--staged", "--name-status"]).unwrap_or_default();
    let combined = format!(
        "Changed files:\n{}\n\n{}",
        stat.trim(),
        filter_diff(&diff, &config.diffProcessor)
    );

    Ok(combined)
}

/// Raw `--numstat` output, dipakai generator heuristic.
pub fn staged_numstat() -> String {
    run_git(&["diff", "--cached", "--numstat"]).unwrap_or_default()
}

/// Ringkas log commit: `hash\x1fsubject\x1fauthor\x1fdate\x1e`.
pub fn log(limit: usize) -> Result<String, String> {
    let max = format!("--max-count={}", limit);
    run_git(&[
        "log",
        &max,
        "--date=short",
        "--pretty=format:%h%x1f%s%x1f%an%x1f%ad%x1e",
    ])
}

fn run_git(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Bukan repositori git.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn should_skip(path: &str, config: &DiffProcessor) -> bool {
    let lower = path.to_lowercase();
    if config.ignoreLocks
        && [
            "package-lock.json",
            "pnpm-lock.yaml",
            "yarn.lock",
            "cargo.lock",
            "go.sum",
        ]
        .iter()
        .any(|lock| lower.ends_with(lock))
    {
        return true;
    }
    config
        .ignoredExts
        .iter()
        .any(|ext| lower.ends_with(&ext.to_lowercase()))
}

// ponytail: muat konteks kecil — header 1 baris + sinyal +/- saja, konteks dibuang total
fn filter_diff(diff: &str, config: &DiffProcessor) -> String {
    const MAX_FILE_CHARS: usize = 4000;
    let max_total = if config.maxTotalChars == 0 {
        8000
    } else {
        config.maxTotalChars
    };

    let mut out = String::new();
    for chunk in diff.split("diff --git ").skip(1) {
        let header = chunk.lines().next().unwrap_or("");
        let path = header
            .split_whitespace()
            .last()
            .unwrap_or("")
            .trim_start_matches("b/");
        if should_skip(path, config) {
            continue;
        }
        let mut piece = format!("file: {}\n", path);
        for line in chunk.lines().skip(1) {
            if line.starts_with("index ")
                || line.starts_with("new file mode")
                || line.starts_with("deleted file mode")
                || line.starts_with("similarity ")
                || line.starts_with("rename ")
            {
                continue;
            }
            // hanya sinyal: tambah/hapus/hunk, konteks (spasi) dibuang
            if line.starts_with('+') || line.starts_with('-') || line.starts_with("@@") {
                if line.len() > 200 {
                    piece.push_str(&line[..200]);
                    piece.push_str("…\n");
                } else {
                    piece.push_str(line);
                    piece.push('\n');
                }
            }
        }
        if piece.len() > MAX_FILE_CHARS {
            piece.truncate(MAX_FILE_CHARS);
            piece.push_str("\n[...dipotong demi limit API...]\n");
        }
        if out.len() + piece.len() > max_total {
            out.push_str("[...sisa diff dipotong demi limit API...]");
            break;
        }
        out.push_str(&piece);
    }
    if out.trim().is_empty() {
        diff.chars().take(max_total).collect()
    } else {
        out
    }
}

pub fn commit(msg: &str) -> Result<(), String> {
    let status = Command::new("git")
        .args(["commit", "-m", msg])
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err("Gagal melakukan git commit.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(files: &[(&str, &str)]) -> String {
        files
            .iter()
            .map(|(path, body)| {
                format!(
                    "diff --git a/{path} b/{path}\nindex 000..111 100644\n--- a/{path}\n+++ b/{path}\n{body}"
                )
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn keeps_only_signal_lines() {
        let raw = diff(&[("src/a.rs", "@@ -1 +1 @@\n-old\n+new\n context\n")]);
        let out = filter_diff(&raw, &DiffProcessor::default());
        assert!(out.contains("file: src/a.rs"));
        assert!(out.contains("+new"));
        assert!(out.contains("-old"));
        assert!(!out.contains("index "));
        assert!(!out.contains("..a/src/a.rs"));
        assert!(!out.contains(" context"));
    }

    #[test]
    fn skips_lockfiles_and_ignored_extensions() {
        let raw = diff(&[
            ("Cargo.lock", "+lock content\n"),
            ("assets/logo.png", "+binary\n"),
            ("src/main.rs", "+real\n"),
        ]);
        let out = filter_diff(&raw, &DiffProcessor::default());
        assert!(!out.contains("Cargo.lock"));
        assert!(!out.contains("logo.png"));
        assert!(out.contains("src/main.rs"));
    }

    #[test]
    fn respects_max_total_chars() {
        let config = DiffProcessor {
            maxTotalChars: 20,
            ..Default::default()
        };
        let raw = diff(&[("src/a.rs", &"+some long added line\n".repeat(5))]);
        let out = filter_diff(&raw, &config);
        assert!(out.contains("dipotong"));
    }

    #[test]
    fn lock_filter_can_be_disabled() {
        let config = DiffProcessor {
            ignoreLocks: false,
            ..Default::default()
        };
        let raw = diff(&[("Cargo.lock", "+lock content\n")]);
        assert!(filter_diff(&raw, &config).contains("Cargo.lock"));
    }
}
