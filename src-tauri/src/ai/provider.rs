//! Connecting to the configured model through rig, and the small helpers every AI task uses:
//! token estimates, request budgets, streaming text, and readable errors.

use anyhow::{Result, bail};
use futures_util::StreamExt;
use rig_core::DynModel;
use rig_core::catalog::Catalog;
use rig_core::completion::CompletionRequest;
use rig_core::error::ProviderError;
use rig_core::operation::Completion;
use rig_core::providers::registry::ConnectOptions;
use rig_core::streaming::{Item, StreamEvent};

use super::config::{AiConfig, CUSTOM};

pub type ChatModel = DynModel<Completion>;

/// Builds the configured chat model. Nothing is sent until a request is made.
pub fn connect(cfg: &AiConfig, key: Option<String>) -> Result<ChatModel> {
    if !cfg.is_configured() {
        bail!(crate::t!("ai_not_configured"));
    }
    if cfg.provider == CUSTOM && cfg.base_url.is_none() {
        bail!(crate::t!("ai_custom_needs_url"));
    }
    let key = key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
    if cfg.provider == CUSTOM {
        // OpenAI-compatible servers speak Chat Completions (not OpenAI's Responses API).
        // Servers without authentication accept any key; rig needs one to build the request.
        use rig_core::providers::openai::OpenAIConfig;
        let config = OpenAIConfig::new(key.unwrap_or_else(|| "not-needed".into()))
            .with_base_url(cfg.base_url.clone().unwrap_or_default());
        let client = if cfg.is_local() {
            config.connect(direct_http())
        } else {
            config.client()
        };
        return Ok(client.chat(cfg.model.clone()).erase());
    }
    let mut options = ConnectOptions::new();
    if let Some(k) = key {
        options = options.api_key(k);
    }
    if let Some(url) = &cfg.base_url {
        options = options.base_url(url.clone());
    }
    if cfg.is_local() {
        options = options.http(direct_http());
    }
    let reference = format!("{}/{}", cfg.vendor(), cfg.model);
    Catalog::builtin()
        .connect_with(reference.as_str(), options)
        .map_err(|e| {
            let text = e.to_string();
            if text.to_lowercase().contains("key") {
                anyhow::anyhow!(crate::t!("ai_missing_key"))
            } else {
                anyhow::anyhow!(crate::t!(
                    "ai_connect_failed",
                    model = reference,
                    error = text
                ))
            }
        })
}

/// An HTTP client that ignores system proxies. Local model servers must be reached directly:
/// proxy tools (common where cloud AI services need one) would otherwise intercept
/// `localhost` requests and answer 502.
fn direct_http() -> rig_reqwest::ReqwestClient {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap_or_default()
        .into()
}

/// Rough token count: CJK characters are about one token each, other text about four
/// characters per token.
pub fn estimate_tokens(text: &str) -> usize {
    let (mut cjk, mut other) = (0usize, 0usize);
    for c in text.chars() {
        if matches!(c as u32, 0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
        {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + other.div_ceil(4)
}

/// How many input tokens of book text to send per request.
pub fn input_budget(cfg: &AiConfig, model: &ChatModel) -> usize {
    if let Some(window) = model.spec().and_then(|s| s.context_window) {
        return (window as usize / 2).clamp(4_000, 120_000);
    }
    // Unknown models: local servers often run with small context windows.
    if cfg.is_local() { 6_000 } else { 24_000 }
}

/// (input, output) price in USD per million tokens, when the catalog knows it.
pub fn pricing(model: &ChatModel) -> Option<(f64, f64)> {
    model.spec()?.pricing.as_ref().map(|p| (p.input, p.output))
}

/// Streams a reply, calling `on_text` with each fragment; returns the whole text.
#[allow(
    clippy::result_large_err,
    reason = "rig's error type; callers convert it right away"
)]
pub async fn stream_text(
    model: &ChatModel,
    request: CompletionRequest,
    mut on_text: impl FnMut(&str),
) -> Result<String, ProviderError> {
    let mut stream = model.stream(request)?;
    let mut text = String::new();
    while let Some(item) = stream.next().await {
        if let Item::Event(StreamEvent::Text { text: fragment, .. }) = item? {
            on_text(&fragment);
            text.push_str(&fragment);
        }
    }
    Ok(text)
}

/// One complete reply as text.
#[allow(
    clippy::result_large_err,
    reason = "rig's error type; callers convert it right away"
)]
pub async fn complete_text(
    model: &ChatModel,
    request: CompletionRequest,
) -> Result<String, ProviderError> {
    Ok(model.call(request).await?.text())
}

/// A message for the user, without provider JSON noise.
pub fn friendly_error(cfg: &AiConfig, error: &ProviderError) -> String {
    let body = error.provider_response_body().unwrap_or("").to_lowercase();
    let status = error
        .provider_response()
        .and_then(|r| r.status)
        .map(|s| s.as_u16());
    match error {
        _ if matches!(error, ProviderError::InvalidAuthentication(_))
            || matches!(status, Some(401 | 403)) =>
        {
            crate::t!("ai_bad_key")
        }
        _ if cfg.is_local()
            && (matches!(error, ProviderError::Http(_)) || matches!(status, Some(502..=504))) =>
        {
            crate::t!("ai_local_unreachable")
        }
        ProviderError::Http(_) => crate::t!("ai_network"),
        _ if status == Some(429) || body.contains("quota") || body.contains("rate limit") => {
            crate::t!("ai_rate_limited")
        }
        _ if status == Some(404)
            || body.contains("model_not_found")
            || body.contains("not found") =>
        {
            crate::t!("ai_model_not_found", model = cfg.model)
        }
        _ if body.contains("context")
            && (body.contains("length")
                || body.contains("too long")
                || body.contains("maximum")) =>
        {
            crate::t!("ai_too_long")
        }
        _ => {
            let detail = error.to_string();
            let detail: String = detail.chars().take(300).collect();
            crate::t!("ai_service_error", detail = detail)
        }
    }
}

/// Models installed on a local Ollama daemon.
pub async fn list_ollama_models(cfg: &AiConfig) -> Result<Vec<String>> {
    use rig_core::providers::ollama::OllamaConfig;
    let mut config = OllamaConfig::new();
    if let Some(url) = &cfg.base_url {
        config = config.with_base_url(url);
    }
    let client = if cfg.is_local() {
        config.connect(direct_http())
    } else {
        config.client()
    };
    let list = client
        .list_models()
        .await
        .map_err(|e| anyhow::anyhow!(friendly_error(cfg, &e)))?;
    Ok(list.data.into_iter().map(|m| m.id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimates() {
        assert_eq!(estimate_tokens("你好世界"), 4);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        assert_eq!(estimate_tokens("中文 text"), 2 + 2);
    }

    #[test]
    fn connect_validates_config() {
        assert!(connect(&AiConfig::default(), None).is_err());
        let custom = AiConfig {
            provider: CUSTOM.into(),
            model: "m".into(),
            ..Default::default()
        };
        assert!(
            connect(&custom, None)
                .unwrap_err()
                .to_string()
                .contains("endpoint URL")
        );
        let local = AiConfig {
            base_url: Some("http://localhost:1234/v1".into()),
            ..custom
        };
        assert!(
            connect(&local, None).is_ok(),
            "local OpenAI-compatible servers need no key"
        );
        let ollama = AiConfig {
            provider: "ollama".into(),
            model: "qwen3:4b".into(),
            ..Default::default()
        };
        assert!(connect(&ollama, None).is_ok());
        let claude = AiConfig {
            provider: "anthropic".into(),
            model: "claude-opus-5-5".into(),
            ..Default::default()
        };
        let model = connect(&claude, Some("sk-test".into())).unwrap();
        assert!(
            input_budget(&claude, &model) >= 100_000,
            "catalog context window is used"
        );
        assert!(pricing(&model).is_some());
    }
}
