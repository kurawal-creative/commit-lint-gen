use reqwest::Client;

use crate::types::{ChatCompletionRequest, ChatCompletionResponse};

pub async fn generate_commit(
    diff: &str,
    api_key: &str,
    base_url: &str,
    model: &str,
    is_indo: bool,
) -> Result<String, String> {
    let lang = if is_indo { "Indonesian" } else { "English" };

    let sys_prompt = format!(
        "Write a Conventional Commits message, terse and exact. Language: {lang}.\n\
         Subject: <type>(<scope>): <imperative summary>, scope optional. Types: feat fix refactor perf docs test chore build ci style revert. Imperative: add fix remove, never added/adds/adding. Target <=50 chars, hard cap 72. No trailing period. Lowercase after colon unless project uses caps.\n\
         Body only when: non-obvious why, breaking changes, migration notes, linked issues; else skip entirely. If 2+ files changed, always add 1-3 bullet body (-, wrap 72) summarizing each file's change. Always include body for: breaking changes, security fixes, data migrations, reverts — never compress these to subject-only. Issues at end: Closes #N, Refs #N.\n\
         Never: This commit does X, I/we/now/currently, As requested by (use Co-authored-by trailer instead), AI attribution (unless Assisted-by trailer required), emoji, filename restated when scope says it.\n\
         Example good: feat(api): add GET /users/:id/profile + body why + Closes #128. Example bad: feat: add a new endpoint to get user profile information from the database. Breaking example: feat(api)!: rename /v1/orders to /v1/checkout + BREAKING CHANGE body.\n\
         Output ONLY the raw commit message, no codeblocks, no reasoning.",
        lang = lang
    );

    let payload = ChatCompletionRequest {
        model: model.to_string(),
        messages: vec![
            crate::types::ChatMessage {
                role: "system".into(),
                content: sys_prompt,
            },
            crate::types::ChatMessage {
                role: "user".into(),
                content: format!("Diff:\n{}", diff),
            },
        ],
        temperature: 0.2,
        max_tokens: 300,
        reasoning_effort: Some("none".into()),
        reasoning_format: Some("hidden".into()),
    };

    let client = Client::new();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    // ponytail: fallback ke 3.6 kalau model utama error
    let mut models = vec![model.to_string()];
    if model != "qwen/qwen3.6-27b" {
        models.push("qwen/qwen3.6-27b".to_string());
    }
    let mut last_err = String::new();
    for m in &models {
        let mut payload = payload.clone();
        payload.model = m.clone();
        // ponytail: retry 429 mengikuti "try again in Ns" dari Groq (max 3x)
        let mut attempt = 0;
        let res = loop {
            let res = client
                .post(&url)
                .bearer_auth(api_key)
                .json(&payload)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if res.status().as_u16() != 429 || attempt >= 3 {
                break res;
            }
            let body = res.text().await.unwrap_or_default();
            let wait = parse_retry_after(&body).unwrap_or(20);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            attempt += 1;
        };

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            last_err = format!("Groq API {} ({}): {}", status, m, body.chars().take(200).collect::<String>());
            continue;
        }

        match res.json::<ChatCompletionResponse>().await.map_err(|e| e.to_string()) {
            Err(e) => last_err = e,
            Ok(data) => {
                let raw = data.choices.first().map(|c| c.message.content.as_str()).unwrap_or("");
                let cleaned = clean(raw);
                if cleaned.is_empty() {
                    last_err = "Respon kosong.".into();
                } else {
                    return Ok(cleaned);
                }
            }
        }
    }
    Err(last_err)
}

fn parse_retry_after(body: &str) -> Option<u64> {
    body.find("try again in ").and_then(|i| {
        body[i + 13..]
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .next()?
            .parse::<f64>()
            .ok()
            .map(|f| f.ceil() as u64 + 1)
    })
}

fn clean(input: &str) -> String {
    // Strip everything up to the LAST </think> tag (takes content after it)
    let stripped = if let Some(pos) = input.rfind("</think>") {
        input[pos + 8..].trim().to_string()
    } else if let Some(pos) = input.rfind("<think>") {
        input[pos + 8..].trim().to_string()
    } else {
        input.trim().to_string()
    };
    // ponytail: tiap baris >72 cell dipotong + "..."
    use unicode_width::UnicodeWidthStr;
    stripped
        .lines()
        .map(|l| {
            if l.width() <= 72 {
                l.to_string()
            } else {
                let mut out = String::new();
                let mut w = 0;
                for c in l.chars() {
                    let chw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                    if w + chw > 69 {
                        break;
                    }
                    out.push(c);
                    w += chw;
                }
                out.push_str("...");
                out
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
