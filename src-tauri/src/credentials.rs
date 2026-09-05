use std::{
    fs,
    io::{self, Write},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::{
    app_paths::AppPaths,
    provider::{list_provider_file, validate_provider_id},
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

fn entry(provider_id: &str) -> Result<keyring::Entry, String> {
    validate_provider_id(provider_id)?;
    keyring::Entry::new(KEYRING_SERVICE, provider_id)
        .map_err(|error| format!("failed to open Windows Credential Manager entry: {error}"))
}

fn validate_api_key(api_key: &str) -> Result<(), String> {
    if api_key.trim().is_empty()
        || api_key.len() > MAX_API_KEY_LENGTH
        || api_key.chars().any(|character| character.is_control())
    {
        return Err("API key is invalid".into());
    }
    Ok(())
}

fn validate_connection_url(base_url: &str) -> Result<(), String> {
    let url =
        tauri::Url::parse(base_url).map_err(|error| format!("base URL is invalid: {error}"))?;
    let is_loopback_http =
        url.scheme() == "http" && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !is_loopback_http {
        return Err("base URL must use HTTPS or loopback HTTP".into());
    }
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
        return Err("base URL must not contain credentials or a query".into());
    }
    Ok(())
}

fn credential_configured(provider_id: &str) -> Result<bool, String> {
    match entry(provider_id)?.get_password() {
        Ok(api_key) => Ok(!api_key.is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(format!("failed to read provider credential: {error}")),
    }
}

pub(crate) fn provider_api_key(provider_id: &str) -> Result<Option<String>, String> {
    match entry(provider_id)?.get_password() {
        Ok(api_key) if !api_key.is_empty() => Ok(Some(api_key)),
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("failed to read provider credential: {error}")),
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
            keyring::Error::NoEntry => "provider credential is not configured".to_string(),
            error => format!("failed to read provider credential: {error}"),
        })?;
    io::stdout()
        .write_all(api_key.as_bytes())
        .map_err(|error| format!("failed to write provider credential: {error}"))
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
        .map_err(|error| format!("failed to save provider credential: {error}"))?;
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
        Err(error) => Err(format!("failed to delete provider credential: {error}")),
    }
}

#[tauri::command]
pub fn test_provider_connection(
    paths: State<'_, AppPaths>,
    request: ProviderCredentialRequest,
) -> Result<ProviderConnectionResult, String> {
    let provider_id = request.provider_id.clone();
    validate_provider_id(&provider_id)?;
    let provider = list_provider_file(&paths.pi_models_file())?
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("Pi provider not found: {provider_id}"))?;
    let base_url = provider
        .base_url
        .as_deref()
        .ok_or_else(|| "provider has no base URL".to_string())?;
    validate_connection_url(base_url)?;
    let agent = crate::provider::provider_agent(&provider)?;
    let api_key = provider_api_key(&provider.id)?;
    let mut request = agent.get(base_url).header("Accept", "application/json");
    for (name, value) in &provider.headers {
        request = request.header(name, value);
    }
    if let Some(api_key) = api_key.as_deref() {
        request = match provider.api.as_str() {
            "anthropic-messages" => request
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01"),
            "google-generative-ai" => request.header("x-goog-api-key", api_key),
            _ => request.header("Authorization", &format!("Bearer {api_key}")),
        };
    }
    let response = request
        .call()
        .map_err(|error| format!("provider connection failed: {error}"))?;
    Ok(ProviderConnectionResult {
        provider_id,
        reachable: true,
        status: response.status().as_u16(),
    })
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
