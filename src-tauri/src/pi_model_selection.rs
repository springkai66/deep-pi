use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::{app_paths::AppPaths, settings::SettingsStore};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelSelection {
    pub model_ids: Option<Vec<String>>,
}

pub(crate) fn validate_ids(ids: Option<&[String]>) -> Result<(), String> {
    if let Some(ids) = ids {
        if ids.len() > 2000
            || ids.iter().any(|id| {
                id.trim().is_empty() || id.len() > 512 || id.chars().any(char::is_control)
            })
        {
            return Err("Invalid subscription model selection".into());
        }
    }
    Ok(())
}

fn validate_catalog(ids: Option<&[String]>, catalog: &[String]) -> Result<(), String> {
    validate_ids(ids)?;
    if ids.is_some_and(|ids| ids.iter().any(|id| !catalog.contains(id))) {
        return Err("Selected model is not in the subscription catalog".into());
    }
    Ok(())
}

pub(crate) fn normalize_ids(ids: Option<Vec<String>>) -> Option<Vec<String>> {
    ids.map(|ids| {
        ids.into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    })
}

/// Only filter picker candidates. Do not change get_state, set_model, or a live session.
pub(crate) fn filter_available_models(data: &mut Value, selected: Option<&[String]>) {
    let Some(selected) = selected else { return };
    if let Some(models) = data.get_mut("models").and_then(Value::as_array_mut) {
        models.retain(|model| {
            model["provider"] != "openai-codex"
                || model["id"]
                    .as_str()
                    .is_some_and(|id| selected.iter().any(|allowed| allowed == id))
        });
    }
}

#[tauri::command]
pub fn pi_auth_model_selection(app: AppHandle) -> Result<ModelSelection, String> {
    Ok(ModelSelection {
        model_ids: app.state::<SettingsStore>().get()?.codex_selected_models,
    })
}

#[tauri::command]
pub async fn pi_auth_save_model_selection(
    app: AppHandle,
    request: ModelSelection,
) -> Result<ModelSelection, String> {
    tauri::async_runtime::spawn_blocking(move || {
        validate_ids(request.model_ids.as_deref())?;
        if request
            .model_ids
            .as_ref()
            .is_some_and(|ids| !ids.is_empty())
        {
            let catalog = crate::pi_auth::official_models_for(
                &app,
                &app.state::<AppPaths>(),
                "openai-codex",
            )?;
            let known: Vec<String> = catalog.into_iter().map(|model| model.id).collect();
            validate_catalog(request.model_ids.as_deref(), &known)?;
        }
        let model_ids = normalize_ids(request.model_ids);
        app.state::<SettingsStore>()
            .set_codex_selected_models(model_ids.clone())?;
        Ok(ModelSelection { model_ids })
    })
    .await
    .map_err(|_| "Could not save subscription model selection".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selection_filters_only_codex_and_distinguishes_all_from_none() {
        let all = json!({"models":[
            {"provider":"openai-codex","id":"one","name":"One"},
            {"provider":"openai-codex","id":"two","name":"Two"},
            {"provider":"other","id":"two","name":"Other"}
        ],"unrelated":"preserve"});
        let mut data = all.clone();
        filter_available_models(&mut data, None);
        assert_eq!(data, all);
        filter_available_models(&mut data, Some(&["two".into()]));
        assert_eq!(data["models"].as_array().unwrap().len(), 2);
        assert_eq!(data["models"][0]["id"], "two");
        assert_eq!(data["models"][1]["provider"], "other");
        assert_eq!(data["unrelated"], "preserve");
        filter_available_models(&mut data, Some(&[]));
        assert_eq!(data["models"].as_array().unwrap().len(), 1);
        assert_eq!(data["models"][0]["provider"], "other");
        let mut fresh = all.clone();
        filter_available_models(&mut fresh, Some(&["removed-from-catalog".into()]));
        assert_eq!(fresh["models"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn catalog_validation_rejects_unknown_models_and_normalizes_duplicates() {
        let known = vec!["one".into(), "two".into()];
        assert!(validate_catalog(None, &known).is_ok());
        assert!(validate_catalog(Some(&[]), &known).is_ok());
        assert!(validate_catalog(Some(&["three".into()]), &known).is_err());
        assert!(validate_ids(Some(&[" ".into()])).is_err());
        assert!(validate_ids(Some(&["bad\nmodel".into()])).is_err());
        assert!(validate_ids(Some(&vec!["one".into(); 2001])).is_err());
        assert_eq!(
            normalize_ids(Some(vec!["two".into(), "one".into(), "two".into()])),
            Some(vec!["one".into(), "two".into()])
        );
    }
}
