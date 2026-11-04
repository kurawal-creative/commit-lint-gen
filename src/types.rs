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

#[derive(Serialize, Deserialize, Clone)]
#[allow(non_snake_case)]
#[serde(default)]
pub struct Rules {
    pub maxLength: usize,
    pub minLength: usize,
    pub types: Vec<String>,
    pub scopes: Vec<String>,
    pub requireScope: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            maxLength: 100,
            minLength: 10,
            types: [
                "feat", "fix", "docs", "style", "refactor", "test", "chore", "ci",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            scopes: Vec::new(),
            requireScope: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[allow(non_snake_case)]
#[serde(default)]
pub struct DiffProcessor {
    pub maxTotalChars: usize,
    pub ignoredExts: Vec<String>,
    pub ignoreLocks: bool,
}

impl Default for DiffProcessor {
    fn default() -> Self {
        Self {
            maxTotalChars: 8000,
            ignoredExts: [
                ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".pdf", ".zip", ".tar", ".gz",
                ".wasm", ".mp4",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            ignoreLocks: true,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
#[serde(default)]
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
    pub rules: Rules,
    pub diffProcessor: DiffProcessor,
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
            rules: Rules::default(),
            diffProcessor: DiffProcessor::default(),
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
        let cfg = std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str::<Self>(&s).ok())
            .unwrap_or_default();
        // ponytail: env key menang atas file biar aman untuk CI
        if cfg.apiKey.trim().is_empty()
            && let Ok(key) = std::env::var("GROQ_API_KEY").or_else(|_| std::env::var("apiKey"))
            && !key.trim().is_empty()
        {
            return Self { apiKey: key, ..cfg };
        }
        cfg
    }

    pub fn save(&self) -> Result<(), String> {
        std::fs::write(Self::path(), serde_json::to_string_pretty(self).unwrap())
            .map_err(|e| e.to_string())
    }

    pub fn is_indo(&self) -> bool {
        self.language.to_lowercase().starts_with("id")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_fill_missing_fields() {
        let cfg: Config = serde_json::from_str(r#"{"apiKey":"x"}"#).unwrap();
        assert_eq!(cfg.apiKey, "x");
        assert_eq!(cfg.baseURL, "https://api.groq.com/openai/v1");
        assert_eq!(cfg.language, "en");
        assert!(cfg.rules.types.contains(&"feat".to_string()));
        assert_eq!(cfg.rules.maxLength, 100);
        assert!(cfg.diffProcessor.ignoreLocks);
    }

    #[test]
    fn custom_rules_and_diff_processor_roundtrip() {
        let json = r#"{
            "apiKey": "k",
            "rules": { "types": ["feat"], "requireScope": true, "maxLength": 72 },
            "diffProcessor": { "maxTotalChars": 1000, "ignoredExts": [".lock"], "ignoreLocks": false }
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.rules.types, vec!["feat"]);
        assert!(cfg.rules.requireScope);
        assert_eq!(cfg.rules.maxLength, 72);
        assert_eq!(cfg.rules.minLength, 10);
        assert_eq!(cfg.diffProcessor.maxTotalChars, 1000);
        assert!(!cfg.diffProcessor.ignoreLocks);
    }

    #[test]
    fn language_detection() {
        let cfg = Config {
            language: "id".into(),
            ..Default::default()
        };
        assert!(cfg.is_indo());
        let cfg = Config {
            language: "en".into(),
            ..Default::default()
        };
        assert!(!cfg.is_indo());
    }
}
