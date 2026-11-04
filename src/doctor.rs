use std::process::Command;

use crate::types::Config;

fn line(ok: bool, label: &str, detail: &str) {
    let mark = if ok { "✓" } else { "✗" };
    if detail.is_empty() {
        println!("  {} {}", mark, label);
    } else {
        println!("  {} {}  {}", mark, label, detail);
    }
}

fn warn(label: &str, detail: &str) {
    println!("  ! {}  {}", label, detail);
}

/// Sumber API key: env `GROQ_API_KEY`, env `apiKey`, lalu file config.
pub fn key_source<'a>(
    config: &'a Config,
    groq_env: Option<&'a str>,
    key_env: Option<&'a str>,
) -> Option<&'a str> {
    if groq_env.is_some_and(|v| !v.trim().is_empty()) {
        return Some("GROQ_API_KEY");
    }
    if key_env.is_some_and(|v| !v.trim().is_empty()) {
        return Some("apiKey env");
    }
    if !config.apiKey.trim().is_empty() {
        return Some("config file");
    }
    None
}

pub fn run(config: &Config) {
    println!("clg doctor — checking your environment\n");

    println!("Git");
    match Command::new("git").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            line(true, "Git is installed", &version);
        }
        _ => line(false, "Git is not installed or not in PATH", ""),
    }

    println!("\nConfig");
    let global = Config::path();
    let local = std::env::current_dir()
        .map(|dir| dir.join(".commitlintgenrc.json").display().to_string())
        .unwrap_or_default();

    if std::path::Path::new(&local).exists() {
        line(true, "Local config found", &local);
    } else {
        warn(
            "No local config",
            ".commitlintgenrc.json not found in project root",
        );
    }
    if std::path::Path::new(&global).exists() {
        line(true, "Global config found", &global);
    } else {
        warn("No global config", "~/.commitlintgenrc.json not found");
    }

    let groq_env = std::env::var("GROQ_API_KEY").ok();
    let key_env = std::env::var("apiKey").ok();
    match key_source(config, groq_env.as_deref(), key_env.as_deref()) {
        Some(source) => line(true, "API key found", source),
        None => line(
            false,
            "No API key found",
            "Set GROQ_API_KEY or run `clg config`",
        ),
    }

    println!("  provider : {}", config.aiProvider);
    println!("  baseURL  : {}", config.baseURL);
    println!("  model    : {}", config.model);

    println!("\nAPI Connection");
    if config.apiKey.trim().is_empty() && groq_env.is_none() && key_env.is_none() {
        line(false, "Skipping API test — no API key configured", "");
        println!("\nRun `clg config` to set up your API key.");
        return;
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            line(false, "Cannot start network runtime", &e.to_string());
            return;
        }
    };
    let start = std::time::Instant::now();
    let result = runtime.block_on(async {
        let client = reqwest::Client::new();
        let url = format!("{}/chat/completions", config.baseURL.trim_end_matches('/'));
        let payload = serde_json::json!({
            "model": config.model,
            "messages": [{ "role": "user", "content": "Reply with the single word: ok" }],
            "max_tokens": 10,
        });
        client
            .post(url)
            .bearer_auth(config.apiKey.trim())
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())
    });

    match result {
        Ok(response) if response.status().is_success() => {
            line(
                true,
                &format!("API responded in {}ms", start.elapsed().as_millis()),
                &format!("model={}", config.model),
            );
        }
        Ok(response) => line(
            false,
            "API connection failed",
            &response.status().to_string(),
        ),
        Err(error) => line(false, "API connection failed", &error),
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_source_priority() {
        let mut cfg = Config {
            apiKey: "from-file".into(),
            ..Default::default()
        };
        assert_eq!(
            key_source(&cfg, Some("from-env"), Some("x")),
            Some("GROQ_API_KEY")
        );
        assert_eq!(key_source(&cfg, Some("  "), Some("x")), Some("apiKey env"));
        assert_eq!(key_source(&cfg, None, None), Some("config file"));

        cfg.apiKey = String::new();
        assert_eq!(key_source(&cfg, None, Some(" ")), None);
    }
}
