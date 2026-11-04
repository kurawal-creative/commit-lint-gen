use reqwest::Client;

use crate::types::{ChatCompletionRequest, ChatCompletionResponse};

pub async fn generate_commit(
    diff: &str,
    api_key: &str,
    base_url: &str,
    model: &str,
    is_indo: bool,
    previous: Option<&str>,
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

    let user = build_user_content(diff, previous);
    let payload = ChatCompletionRequest {
        model: model.to_string(),
        messages: vec![
            crate::types::ChatMessage {
                role: "system".into(),
                content: sys_prompt,
            },
            crate::types::ChatMessage {
                role: "user".into(),
                content: user,
            },
        ],
        temperature: 0.2,
        max_tokens: 300,
        reasoning_effort: Some("none".into()),
        reasoning_format: Some("hidden".into()),
    };

    let client = Client::new();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
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
        return Err(format!(
            "Groq API {}: {}",
            status,
            body.chars().take(200).collect::<String>()
        ));
    }

    let data = res
        .json::<ChatCompletionResponse>()
        .await
        .map_err(|e| e.to_string())?;
    let raw = data
        .choices
        .first()
        .map(|c| c.message.content.as_str())
        .unwrap_or("");
    let cleaned = clean(raw);
    if cleaned.is_empty() {
        return Err("Respon kosong.".into());
    }
    Ok(cleaned)
}

fn build_user_content(diff: &str, previous: Option<&str>) -> String {
    let mut user = format!("Diff:\n{}", diff);
    if let Some(previous) = previous
        && !previous.trim().is_empty()
    {
        user.push_str(&format!(
            "\n\nPrevious draft (improve it, keep the intent):\n{}",
            previous
        ));
    }
    user
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
    // tanpa truncate: serahkan wrap ke display (TUI/leftover), isi tetap utuh
    stripped.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_thinking_tags() {
        assert_eq!(clean("</think>feat: add x"), "feat: add x");
        assert_eq!(clean("reasoning...</think>\nfeat: add x\n"), "feat: add x");
        assert_eq!(clean("  feat: add x  "), "feat: add x");
    }

    #[test]
    fn parses_retry_after_seconds() {
        assert_eq!(
            parse_retry_after("Please try again in 20.5s. Visit ..."),
            Some(22)
        );
        assert_eq!(parse_retry_after("try again in 3s"), Some(4));
        assert_eq!(parse_retry_after("no hint"), None);
    }

    #[test]
    fn user_content_includes_previous_draft_only_when_present() {
        let plain = build_user_content("+new line", None);
        assert_eq!(plain, "Diff:\n+new line");
        assert!(!plain.contains("Previous draft"));

        let with_prev = build_user_content("+new line", Some("feat: add x"));
        assert!(with_prev.contains("Previous draft"));
        assert!(with_prev.contains("feat: add x"));

        let blank = build_user_content("+new line", Some("   "));
        assert!(!blank.contains("Previous draft"));
    }
}
