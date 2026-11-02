//! Reading aloud with an online voice instead of the system's: one sentence of text in, MP3
//! audio out. The frontend keeps the choice of service in its settings; keys are in the keychain.

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::t;

const USER_AGENT: &str = concat!("AiReader/", env!("CARGO_PKG_VERSION"));

/// Which service, and how to reach it. Fields a service doesn't use are ignored.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TtsConfig {
    /// "openai" (any OpenAI-compatible service), "azure", "volcengine" or "elevenlabs".
    pub provider: String,
    /// OpenAI-compatible: the API root, e.g. https://api.openai.com/v1.
    pub base_url: String,
    pub model: String,
    pub voice: String,
    /// Azure: the resource's region, e.g. eastasia.
    pub region: String,
    /// Volcengine: the app id and cluster from its console.
    pub app_id: String,
    pub cluster: String,
}

/// The keychain entry for a service's key.
pub fn key_name(provider: &str) -> String {
    format!("tts:{provider}")
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .build()?)
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The Azure SSML for a sentence. The language comes from the voice name (zh-CN-XiaoxiaoNeural).
fn azure_ssml(voice: &str, text: &str) -> String {
    let lang: String = voice.split('-').take(2).collect::<Vec<_>>().join("-");
    format!(
        "<speak version='1.0' xml:lang='{lang}'><voice name='{voice}'>{}</voice></speak>",
        escape_xml(text),
        voice = escape_xml(voice),
        lang = escape_xml(&lang),
    )
}

/// An error a user can act on, from a failed response.
fn failed(status: StatusCode, body: &str) -> anyhow::Error {
    match status.as_u16() {
        401 | 403 => anyhow!(t!("tts_auth")),
        429 => anyhow!(t!("tts_rate")),
        code => {
            let detail: String = body.chars().take(200).collect();
            log::warn!("tts request failed: {code} {detail}");
            anyhow!(t!("tts_failed", status = code))
        }
    }
}

fn network(e: reqwest::Error) -> anyhow::Error {
    log::warn!("tts request failed: {e}");
    anyhow!(t!("tts_network"))
}

async fn audio(response: reqwest::Response) -> Result<Vec<u8>> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(failed(status, &body));
    }
    Ok(response.bytes().await.map_err(network)?.to_vec())
}

/// MP3 audio of `text` read by the configured service.
pub async fn synthesize(cfg: &TtsConfig, key: Option<&str>, text: &str) -> Result<Vec<u8>> {
    let key = key.filter(|k| !k.is_empty());
    let need_key = || key.ok_or_else(|| anyhow!(t!("tts_no_key")));
    let http = client()?;
    match cfg.provider.as_str() {
        "openai" => {
            let base = cfg.base_url.trim().trim_end_matches('/');
            if !(base.starts_with("http://") || base.starts_with("https://")) {
                bail!(t!("opds_bad_address"));
            }
            // Local servers (Kokoro and the like) take no key.
            let mut request = http
                .post(format!("{base}/audio/speech"))
                .json(&serde_json::json!({
                    "model": cfg.model.trim(),
                    "voice": cfg.voice.trim(),
                    "input": text,
                    "response_format": "mp3",
                }));
            if let Some(key) = key {
                request = request.bearer_auth(key);
            }
            audio(request.send().await.map_err(network)?).await
        }
        "azure" => {
            let region = cfg.region.trim();
            if region.is_empty() || !region.chars().all(|c| c.is_ascii_alphanumeric()) {
                bail!(t!("tts_bad_region"));
            }
            let response = http
                .post(format!(
                    "https://{region}.tts.speech.microsoft.com/cognitiveservices/v1"
                ))
                .header("Ocp-Apim-Subscription-Key", need_key()?)
                .header("Content-Type", "application/ssml+xml")
                .header(
                    "X-Microsoft-OutputFormat",
                    "audio-24khz-48kbitrate-mono-mp3",
                )
                .body(azure_ssml(cfg.voice.trim(), text))
                .send()
                .await
                .map_err(network)?;
            audio(response).await
        }
        "volcengine" => {
            let token = need_key()?;
            let cluster = match cfg.cluster.trim() {
                "" => "volcano_tts",
                c => c,
            };
            let reqid: String = {
                use std::fmt::Write;
                let mut id = String::new();
                for b in uuid_bytes() {
                    let _ = write!(id, "{b:02x}");
                }
                id
            };
            let response = http
                .post("https://openspeech.bytedance.com/api/v1/tts")
                .header("Authorization", format!("Bearer;{token}"))
                .json(&serde_json::json!({
                    "app": { "appid": cfg.app_id.trim(), "token": token, "cluster": cluster },
                    "user": { "uid": "aireader" },
                    "audio": { "voice_type": cfg.voice.trim(), "encoding": "mp3" },
                    "request": { "reqid": reqid, "text": text, "operation": "query" },
                }))
                .send()
                .await
                .map_err(network)?;
            let status = response.status();
            let body = response.text().await.map_err(network)?;
            // Errors come back as JSON with a code, sometimes with status 200.
            let reply: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
            let code = reply["code"].as_i64().unwrap_or(-1);
            if !status.is_success() || code != 3000 {
                let status = if status.is_success() {
                    StatusCode::BAD_GATEWAY
                } else {
                    status
                };
                return Err(failed(status, &body));
            }
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(reply["data"].as_str().unwrap_or(""))
                .map_err(|_| anyhow!(t!("tts_failed", status = 502)))
        }
        "elevenlabs" => {
            let voice = cfg.voice.trim();
            if voice.is_empty() || !voice.chars().all(|c| c.is_ascii_alphanumeric()) {
                bail!(t!("tts_bad_voice"));
            }
            let model = match cfg.model.trim() {
                "" => "eleven_multilingual_v2",
                m => m,
            };
            let response = http
                .post(format!(
                    "https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format=mp3_44100_128"
                ))
                .header("xi-api-key", need_key()?)
                .json(&serde_json::json!({ "text": text, "model_id": model }))
                .send()
                .await
                .map_err(network)?;
            audio(response).await
        }
        _ => bail!(t!("tts_no_service")),
    }
}

/// 16 random bytes for a request id.
fn uuid_bytes() -> [u8; 16] {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut out = [0u8; 16];
    for half in out.chunks_mut(8) {
        half.copy_from_slice(&RandomState::new().build_hasher().finish().to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssml_escapes_the_text_and_takes_the_language_from_the_voice() {
        let ssml = azure_ssml("zh-CN-XiaoxiaoNeural", "甲 < 乙 & \"丙\"");
        assert!(ssml.contains("xml:lang='zh-CN'"));
        assert!(ssml.contains("<voice name='zh-CN-XiaoxiaoNeural'>"));
        assert!(ssml.contains("甲 &lt; 乙 &amp; &quot;丙&quot;"));
    }

    #[tokio::test]
    async fn bad_settings_are_refused_before_anything_is_sent() {
        let cfg = |provider: &str| TtsConfig {
            provider: provider.into(),
            ..Default::default()
        };
        assert!(synthesize(&cfg("azure"), Some("k"), "x").await.is_err());
        assert!(
            synthesize(&cfg("elevenlabs"), Some("k"), "x")
                .await
                .is_err()
        );
        assert!(synthesize(&cfg("volcengine"), None, "x").await.is_err());
        assert!(synthesize(&cfg("openai"), None, "x").await.is_err());
        assert!(synthesize(&cfg("nope"), None, "x").await.is_err());
    }
}

// --- commands ---------------------------------------------------------------------------------

use crate::ai::commands::{secret, set_secret};
use crate::commands::{CmdResult, err};
use tauri::AppHandle;

/// MP3 audio of `text` (a sentence) from the service in `config`.
#[tauri::command]
pub async fn tts_synthesize(
    app: AppHandle,
    config: TtsConfig,
    text: String,
) -> CmdResult<tauri::ipc::Response> {
    let key = secret(&app, &key_name(&config.provider));
    synthesize(&config, key.as_deref(), &text)
        .await
        .map(tauri::ipc::Response::new)
        .map_err(err)
}

/// Stores a service's key in the keychain; an empty key removes it.
#[tauri::command]
pub fn tts_set_key(app: AppHandle, provider: String, key: String) -> CmdResult<()> {
    set_secret(&app, &key_name(&provider), key.trim()).map_err(err)
}

#[tauri::command]
pub fn tts_has_key(app: AppHandle, provider: String) -> bool {
    secret(&app, &key_name(&provider)).is_some()
}
