use std::{fs::File, io::Read, path::Path};

use serde::Deserialize;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};

use crate::{app_paths::AppPaths, provider::ProviderConfigGate};

const PROVIDER_ID: &str = "openai-codex";
const MAX_MODEL_ID_BYTES: usize = 512;
const MAX_MODELS_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const INVALID_REQUEST: &str = "Invalid official model limits request";
const INVALID_FILE: &str = "Invalid Pi models configuration";
const FILE_UNAVAILABLE: &str = "Pi models configuration unavailable";
const CATALOG_UNAVAILABLE: &str = "Official model catalog unavailable";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OfficialModelLimitsRequest {
    pub provider_id: String,
    pub model_id: String,
    pub context_window: Option<u64>,
    pub max_tokens: Option<u64>,
}

fn validate_request(
    request: &OfficialModelLimitsRequest,
    catalog_model_ids: &[&str],
) -> Result<(), String> {
    if request.provider_id != PROVIDER_ID
        || request.model_id.trim().is_empty()
        || request.model_id.len() > MAX_MODEL_ID_BYTES
        || request.model_id.chars().any(char::is_control)
        || !catalog_model_ids.contains(&request.model_id.as_str())
    {
        return Err(INVALID_REQUEST.into());
    }
    for value in [request.context_window, request.max_tokens]
        .into_iter()
        .flatten()
    {
        if value == 0 || value > MAX_SAFE_INTEGER {
            return Err(INVALID_REQUEST.into());
        }
    }
    if matches!((request.context_window, request.max_tokens), (Some(context), Some(max)) if max > context)
    {
        return Err(INVALID_REQUEST.into());
    }
    Ok(())
}

fn object_entry<'a>(
    object: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    object
        .entry(key.to_owned())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| INVALID_FILE.into())
}

fn merge_limits(root: &mut Value, request: &OfficialModelLimitsRequest) -> Result<(), String> {
    let providers = object_entry(root.as_object_mut().ok_or(INVALID_FILE)?, "providers")?;
    let provider = object_entry(providers, PROVIDER_ID)?;
    let overrides = object_entry(provider, "modelOverrides")?;
    let model = object_entry(overrides, &request.model_id)?;
    for (key, value) in [
        ("contextWindow", request.context_window),
        ("maxTokens", request.max_tokens),
    ] {
        match value {
            Some(value) => {
                model.insert(key.into(), json!(value));
            }
            None => {
                model.remove(key);
            }
        }
    }
    Ok(())
}

fn read_models_file(path: &Path) -> Result<Value, String> {
    crate::snapshot::reject_link(path).map_err(|_| FILE_UNAVAILABLE.to_owned())?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({ "providers": {} }));
        }
        Err(_) => return Err(FILE_UNAVAILABLE.into()),
    };
    let metadata = file.metadata().map_err(|_| FILE_UNAVAILABLE.to_owned())?;
    if !metadata.is_file() || metadata.len() > MAX_MODELS_FILE_BYTES {
        return Err(INVALID_FILE.into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_MODELS_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FILE_UNAVAILABLE.to_owned())?;
    if bytes.len() as u64 > MAX_MODELS_FILE_BYTES {
        return Err(INVALID_FILE.into());
    }
    let root: Value = serde_json::from_slice(&bytes).map_err(|_| INVALID_FILE.to_owned())?;
    if !root.is_object() {
        return Err(INVALID_FILE.into());
    }
    Ok(root)
}

fn save_limits_file(
    path: &Path,
    request: &OfficialModelLimitsRequest,
    catalog_model_ids: &[&str],
) -> Result<(), String> {
    validate_request(request, catalog_model_ids)?;
    let mut root = read_models_file(path)?;
    merge_limits(&mut root, request)?;
    let bytes = serde_json::to_vec_pretty(&root).map_err(|_| INVALID_FILE.to_owned())?;
    if bytes.len() as u64 > MAX_MODELS_FILE_BYTES {
        return Err(INVALID_FILE.into());
    }
    crate::durable_file::write(path, &bytes).map_err(|_| FILE_UNAVAILABLE.to_owned())
}

#[tauri::command]
pub async fn pi_auth_save_model_limits(
    app: AppHandle,
    request: OfficialModelLimitsRequest,
) -> Result<Vec<crate::pi_auth::PiOfficialModel>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if request.provider_id != PROVIDER_ID {
            return Err(INVALID_REQUEST.into());
        }
        let paths = app.state::<AppPaths>();
        let catalog = crate::pi_auth::official_models_for(&app, &paths, PROVIDER_ID)
            .map_err(|_| CATALOG_UNAVAILABLE.to_owned())?;
        let ids: Vec<&str> = catalog.iter().map(|model| model.id.as_str()).collect();
        validate_request(&request, &ids)?;
        {
            let gate = app.state::<ProviderConfigGate>();
            let _permit = gate.0.lock().map_err(|_| FILE_UNAVAILABLE.to_owned())?;
            save_limits_file(&paths.pi_home.join("models.json"), &request, &ids)?;
        }
        crate::pi_auth::official_models_for(&app, &paths, PROVIDER_ID)
            .map_err(|_| CATALOG_UNAVAILABLE.to_owned())
    })
    .await
    .map_err(|_| FILE_UNAVAILABLE.to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn request(context_window: Option<u64>, max_tokens: Option<u64>) -> OfficialModelLimitsRequest {
        OfficialModelLimitsRequest {
            provider_id: PROVIDER_ID.into(),
            model_id: "gpt-5".into(),
            context_window,
            max_tokens,
        }
    }

    #[test]
    fn adds_and_updates_only_requested_limit_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("models.json");
        let original = json!({
            "unrelated": {"sentinel": true},
            "providers": {
                "other": {"models": [{"id": "other-model"}]},
                "openai-codex": {
                    "apiKey": "!credential-helper", "baseUrl": "https://example.invalid",
                    "headers": {"Authorization": "sentinel"},
                    "models": [{"id": "custom"}],
                    "modelOverrides": {
                        "other-model": {"name": "keep"},
                        "gpt-5": {"name": "sentinel", "contextWindow": 100}
                    }
                }
            }
        });
        fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        save_limits_file(&path, &request(Some(200), Some(50)), &["gpt-5"]).unwrap();
        let updated: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let mut expected = original.clone();
        expected["providers"]["openai-codex"]["modelOverrides"]["gpt-5"]["contextWindow"] =
            json!(200);
        expected["providers"]["openai-codex"]["modelOverrides"]["gpt-5"]["maxTokens"] = json!(50);
        assert_eq!(updated, expected);
        save_limits_file(&path, &request(Some(250), Some(60)), &["gpt-5"]).unwrap();
        expected["providers"]["openai-codex"]["modelOverrides"]["gpt-5"]["contextWindow"] =
            json!(250);
        expected["providers"]["openai-codex"]["modelOverrides"]["gpt-5"]["maxTokens"] = json!(60);
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
            expected
        );
    }

    #[test]
    fn reset_removes_only_limit_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("models.json");
        let original = json!({"providers": {"openai-codex": {
            "apiKey": "sentinel", "modelOverrides": {
                "gpt-5": {"name": "keep", "contextWindow": 100, "maxTokens": 20},
                "other": {"contextWindow": 300}
            }
        }}});
        fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        save_limits_file(&path, &request(None, None), &["gpt-5"]).unwrap();
        let mut expected = original;
        expected["providers"]["openai-codex"]["modelOverrides"]["gpt-5"] = json!({"name": "keep"});
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
            expected
        );
    }

    #[test]
    fn rejects_invalid_request_and_corrupt_file_without_overwriting() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("models.json");
        let original = b"{\"providers\":{\"openai-codex\":{\"apiKey\":\"sentinel\"}}}";
        fs::write(&path, original).unwrap();
        let mut invalid = request(Some(200), Some(20));
        invalid.model_id = "not-in-catalog".into();
        assert!(save_limits_file(&path, &invalid, &["gpt-5"]).is_err());
        invalid.model_id = "gpt-5".into();
        invalid.provider_id = "other-provider".into();
        assert!(save_limits_file(&path, &invalid, &["gpt-5"]).is_err());
        invalid.provider_id = PROVIDER_ID.into();
        invalid.model_id.clear();
        assert!(save_limits_file(&path, &invalid, &["gpt-5"]).is_err());
        for request in [
            request(Some(0), None),
            request(None, Some(MAX_SAFE_INTEGER + 1)),
            request(Some(10), Some(11)),
        ] {
            assert!(save_limits_file(&path, &request, &["gpt-5"]).is_err());
        }
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::write(&path, b"{invalid").unwrap();
        assert!(save_limits_file(&path, &request(Some(200), None), &["gpt-5"]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{invalid");
        fs::write(
            &path,
            b"{\"providers\":{\"openai-codex\":{\"modelOverrides\":[]}}}",
        )
        .unwrap();
        let malformed = fs::read(&path).unwrap();
        assert!(save_limits_file(&path, &request(Some(200), None), &["gpt-5"]).is_err());
        assert_eq!(fs::read(&path).unwrap(), malformed);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_and_non_file_targets() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let target = dir.path().join("target.json");
        let link = dir.path().join("models.json");
        fs::write(&target, b"{\"providers\":{}}").unwrap();
        symlink(&target, &link).unwrap();
        assert!(save_limits_file(&link, &request(Some(100), None), &["gpt-5"]).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"{\"providers\":{}}");
        fs::remove_file(&link).unwrap();
        fs::create_dir(&link).unwrap();
        assert!(save_limits_file(&link, &request(Some(100), None), &["gpt-5"]).is_err());
    }
}
