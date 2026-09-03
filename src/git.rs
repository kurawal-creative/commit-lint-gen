use std::process::Command;

pub fn staged_diff() -> Result<String, String> {
    let output = Command::new("git")
        .args(["diff", "--staged"])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err("Bukan repositori git.".into());
    }

    let diff = String::from_utf8_lossy(&output.stdout).to_string();
    if diff.trim().is_empty() {
        return Err("Tidak ada staged changes (jalankan 'git add').".into());
    }

    // ponytail: peta file dulu (murah) biar model lihat semua scope, bukan cuma head diff
    let stat = Command::new("git")
        .args(["diff", "--staged", "--name-status"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let combined = format!("Changed files:\n{}\n\n{}", stat.trim(), filter_diff(&diff));

    Ok(combined)
}

// ponytail: muat konteks kecil — header 1 baris + sinyal +/- saja, konteks dibuang total
fn filter_diff(diff: &str) -> String {
    const MAX_FILE_CHARS: usize = 4000;
    const MAX_TOTAL_CHARS: usize = 12000;

    let mut out = String::new();
    for chunk in diff.split("diff --git ").skip(1) {
        let header = chunk.lines().next().unwrap_or("");
        let path = header.split_whitespace().last().unwrap_or("").trim_start_matches("b/");
        let mut piece = format!("file: {}\n", path);
        for line in chunk.lines().skip(1) {
            if line.starts_with("index ") || line.starts_with("new file mode") || line.starts_with("deleted file mode") || line.starts_with("similarity ") || line.starts_with("rename ") {
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
        if out.len() + piece.len() > MAX_TOTAL_CHARS {
            out.push_str("[...sisa diff dipotong demi limit API...]");
            break;
        }
        out.push_str(&piece);
    }
    if out.trim().is_empty() {
        diff.chars().take(MAX_TOTAL_CHARS).collect()
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
