use std::process::Command;

use crate::types::{Config, DiffProcessor};

pub fn staged_diff(config: &Config) -> Result<String, String> {
    let diff = run_git(&["diff", "--staged"])?;
    if diff.trim().is_empty() {
        return Err("Tidak ada staged changes (jalankan 'git add').".into());
    }

    // ponytail: peta file dulu (murah) biar model lihat semua scope, bukan cuma head diff
    let stat = run_git(&["diff", "--staged", "--name-status"]).unwrap_or_default();
    let (stat, build_skipped) = without_build_assets(&stat);
    let note = if build_skipped > 0 {
        format!(
            "\n\n[{} file build output (Vite/dist) diabaikan: asset hasil build, bukan kode sumber.]",
            build_skipped
        )
    } else {
        String::new()
    };
    let combined = format!(
        "Changed files:\n{}\n\n{}{}",
        stat.trim(),
        filter_diff(&diff, &config.diffProcessor),
        note
    );

    Ok(combined)
}

/// Raw `--numstat` output, dipakai generator heuristic.
pub fn staged_numstat() -> String {
    let numstat = run_git(&["diff", "--cached", "--numstat"]).unwrap_or_default();
    without_build_assets(&numstat).0
}

fn without_build_assets(text: &str) -> (String, usize) {
    let mut kept = Vec::new();
    let mut skipped = 0;
    for line in text.lines() {
        let path = line.split('\t').last().unwrap_or("");
        if is_build_asset(path) {
            skipped += 1;
        } else {
            kept.push(line);
        }
    }
    (kept.join("\n"), skipped)
}

/// Output build Vite/Next dsb. Bukan kode sumber: jangan dimasukkan ke konteks AI.
fn is_build_asset(path: &str) -> bool {
    const BUILD_DIRS: &[&str] = &[
        "dist",
        ".next",
        ".nuxt",
        ".output",
        ".svelte-kit",
        ".vite",
        "coverage",
        "node_modules",
    ];
    let lower = path.to_lowercase();
    let parts: Vec<&str> = lower.split('/').filter(|p| !p.is_empty()).collect();
    let dirs = &parts[..parts.len().saturating_sub(1)];
    if dirs.iter().any(|dir| BUILD_DIRS.contains(dir)) {
        return true;
    }
    // `build/` dan `out/` hanya di root atau langsung di bawah public/static.
    dirs.iter().enumerate().any(|(i, dir)| {
        (*dir == "build" || *dir == "out")
            && (i == 0 || (i == 1 && (dirs[0] == "public" || dirs[0] == "static")))
    })
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
    if is_build_asset(path) {
        return true;
    }
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
    let mut skipped_build = false;
    for chunk in diff.split("diff --git ").skip(1) {
        let header = chunk.lines().next().unwrap_or("");
        let path = header
            .split_whitespace()
            .last()
            .unwrap_or("")
            .trim_start_matches("b/");
        if is_build_asset(path) {
            skipped_build = true;
        }
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
        // Jangan fallback ke raw diff kalau isinya cuma build output.
        if skipped_build {
            String::new()
        } else {
            diff.chars().take(max_total).collect()
        }
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
    fn skips_vite_build_output() {
        let raw = diff(&[
            ("dist/assets/index-a1b2c3.js", "+bundle\n"),
            ("packages/app/dist/main.js", "+bundle\n"),
            ("public/build/app.js", "+bundle\n"),
            ("src/main.ts", "+real\n"),
        ]);
        let out = filter_diff(&raw, &DiffProcessor::default());
        assert!(!out.contains("dist/"));
        assert!(!out.contains("public/build"));
        assert!(out.contains("src/main.ts"));
    }

    #[test]
    fn build_output_only_produces_empty_diff() {
        let raw = diff(&[("dist/assets/index-a1b2c3.js", "+bundle\n")]);
        assert!(filter_diff(&raw, &DiffProcessor::default()).is_empty());
    }

    #[test]
    fn build_asset_detection_avoids_source_dirs() {
        assert!(is_build_asset("dist/assets/index.js"));
        assert!(is_build_asset("packages/app/dist/main.js"));
        assert!(is_build_asset("public/build/app.js"));
        assert!(is_build_asset("node_modules/pkg/index.js"));
        assert!(!is_build_asset("build.rs"));
        assert!(!is_build_asset("src/build/mod.rs"));
        assert!(!is_build_asset("src/out/mod.rs"));
    }

    #[test]
    fn count_and_strip_build_assets_from_stat() {
        let stat = "M\tdist/app.js\nA\tsrc/app.ts\nM\tpublic/build/x.js";
        let (kept, skipped) = without_build_assets(stat);
        assert_eq!(skipped, 2);
        assert!(kept.contains("src/app.ts"));
        assert!(!kept.contains("dist"));
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
