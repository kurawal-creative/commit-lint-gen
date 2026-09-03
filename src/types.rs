use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Clone)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_format: Option<String>,
}

#[derive(Deserialize)]
pub struct ChatMessageResponse {
    pub content: String,
}

#[derive(Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessageResponse,
}

#[derive(Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[allow(non_snake_case)]
pub struct Config {
    #[serde(default = "default_provider")]
    pub aiProvider: String,
    #[serde(default)]
    pub apiKey: String,
    #[serde(default = "default_base_url")]
    pub baseURL: String,
    #[serde(default = "default_lang")]
    pub language: String,
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_provider() -> String {
    "groq".into()
}
fn default_base_url() -> String {
    "https://api.groq.com/openai/v1".into()
}
fn default_lang() -> String {
    "en".into()
}
fn default_model() -> String {
    "qwen/qwen3.8-27b".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            aiProvider: default_provider(),
            apiKey: String::new(),
            baseURL: default_base_url(),
            language: default_lang(),
            model: default_model(),
        }
    }
}

impl Config {
    // ponytail: home lintas OS (HOME unix, USERPROFILE windows)
    pub fn path() -> String {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(|h| format!("{}/.commitlintgenrc.json", h.trim_end_matches(['/', '\\'])))
            .unwrap_or_else(|_| ".commitlintgenrc.json".into())
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        std::fs::write(Self::path(), serde_json::to_string_pretty(self).unwrap())
            .map_err(|e| e.to_string())
    }
}
