use std::cmp::Ordering;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::Config;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO: &str = "kurawal-creative/commit-lint-gen";
const PACKAGE: &str = "commitlg";
const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;
const BETA_MARKER: &str = "-beta.";

pub fn version_line() -> String {
    format!("clg {} (github.com/{})", VERSION, REPO)
}

/// Channel ditentukan dari versi yang ter-embed: `x.y.z-beta.n` = beta, selain itu latest.
/// Tidak perlu tanya registry, jadi beta tidak akan pernah "pindah" ke stable.
pub fn channel_for(version: &str) -> &'static str {
    if version.contains(BETA_MARKER) {
        "beta"
    } else {
        "latest"
    }
}

pub fn channel() -> &'static str {
    channel_for(VERSION)
}

fn parse_version(input: &str) -> Option<(u64, u64, u64, Vec<&str>)> {
    let clean = input.trim().trim_start_matches('v');
    let (core, pre) = match clean.split_once('-') {
        Some((core, pre)) => (core, pre.split('+').next()?),
        None => (clean.split('+').next()?, ""),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    let pre = if pre.is_empty() {
        Vec::new()
    } else {
        pre.split('.').collect()
    };
    Some((major, minor, patch, pre))
}

fn compare_pre(a: &[&str], b: &[&str]) -> Ordering {
    // Tanpa prerelease = lebih baru dari prerelease apa pun.
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    for (left, right) in a.iter().zip(b.iter()) {
        let ordering = match (left.parse::<u64>(), right.parse::<u64>()) {
            (Ok(l), Ok(r)) => l.cmp(&r),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => left.cmp(right),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}

/// `true` kalau `latest` lebih baru dari `current` (semver + prerelease).
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (Some(latest), Some(current)) = (parse_version(latest), parse_version(current)) else {
        return false;
    };
    match latest.0.cmp(&current.0) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => match latest.1.cmp(&current.1) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => match latest.2.cmp(&current.2) {
                Ordering::Greater => true,
                Ordering::Less => false,
                Ordering::Equal => compare_pre(&latest.3, &current.3) == Ordering::Greater,
            },
        },
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Cek maksimal sekali per 24 jam.
pub fn should_check(now: u64, last: u64) -> bool {
    last == 0 || now.saturating_sub(last) >= CHECK_INTERVAL_SECS
}

fn marker_path() -> PathBuf {
    PathBuf::from(Config::path()).with_file_name(".commitlintgenrc.lastcheck")
}

fn installed_via_npm() -> bool {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().contains("node_modules"))
        .unwrap_or(false)
}

fn npm() -> Command {
    if cfg!(windows) {
        Command::new("npm.cmd")
    } else {
        Command::new("npm")
    }
}

pub fn latest_version() -> Result<String, String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let url = format!("https://registry.npmjs.org/{}/{}", PACKAGE, channel());
        let response = reqwest::Client::new()
            .get(url)
            .header("User-Agent", format!("clg/{}", VERSION))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("npm registry {}", response.status()));
        }
        let data: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        data.get("version")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .ok_or_else(|| "versi tidak ditemukan di response".into())
    })
}

fn install_latest() -> Result<(), String> {
    let status = npm()
        .args(["install", "-g", &format!("{}@{}", PACKAGE, channel())])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("npm install gagal".into())
    }
}

/// Jalankan update setelah proses clg yang sedang berjalan keluar.
/// Diperlukan terutama di Windows: file .exe yang sedang dieksekusi terkunci.
fn spawn_deferred_install() -> Result<(), String> {
    let pid = std::process::id();
    let package = format!("{}@{}", PACKAGE, channel());

    #[cfg(unix)]
    let child = {
        let script = format!(
            "while kill -0 {} 2>/dev/null; do sleep 1; done; npm install -g {} >/dev/null 2>&1",
            pid, package
        );
        Command::new("sh")
            .args(["-c", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };

    #[cfg(windows)]
    let child = {
        let script = format!(
            "while (Get-Process -Id {} -ErrorAction SilentlyContinue) {{ Start-Sleep -Seconds 1 }}; npm.cmd install -g {}",
            pid, package
        );
        Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };

    child
        .map(|_| ())
        .map_err(|e| format!("gagal menjadwalkan update: {}", e))
}

/// Cek + update diam-diam setiap 24 jam, hanya untuk instalasi lewat npm.
pub fn auto_update() {
    if std::env::var("CLG_NO_UPDATE").is_ok() || !installed_via_npm() {
        return;
    }
    std::thread::spawn(|| {
        let marker = marker_path();
        let now = now_secs();
        let last = std::fs::read_to_string(&marker)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        if !should_check(now, last) {
            return;
        }
        // tulis marker duluan biar offline tidak retry tiap launch
        let _ = std::fs::write(&marker, now.to_string());
        if let Ok(latest) = latest_version()
            && is_newer(&latest, VERSION)
        {
            let _ = spawn_deferred_install();
        }
    });
}

pub fn run(check_only: bool) -> Result<(), String> {
    println!("{} [{}]", version_line(), channel());
    let latest = latest_version().map_err(|e| format!("Tidak bisa cek versi baru: {}", e))?;
    if !is_newer(&latest, VERSION) {
        println!(
            "✓ Sudah versi terbaru di channel {} (v{})",
            channel(),
            VERSION
        );
        return Ok(());
    }
    if check_only {
        println!(
            "Update tersedia di channel {}: v{} → v{}",
            channel(),
            VERSION,
            latest
        );
        println!("Jalankan `clg update` untuk memasangnya.");
        return Ok(());
    }
    println!(
        "Memperbarui channel {}: v{} → v{} ...",
        channel(),
        VERSION,
        latest
    );
    if cfg!(windows) {
        spawn_deferred_install()?;
        println!("✓ Update dijadwalkan; dipasang setelah clg ditutup.");
    } else {
        install_latest()?;
        println!("✓ Update selesai. Jalankan ulang `clg`.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_stable_versions() {
        assert!(is_newer("2.0.2", "2.0.1"));
        assert!(is_newer("v2.1.0", "2.0.9"));
        assert!(is_newer("3.0.0", "2.99.99"));
        assert!(!is_newer("2.0.1", "2.0.1"));
        assert!(!is_newer("2.0.0", "2.0.1"));
        assert!(!is_newer("garbage", "2.0.1"));
    }

    #[test]
    fn compares_prereleases() {
        assert!(is_newer("2.0.0-beta.10", "2.0.0-beta.9"));
        assert!(is_newer("2.0.0-beta.2", "2.0.0-beta.1"));
        assert!(is_newer("2.0.0", "2.0.0-beta.9"));
        assert!(!is_newer("2.0.0-beta.9", "2.0.0"));
        assert!(!is_newer("2.0.0-beta.1", "2.0.0-beta.1"));
        assert!(is_newer("2.0.1-beta.1", "2.0.0"));
    }

    #[test]
    fn parses_partial_and_build_versions() {
        assert_eq!(parse_version("2"), Some((2, 0, 0, vec![])));
        assert_eq!(parse_version(" 1.2.3+build "), Some((1, 2, 3, vec![])));
        assert_eq!(
            parse_version("1.2.3-beta.4"),
            Some((1, 2, 3, vec!["beta", "4"]))
        );
    }

    #[test]
    fn channel_follows_embedded_version() {
        assert_eq!(channel_for("2.0.1"), "latest");
        assert_eq!(channel_for("2.0.1-beta.1"), "beta");
        assert_eq!(channel(), channel_for(VERSION));
    }

    #[test]
    fn throttle_is_daily() {
        let now = 1_000_000;
        assert!(should_check(now, 0));
        assert!(!should_check(now, now - 60));
        assert!(!should_check(now, now - CHECK_INTERVAL_SECS + 1));
        assert!(should_check(now, now - CHECK_INTERVAL_SECS));
    }

    #[test]
    fn version_line_embeds_repo_and_version() {
        let line = version_line();
        assert!(line.contains(VERSION));
        assert!(line.contains(REPO));
    }
}
