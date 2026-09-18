use std::{
    fs,
    io::{self, Write},
    time::Instant,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use crate::message::{msg, msg_with};
use crate::{
    app_paths::AppPaths,
    provider::{list_provider_file, provider_agent, validate_provider_id},
};

const KEYRING_SERVICE: &str = "com.deeppi.desktop.pi.provider";
const MAX_API_KEY_LENGTH: usize = 8_192;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCredentialRequest {
    provider_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderCredentialRequest {
    provider_id: String,
    api_key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCredentialStatus {
    provider_id: String,
    configured: bool,
    api_key_configured: bool,
    native_auth_configured: bool,
    auth_type: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnectionResult {
    provider_id: String,
    reachable: bool,
    status: u16,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTestRequest {
    provider_id: String,
    model_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTestResult {
    model_id: String,
    ok: bool,
    status: Option<u16>,
    latency_ms: Option<u64>,
    error: Option<String>,
}

fn entry(provider_id: &str) -> Result<keyring::Entry, String> {
    validate_provider_id(provider_id)?;
    keyring::Entry::new(KEYRING_SERVICE, provider_id)
        .map_err(|error| msg_with("credentials.open.failed", &[("error", &error.to_string())]))
}

fn validate_api_key(api_key: &str) -> Result<(), String> {
    if api_key.trim().is_empty()
        || api_key.len() > MAX_API_KEY_LENGTH
        || api_key.chars().any(|character| character.is_control())
    {
        return Err(msg("credentials.api_key.invalid"));
    }
    Ok(())
}

fn validate_connection_url(base_url: &str) -> Result<(), String> {
    let url = tauri::Url::parse(base_url).map_err(|error| {
        msg_with(
            "provider.base_url.unparsable",
            &[("error", &error.to_string())],
        )
    })?;
    let is_loopback_http =
        url.scheme() == "http" && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !is_loopback_http {
        return Err(msg("provider.base_url.scheme"));
    }
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
        return Err(msg("provider.base_url.query"));
    }
    Ok(())
}

fn credential_configured(provider_id: &str) -> Result<bool, String> {
    match entry(provider_id)?.get_password() {
        Ok(api_key) => Ok(!api_key.is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(msg_with(
            "credentials.read.failed",
            &[("error", &error.to_string())],
        )),
    }
}

pub(crate) fn provider_api_key(provider_id: &str) -> Result<Option<String>, String> {
    match entry(provider_id)?.get_password() {
        Ok(api_key) if !api_key.is_empty() => Ok(Some(api_key)),
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(msg_with(
            "credentials.read.failed",
            &[("error", &error.to_string())],
        )),
    }
}

fn native_auth_type(paths: &AppPaths, provider_id: &str) -> Option<String> {
    let content = fs::read_to_string(paths.pi_auth_file()).ok()?;
    let root: Value = serde_json::from_str(&content).ok()?;
    let entry = root.get(provider_id)?;
    let auth_type = entry.get("type").and_then(Value::as_str)?;
    matches!(auth_type, "oauth" | "api_key").then(|| auth_type.to_owned())
}

pub fn credential_helper(provider_id: &str) -> Result<(), String> {
    let api_key = entry(provider_id)?
        .get_password()
        .map_err(|error| match error {
            keyring::Error::NoEntry => msg("credentials.not_configured"),
            error => msg_with("credentials.read.failed", &[("error", &error.to_string())]),
        })?;
    io::stdout()
        .write_all(api_key.as_bytes())
        .map_err(|error| msg_with("credentials.write.failed", &[("error", &error.to_string())]))
}

#[tauri::command]
pub fn provider_credential_status(
    paths: State<'_, AppPaths>,
    request: ProviderCredentialRequest,
) -> Result<ProviderCredentialStatus, String> {
    let api_key_configured = credential_configured(&request.provider_id)?;
    let auth_type = native_auth_type(&paths, &request.provider_id);
    Ok(ProviderCredentialStatus {
        configured: api_key_configured || auth_type.is_some(),
        provider_id: request.provider_id,
        api_key_configured,
        native_auth_configured: auth_type.is_some(),
        auth_type,
    })
}

#[tauri::command]
pub fn save_provider_credential(
    request: SaveProviderCredentialRequest,
) -> Result<ProviderCredentialStatus, String> {
    validate_api_key(&request.api_key)?;
    entry(&request.provider_id)?
        .set_password(&request.api_key)
        .map_err(|error| msg_with("credentials.save.failed", &[("error", &error.to_string())]))?;
    Ok(ProviderCredentialStatus {
        provider_id: request.provider_id,
        configured: true,
        api_key_configured: true,
        native_auth_configured: false,
        auth_type: Some("api_key".into()),
    })
}

#[tauri::command]
pub fn delete_provider_credential(request: ProviderCredentialRequest) -> Result<(), String> {
    match entry(&request.provider_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(msg_with(
            "credentials.delete.failed",
            &[("error", &error.to_string())],
        )),
    }
}

#[tauri::command]
pub async fn test_provider_connection(
    app: tauri::AppHandle,
    request: ProviderCredentialRequest,
) -> Result<ProviderConnectionResult, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        test_provider_connection_inner(app.state(), request)
    })
    .await
    .map_err(|error| {
        msg_with(
            "provider.connection.worker_failed",
            &[("error", &error.to_string())],
        )
    })?
}

fn test_provider_connection_inner(
    paths: State<'_, AppPaths>,
    request: ProviderCredentialRequest,
) -> Result<ProviderConnectionResult, String> {
    let provider_id = request.provider_id.clone();
    validate_provider_id(&provider_id)?;
    let provider = list_provider_file(&paths.pi_models_file())?
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| msg_with("provider.not_found", &[("provider", &provider_id)]))?;
    let base_url = provider
        .base_url
        .as_deref()
        .ok_or_else(|| msg("provider.base_url.missing"))?;
    validate_connection_url(base_url)?;
    // 探测 /models 端点（与拉取模型一致），而不是裸根地址：
    // 裸根 GET 常见 404，无法区分“网络可达但配置错误”和“服务健康”。
    let probe_url = crate::provider::provider_models_url(base_url, &provider.api)
        .map(|url| url.to_string())
        .unwrap_or_else(|_| base_url.to_string());
    let agent = crate::provider::provider_agent(&provider)?;
    let api_key = provider_api_key(&provider.id)?;
    let request = agent.get(&probe_url).header("Accept", "application/json");
    let request = crate::provider::apply_provider_auth(request, &provider, api_key.as_deref());
    let response = request.call().map_err(|error| {
        msg_with(
            "provider.connection.failed",
            &[("error", &error.to_string())],
        )
    })?;
    let status = response.status().as_u16();
    Ok(ProviderConnectionResult {
        provider_id,
        reachable: (200..300).contains(&status),
        status,
    })
}

#[tauri::command]
pub async fn test_model_connection(
    app: tauri::AppHandle,
    request: ModelTestRequest,
) -> Result<ModelTestResult, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || test_model_connection_inner(app.state(), request))
        .await
        .map_err(|error| {
            msg_with(
                "provider.model_test.worker_failed",
                &[("error", &error.to_string())],
            )
        })?
}

fn model_probe_target(
    api: &str,
    base_url: &str,
    model_id: &str,
) -> Result<Vec<(String, Value)>, String> {
    let base = base_url.trim_end_matches('/');
    match api {
        "openai-completions" => Ok(vec![(
            format!("{base}/chat/completions"),
            json!({
                "model": model_id,
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 1,
                "stream": false,
            }),
        )]),
        // openai-responses 端点在部分兼容网关上不存在（404）；
        // 依次尝试 /responses，再回退 /chat/completions。
        "openai-responses" => Ok(vec![
            (
                format!("{base}/responses"),
                json!({"model": model_id, "input": "ping", "max_output_tokens": 16}),
            ),
            (
                format!("{base}/chat/completions"),
                json!({
                    "model": model_id,
                    "messages": [{"role": "user", "content": "ping"}],
                    "max_tokens": 1,
                    "stream": false,
                }),
            ),
        ]),
        "anthropic-messages" => {
            let url = if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            };
            Ok(vec![(
                url,
                json!({
                    "model": model_id,
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "ping"}],
                }),
            )])
        }
        "google-generative-ai" => Ok(vec![(
            format!("{base}/models/{model_id}:generateContent"),
            json!({
                "contents": [{"parts": [{"text": "ping"}]}],
                "generationConfig": {"maxOutputTokens": 1},
            }),
        )]),
        _ => Err(msg_with("credentials.api.unsupported", &[("api", api)])),
    }
}

fn test_model_connection_inner(
    paths: State<'_, AppPaths>,
    request: ModelTestRequest,
) -> Result<ModelTestResult, String> {
    let model_id = request.model_id.trim().to_string();
    if model_id.is_empty() || model_id.len() > 256 || model_id.chars().any(char::is_control) {
        return Err(msg("provider.model_id.invalid"));
    }
    let provider = list_provider_file(&paths.pi_models_file())?
        .into_iter()
        .find(|provider| provider.id == request.provider_id)
        .ok_or_else(|| msg_with("provider.not_found", &[("provider", &request.provider_id)]))?;
    let base_url = provider
        .base_url
        .as_deref()
        .ok_or_else(|| msg("provider.base_url.missing"))?;
    validate_connection_url(base_url)?;
    let targets = model_probe_target(&provider.api, base_url, &model_id)?;
    let agent = provider_agent(&provider)?;
    let api_key = provider_api_key(&provider.id)?;
    let mut result = ModelTestResult {
        model_id,
        ok: false,
        status: None,
        latency_ms: None,
        error: None,
    };
    let mut last_error = String::new();
    for (target_url, body) in targets {
        let url = tauri::Url::parse(&target_url).map_err(|error| {
            msg_with(
                "provider.test_url.invalid",
                &[("error", &error.to_string())],
            )
        })?;
        let request = agent
            .post(url.as_str())
            .header("Accept", "application/json");
        let request = crate::provider::apply_provider_auth(request, &provider, api_key.as_deref());
        let started = Instant::now();
        let send = request.send_json(&body);
        let latency = started.elapsed().as_millis() as u64;
        result.latency_ms = Some(latency);
        match send {
            Ok(response) => {
                let status = response.status().as_u16();
                result.status = Some(status);
                if (200..300).contains(&status) {
                    result.ok = true;
                    result.error = None;
                    break;
                }
                // 404 说明该端点不存在，尝试下一个候选；其它错误（401/429/5xx）
                // 说明端点存在但请求被拒，直接报告不再回退。
                if status != 404 {
                    result.error = Some(format!("HTTP {status}"));
                    break;
                }
                last_error = format!("HTTP {status}");
            }
            Err(error) => {
                last_error = msg_with(
                    "provider.connection.failed",
                    &[("error", &error.to_string())],
                );
                result.error = Some(last_error.clone());
                break;
            }
        }
    }
    if !result.ok && result.error.is_none() {
        result.error = Some(last_error);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{native_auth_type, validate_api_key, validate_connection_url};
    use crate::app_paths::AppPaths;

    #[test]
    fn rejects_empty_or_multiline_api_keys() {
        assert!(validate_api_key("sk-test").is_ok());
        assert!(validate_api_key("").is_err());
        assert!(validate_api_key("sk-test\nother").is_err());
    }

    #[test]
    fn detects_native_oauth_without_returning_tokens() {
        let root = std::env::temp_dir().join(format!("deeppi-auth-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = AppPaths::from_roots(root.join("roaming"), root.join("local"), root.clone())
            .expect("application paths should initialize");
        std::fs::write(
            paths.pi_auth_file(),
            br#"{"anthropic":{"type":"oauth","access":"secret-token"}}"#,
        )
        .expect("auth fixture should be written");

        assert_eq!(
            native_auth_type(&paths, "anthropic").as_deref(),
            Some("oauth")
        );
        assert_eq!(native_auth_type(&paths, "missing"), None);

        std::fs::remove_dir_all(root).expect("auth fixture should be removed");
    }

    #[test]
    fn accepts_only_safe_provider_urls() {
        assert!(validate_connection_url("https://api.example.com/v1").is_ok());
        assert!(validate_connection_url("http://127.0.0.1:8080/v1").is_ok());
        assert!(validate_connection_url("http://api.example.com/v1").is_err());
        assert!(validate_connection_url("https://user:pass@example.com/v1").is_err());
        assert!(validate_connection_url("https://api.example.com/v1?key=secret").is_err());
    }
}
