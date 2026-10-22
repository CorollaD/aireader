//! AI service settings (stored in `library.json`) and API keys (stored in the system keychain).

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Keychain service name for API keys.
const KEYRING_SERVICE: &str = "com.aireader.reader";

/// Provider id meaning "any OpenAI-compatible endpoint at `base_url`".
pub const CUSTOM: &str = "custom";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiConfig {
    /// A rig vendor id (`anthropic`, `openai`, `deepseek`, `ollama`, …) or [`CUSTOM`].
    pub provider: String,
    pub model: String,
    /// Overrides the provider's endpoint; required for [`CUSTOM`].
    pub base_url: Option<String>,
    /// Language for answers, e.g. "中文".
    pub language: String,
    /// The user accepted that book text is sent to a cloud service.
    pub consented: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            model: String::new(),
            base_url: None,
            language: "中文".into(),
            consented: false,
        }
    }
}

impl AiConfig {
    pub fn is_configured(&self) -> bool {
        !self.provider.trim().is_empty() && !self.model.trim().is_empty()
    }

    /// The rig vendor the requests go through.
    pub fn vendor(&self) -> &str {
        if self.provider == CUSTOM {
            "openai"
        } else {
            &self.provider
        }
    }

    /// Whether requests stay on this machine (no privacy prompt needed).
    pub fn is_local(&self) -> bool {
        if matches!(self.provider.as_str(), "ollama" | "llamacpp") {
            return self.base_url.as_deref().is_none_or(is_local_url);
        }
        self.base_url.as_deref().is_some_and(is_local_url)
    }

    /// Keychain account name for this provider's key.
    pub fn key_name(&self) -> String {
        format!("ai:{}", self.provider)
    }

    /// Identifies the model in caches, so switching models does not reuse summaries.
    pub fn model_key(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }

    /// Normalises user input: trims fields and drops an empty base URL.
    pub fn cleaned(mut self) -> Self {
        self.provider = self.provider.trim().to_owned();
        self.model = self.model.trim().to_owned();
        self.base_url = self
            .base_url
            .map(|u| u.trim().trim_end_matches('/').to_owned())
            .filter(|u| !u.is_empty());
        if self.language.trim().is_empty() {
            self.language = "中文".into();
        }
        self
    }
}

fn is_local_url(url: &str) -> bool {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', ':'])
        .next()
        .unwrap_or("");
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "0.0.0.0")
}

/// Where API keys live. The app uses the system keychain; tests use memory.
pub trait SecretStore: Send + Sync {
    fn get(&self, name: &str) -> Option<String>;
    fn set(&self, name: &str, value: &str) -> Result<()>;
    fn delete(&self, name: &str) -> Result<()>;
}

pub struct KeyringStore;

impl SecretStore for KeyringStore {
    fn get(&self, name: &str) -> Option<String> {
        keyring::Entry::new(KEYRING_SERVICE, name)
            .ok()?
            .get_password()
            .ok()
    }

    fn set(&self, name: &str, value: &str) -> Result<()> {
        keyring::Entry::new(KEYRING_SERVICE, name)?.set_password(value)?;
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<()> {
        match keyring::Entry::new(KEYRING_SERVICE, name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, name: &str) -> Option<String> {
        self.0.lock().unwrap().get(name).cloned()
    }

    fn set(&self, name: &str, value: &str) -> Result<()> {
        self.0.lock().unwrap().insert(name.into(), value.into());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.0.lock().unwrap().remove(name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locality_and_cleaning() {
        let mut c = AiConfig {
            provider: "ollama".into(),
            model: "qwen3".into(),
            ..Default::default()
        };
        assert!(c.is_local());
        c.base_url = Some("http://192.168.1.5:11434".into());
        assert!(!c.is_local(), "a remote Ollama host is not local");
        let c = AiConfig {
            provider: " custom ".into(),
            model: " m ".into(),
            base_url: Some(" http://localhost:1234/v1/ ".into()),
            language: " ".into(),
            consented: false,
        }
        .cleaned();
        assert_eq!(
            (c.provider.as_str(), c.vendor(), c.base_url.as_deref()),
            ("custom", "openai", Some("http://localhost:1234/v1"))
        );
        assert!(c.is_local());
        assert_eq!(c.language, "中文");
        assert!(
            !AiConfig {
                provider: "anthropic".into(),
                model: "x".into(),
                ..Default::default()
            }
            .is_local()
        );
    }

    #[test]
    fn memory_store_round_trip() {
        let s = MemoryStore::default();
        assert_eq!(s.get("k"), None);
        s.set("k", "v").unwrap();
        assert_eq!(s.get("k").as_deref(), Some("v"));
        s.delete("k").unwrap();
        s.delete("k").unwrap();
        assert_eq!(s.get("k"), None);
    }
}
