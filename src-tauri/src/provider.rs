use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::Path,
    sync::Mutex,
    time::Duration,
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager, State, Url};

use crate::{app_paths::AppPaths, credentials::provider_api_key};

const MAX_PROVIDERS: usize = 100;
const MAX_MODELS: usize = 100;
const MAX_TEXT_LENGTH: usize = 512;
const MAX_URL_LENGTH: usize = 2_000;
const MAX_PROVIDER_MODELS: usize = 200;
const MAX_PROVIDER_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_HEADERS: usize = 32;
const MAX_HEADER_NAME_LENGTH: usize = 128;
const MAX_HEADER_VALUE_LENGTH: usize = 4_096;

#[derive(Default)]
pub struct ProviderConfigGate(Mutex<()>);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostConfig {
    pub input: Option<f64>,
    pub output: Option<f64>,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredModel {
    pub id: String,
    pub name: String,
    pub reasoning: bool,
    pub input: Vec<String>,
    pub context_window: u64,
    pub max_tokens: u64,
    pub thinking_levels: Vec<String>,
    pub cost: Option<ModelCostConfig>,
    /// 覆盖 Provider 的 API 类型；空值时继承 Provider 设置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderRequest {
    pub id: String,
    pub name: String,
    pub api: String,
    pub base_url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub proxy: Option<String>,
    pub models: Vec<ConfiguredModel>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRecord {
    pub id: String,
    pub name: String,
    pub api: String,
    pub base_url: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub proxy: Option<String>,
    pub models: Vec<ConfiguredModel>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelsRequest {
    provider_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelSummary {
    pub provider_id: String,
    pub id: String,
    pub name: String,
    pub context_window: Option<u64>,
    pub max_tokens: Option<u64>,
    pub reasoning: Option<bool>,
    pub input: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProviderRequest {
    pub id: String,
}

fn valid_text(value: &str, field: &str, max_len: usize) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > max_len
        || value.chars().any(|character| character.is_control())
    {
        return Err(format!("{field} is invalid"));
    }
    Ok(())
}

pub(crate) fn validate_provider_id(id: &str) -> Result<(), String> {
    valid_text(id, "provider id", 64)?;
    if id
        .chars()
        .any(|character| !character.is_ascii_alphanumeric() && !matches!(character, '-' | '_'))
    {
        return Err("provider id may contain only letters, numbers, '-' and '_'".into());
    }
    Ok(())
}

pub fn validate_base_url(base_url: Option<&str>) -> Result<(), String> {
    let Some(base_url) = base_url else {
        return Ok(());
    };
    valid_text(base_url, "base URL", MAX_URL_LENGTH)?;
    let url = Url::parse(base_url).map_err(|error| format!("base URL is invalid: {error}"))?;
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

fn validate_headers(headers: &BTreeMap<String, String>) -> Result<(), String> {
    if headers.len() > MAX_HEADERS {
        return Err("too many provider headers".into());
    }
    for (name, value) in headers {
        if name.is_empty()
            || name.len() > MAX_HEADER_NAME_LENGTH
            || !name.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(
                        character,
                        '!' | '#'
                            | '$'
                            | '%'
                            | '&'
                            | '\''
                            | '*'
                            | '+'
                            | '-'
                            | '.'
                            | '^'
                            | '_'
                            | '`'
                            | '|'
                            | '~'
                    )
            })
        {
            return Err("provider header name is invalid".into());
        }
        if value.len() > MAX_HEADER_VALUE_LENGTH
            || value.chars().any(|character| character.is_control())
        {
            return Err("provider header value is invalid".into());
        }
    }
    Ok(())
}

pub fn validate_proxy(proxy: Option<&str>) -> Result<(), String> {
    let Some(proxy) = proxy else {
        return Ok(());
    };
    valid_text(proxy, "proxy", MAX_URL_LENGTH)?;
    let url = Url::parse(proxy).map_err(|error| format!("proxy is invalid: {error}"))?;
    let is_loopback_http =
        url.scheme() == "http" && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !is_loopback_http {
        return Err("proxy must use HTTPS or loopback HTTP".into());
    }
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
        return Err("proxy must not contain credentials or a query".into());
    }
    Ok(())
}

fn validate_cost(cost: Option<&ModelCostConfig>) -> Result<(), String> {
    let Some(cost) = cost else {
        return Ok(());
    };
    for value in [cost.input, cost.output, cost.cache_read, cost.cache_write]
        .into_iter()
        .flatten()
    {
        if !value.is_finite() || value < 0.0 {
            return Err("model costs must be finite and non-negative".into());
        }
    }
    Ok(())
}

fn validate_model(model: &ConfiguredModel) -> Result<(), String> {
    valid_text(&model.id, "model id", MAX_TEXT_LENGTH)?;
    valid_text(&model.name, "model name", MAX_TEXT_LENGTH)?;
    if model.context_window == 0 || model.context_window > 100_000_000 {
        return Err("context window is out of range".into());
    }
    if model.max_tokens == 0 || model.max_tokens > 100_000_000 {
        return Err("max tokens is out of range".into());
    }
    if model.input.is_empty() || model.input.len() > 8 {
        return Err("model input types are invalid".into());
    }
    for input in &model.input {
        valid_text(input, "model input type", 32)?;
    }
    if model.thinking_levels.len() > 8 {
        return Err("model thinking levels are invalid".into());
    }
    let mut levels = BTreeSet::new();
    for level in &model.thinking_levels {
        valid_text(level, "thinking level", 32)?;
        if !levels.insert(level) {
            return Err("model thinking levels must be unique".into());
        }
    }
    if !model.reasoning && !model.thinking_levels.is_empty() {
        return Err("non-reasoning models cannot define thinking levels".into());
    }
    validate_cost(model.cost.as_ref())
}

fn validate_provider(request: &SaveProviderRequest) -> Result<(), String> {
    validate_provider_id(&request.id)?;
    valid_text(&request.name, "provider name", MAX_TEXT_LENGTH)?;
    valid_text(&request.api, "provider API", MAX_TEXT_LENGTH)?;
    if request
        .api
        .chars()
        .any(|character| character.is_whitespace())
    {
        return Err("provider API must not contain whitespace".into());
    }
    validate_base_url(request.base_url.as_deref())?;
    validate_proxy(request.proxy.as_deref())?;
    validate_headers(&request.headers)?;
    if request.models.len() > MAX_MODELS {
        return Err("too many models".into());
    }
    let mut ids = BTreeSet::new();
    for model in &request.models {
        validate_model(model)?;
        if !ids.insert(&model.id) {
            return Err("model ids must be unique".into());
        }
    }
    Ok(())
}

fn model_to_json(model: &ConfiguredModel) -> Value {
    let thinking_level_map: Map<String, Value> = model
        .thinking_levels
        .iter()
        .map(|level| {
            (
                level.clone(),
                if level == "off" {
                    Value::Null
                } else {
                    Value::String(level.clone())
                },
            )
        })
        .collect();
    let mut output = json!({
        "id": model.id,
        "name": model.name,
        "reasoning": model.reasoning,
        "input": model.input,
        "contextWindow": model.context_window,
        "maxTokens": model.max_tokens,
        "thinkingLevelMap": thinking_level_map,
    });
    if let Some(api) = &model.api {
        output["api"] = json!(api);
    }
    if let Some(cost) = &model.cost {
        output["cost"] = json!({
            "input": cost.input,
            "output": cost.output,
            "cacheRead": cost.cache_read,
            "cacheWrite": cost.cache_write,
        });
    }
    output
}

fn model_from_json(value: &Value) -> Result<ConfiguredModel, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "Pi model entry is not an object".to_string())?;
    let thinking_levels = object
        .get("thinkingLevelMap")
        .and_then(Value::as_object)
        .map(|levels| {
            levels
                .iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(level, _)| level.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let cost = object
        .get("cost")
        .and_then(Value::as_object)
        .map(|cost| ModelCostConfig {
            input: cost.get("input").and_then(Value::as_f64),
            output: cost.get("output").and_then(Value::as_f64),
            cache_read: cost.get("cacheRead").and_then(Value::as_f64),
            cache_write: cost.get("cacheWrite").and_then(Value::as_f64),
        });
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "Pi model has no id".to_string())?
        .to_owned();
    let reasoning = object
        .get("reasoning")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let model = ConfiguredModel {
        name: object
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&id)
            .to_owned(),
        id,
        reasoning,
        input: object
            .get("input")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_else(|| vec!["text".to_owned()]),
        context_window: object
            .get("contextWindow")
            .and_then(Value::as_u64)
            .unwrap_or(128_000),
        max_tokens: object
            .get("maxTokens")
            .and_then(Value::as_u64)
            .unwrap_or(8_192),
        thinking_levels,
        cost,
        api: object
            .get("api")
            .and_then(Value::as_str)
            .filter(|api| !api.trim().is_empty())
            .map(str::to_owned),
    };
    validate_model(&model)?;
    Ok(model)
}

fn provider_from_json(id: &str, value: &Value) -> Result<ProviderRecord, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("Pi provider {id} is not an object"))?;
    let models = object
        .get("models")
        .and_then(Value::as_array)
        .map(|models| models.iter().map(model_from_json).collect())
        .transpose()?
        .unwrap_or_default();
    let headers = object
        .get("headers")
        .and_then(Value::as_object)
        .map(|headers| {
            headers
                .iter()
                .filter_map(|(name, value)| {
                    value.as_str().map(|value| (name.clone(), value.to_owned()))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    validate_headers(&headers)?;
    let proxy = object
        .get("proxy")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let base_url = object
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::to_owned);
    validate_base_url(base_url.as_deref())?;
    let record = ProviderRecord {
        id: id.to_owned(),
        name: object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_owned(),
        api: object
            .get("api")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        base_url,
        headers,
        proxy,
        models,
    };
    Ok(record)
}

fn read_models_file(path: &Path) -> Result<Value, String> {
    if !path.is_file() {
        return Ok(json!({"providers": {}}));
    }
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read Pi models.json: {error}"))?;
    let value: Value = serde_json::from_str(&content)
        .map_err(|error| format!("Pi models.json is invalid: {error}"))?;
    if !value.is_object() {
        return Err("Pi models.json root must be an object".into());
    }
    Ok(value)
}

fn write_models_file(path: &Path, value: &Value) -> Result<(), String> {
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize Pi models.json: {error}"))?;
    let mut file = AtomicWriteFile::open(path)
        .map_err(|error| format!("failed to open Pi models.json: {error}"))?;
    file.write_all(&content)
        .map_err(|error| format!("failed to write Pi models.json: {error}"))?;
    file.commit()
        .map_err(|error| format!("failed to commit Pi models.json: {error}"))
}

pub(crate) fn list_provider_file(path: &Path) -> Result<Vec<ProviderRecord>, String> {
    let value = read_models_file(path)?;
    let providers = value
        .get("providers")
        .and_then(Value::as_object)
        .ok_or_else(|| "Pi models.json providers must be an object".to_string())?;
    if providers.len() > MAX_PROVIDERS {
        return Err("too many Pi providers".into());
    }
    providers
        .iter()
        .map(|(id, value)| provider_from_json(id, value))
        .collect()
}

fn credential_command(provider_id: &str) -> Result<String, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("failed to locate DeepPi credential helper: {error}"))?;
    Ok(format!(
        "!\"{}\" credential {provider_id}",
        executable.to_string_lossy()
    ))
}

fn save_provider_file(
    path: &Path,
    request: &SaveProviderRequest,
) -> Result<ProviderRecord, String> {
    validate_provider(request)?;
    let mut root = read_models_file(path)?;
    let providers = root
        .as_object_mut()
        .expect("models file root was validated")
        .entry("providers")
        .or_insert_with(|| json!({}));
    let providers = providers
        .as_object_mut()
        .ok_or_else(|| "Pi models.json providers must be an object".to_string())?;
    if !providers.contains_key(&request.id) && providers.len() >= MAX_PROVIDERS {
        return Err("too many Pi providers".into());
    }
    let provider = providers
        .entry(request.id.clone())
        .or_insert_with(|| json!({}));
    let provider = provider
        .as_object_mut()
        .ok_or_else(|| format!("Pi provider {} is not an object", request.id))?;
    provider.insert("name".into(), Value::String(request.name.clone()));
    provider.insert("api".into(), Value::String(request.api.clone()));
    provider.insert(
        "apiKey".into(),
        Value::String(credential_command(&request.id)?),
    );
    if let Some(base_url) = &request.base_url {
        provider.insert("baseUrl".into(), Value::String(base_url.clone()));
    } else {
        provider.remove("baseUrl");
    }
    if request.headers.is_empty() {
        provider.remove("headers");
    } else {
        provider.insert("headers".into(), json!(request.headers));
    }
    if let Some(proxy) = &request.proxy {
        provider.insert("proxy".into(), Value::String(proxy.clone()));
    } else {
        provider.remove("proxy");
    }
    provider.insert(
        "models".into(),
        Value::Array(request.models.iter().map(model_to_json).collect()),
    );
    let saved_provider = Value::Object(provider.clone());
    write_models_file(path, &root)?;
    provider_from_json(&request.id, &saved_provider)
}

fn delete_provider_file(path: &Path, id: &str) -> Result<(), String> {
    validate_provider_id(id)?;
    let mut root = read_models_file(path)?;
    let providers = root
        .as_object_mut()
        .expect("models file root was validated")
        .get_mut("providers")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "Pi models.json providers must be an object".to_string())?;
    providers.remove(id);
    write_models_file(path, &root)
}

pub(crate) fn provider_models_url(base_url: &str, api: &str) -> Result<Url, String> {
    validate_base_url(Some(base_url))?;
    let mut url = Url::parse(base_url).map_err(|error| format!("base URL is invalid: {error}"))?;
    let base_path = url.path().trim_end_matches('/');
    let suffix = if api == "anthropic-messages" && !base_path.ends_with("/v1") {
        "v1/models"
    } else {
        "models"
    };
    let path = if base_path.is_empty() {
        format!("/{suffix}")
    } else {
        format!("{base_path}/{suffix}")
    };
    url.set_path(&path);
    Ok(url)
}

fn provider_response_body(body: &mut ureq::Body) -> Result<String, String> {
    body.with_config()
        .limit(MAX_PROVIDER_RESPONSE_BYTES)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(|error| format!("failed to read provider model response: {error}"))
}

fn first_string(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
    })
}

fn first_u64(object: &Map<String, Value>, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_u64))
}

fn first_bool(object: &Map<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_bool))
}

fn input_types(object: &Map<String, Value>) -> Vec<String> {
    ["input", "input_modalities", "inputModalities", "modalities"]
        .into_iter()
        .find_map(|key| {
            let values = object.get(key)?.as_array()?;
            let input = values
                .iter()
                .filter_map(Value::as_str)
                .filter(|value| {
                    !value.is_empty()
                        && value.len() <= 32
                        && !value.chars().any(|character| character.is_control())
                })
                .take(8)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            (!input.is_empty()).then_some(input)
        })
        .unwrap_or_default()
}

pub fn parse_provider_models(
    body: &str,
    provider_id: &str,
) -> Result<Vec<ProviderModelSummary>, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("provider model response is invalid JSON: {error}"))?;
    let entries = value
        .as_array()
        .or_else(|| value.get("data").and_then(Value::as_array))
        .or_else(|| value.get("models").and_then(Value::as_array))
        .ok_or_else(|| "provider model response has no model array".to_string())?;
    let mut models = Vec::new();
    for entry in entries.iter().take(MAX_PROVIDER_MODELS) {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let Some(id) = first_string(object, &["id", "model"]) else {
            continue;
        };
        if id.len() > MAX_TEXT_LENGTH || id.chars().any(|character| character.is_control()) {
            continue;
        }
        let name = first_string(object, &["name", "display_name", "displayName"])
            .filter(|value| value.len() <= MAX_TEXT_LENGTH && !value.chars().any(char::is_control))
            .unwrap_or_else(|| id.clone());
        models.push(ProviderModelSummary {
            provider_id: provider_id.to_owned(),
            id,
            name,
            context_window: first_u64(
                object,
                &[
                    "contextWindow",
                    "context_window",
                    "context_length",
                    "max_context_length",
                ],
            ),
            max_tokens: first_u64(
                object,
                &[
                    "maxTokens",
                    "max_tokens",
                    "max_output_tokens",
                    "maxOutputTokens",
                ],
            ),
            reasoning: first_bool(
                object,
                &["reasoning", "supports_reasoning", "supportsReasoning"],
            ),
            input: input_types(object),
        });
    }
    Ok(models)
}

/// Anthropic 的 Messages API 需要一个显式的版本头；缺失时上游会拒绝请求。
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 为请求附加 provider 自定义头与凭据头。
///
/// “测试连接”与实际调用（模型列表、模型探测）必须走同一套鉴权逻辑：
/// 两者分叉会让用户看到假阳性（测试通过但真实调用 401，或反之）。
/// 因此这里集中实现，任何协议细节（如 Anthropic 版本头）只改一处。
pub(crate) fn apply_provider_auth<B>(
    mut request: ureq::RequestBuilder<B>,
    provider: &ProviderRecord,
    api_key: Option<&str>,
) -> ureq::RequestBuilder<B> {
    for (name, value) in &provider.headers {
        request = request.header(name, value);
    }
    if let Some(api_key) = api_key {
        request = match provider.api.as_str() {
            "anthropic-messages" => request
                .header("x-api-key", api_key)
                .header("anthropic-version", ANTHROPIC_VERSION),
            "google-generative-ai" => request.header("x-goog-api-key", api_key),
            _ => request.header("Authorization", &format!("Bearer {api_key}")),
        };
    }
    request
}

pub(crate) fn provider_agent(provider: &ProviderRecord) -> Result<ureq::Agent, String> {
    let mut config = ureq::Agent::config_builder()
        .timeout_global(Some(PROVIDER_REQUEST_TIMEOUT))
        .http_status_as_error(false);
    if let Some(proxy) = provider.proxy.as_deref() {
        let proxy =
            ureq::Proxy::new(proxy).map_err(|error| format!("proxy is invalid: {error}"))?;
        config = config.proxy(Some(proxy));
    }
    Ok(config.build().new_agent())
}

const CHAT_MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

/// 一次「用已配置 Provider 调聊天补全」的失败分类。
///
/// 这里只携带分类与细节文本，不带 `@msg:` 码：调用方（提示词增强、站点翻译）
/// 各自把这些分类映射成自己的用户可见消息码与文案。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChatCallError {
    /// 没有任何带模型的 Pi Provider。
    ProviderUnconfigured,
    /// Provider 缺少 base URL。
    BaseUrlMissing,
    /// 请求发送失败（网络、代理、超时…）。
    Request(String),
    /// 上游返回非 2xx 状态码。
    Http(u16),
    /// 响应体读取失败。
    ResponseRead(String),
    /// 响应体不是可识别的聊天补全结构；细节是句子片段（如 `is not valid JSON: …`）。
    ResponseInvalid(String),
    /// 本地准备阶段失败：models.json、凭据、URL、代理配置。
    Local(String),
}

impl ChatCallError {
    /// 细节文本：用于日志与 `{error}` 占位符。
    pub(crate) fn detail(&self) -> String {
        match self {
            Self::ProviderUnconfigured => "no Pi provider with models is configured".into(),
            Self::BaseUrlMissing => "provider has no base URL".into(),
            Self::Request(error)
            | Self::ResponseRead(error)
            | Self::ResponseInvalid(error)
            | Self::Local(error) => error.clone(),
            Self::Http(status) => format!("HTTP {status}"),
        }
    }
}

/// 带自定义超时的 provider agent；除超时外与 [`provider_agent`] 的策略一致。
fn chat_agent(provider: &ProviderRecord, timeout: Duration) -> Result<ureq::Agent, String> {
    let mut config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false);
    if let Some(proxy) = provider.proxy.as_deref() {
        let proxy =
            ureq::Proxy::new(proxy).map_err(|error| format!("proxy is invalid: {error}"))?;
        config = config.proxy(Some(proxy));
    }
    Ok(config.build().new_agent())
}

/// 按 Provider API 类型构造聊天补全 URL。
pub(crate) fn chat_url(base_url: &str, api: &str) -> Result<String, String> {
    let base = base_url.trim().trim_end_matches('/');
    let path = if api == "anthropic-messages" {
        if base.ends_with("/v1") {
            format!("{base}/messages")
        } else {
            format!("{base}/v1/messages")
        }
    } else if api == "openai-responses" {
        format!("{base}/responses")
    } else {
        format!("{base}/chat/completions")
    };
    // 仅用于校验 URL 形状；实际请求使用字符串。
    ureq::http::Uri::try_from(path.as_str())
        .map_err(|error| format!("base URL is invalid: {error}"))?;
    Ok(path)
}

/// 按 Provider API 类型构造聊天补全请求体。
///
/// `system`/`user` 由调用方给（提示词增强、站点翻译的提示词不同）；
/// 协议差异（`system`、`instructions`、`max_tokens` 字段名）只在这里处理。
pub(crate) fn chat_request_body(
    api: &str,
    model_id: &str,
    system: &str,
    user: &str,
    max_output_tokens: u64,
) -> Value {
    if api == "anthropic-messages" {
        json!({
            "model": model_id,
            "system": system,
            "messages": [{"role": "user", "content": user}],
            "max_tokens": max_output_tokens,
        })
    } else if api == "openai-responses" {
        json!({
            "model": model_id,
            "instructions": system,
            "input": user,
            "max_output_tokens": max_output_tokens,
        })
    } else {
        json!({
            "model": model_id,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "max_tokens": max_output_tokens,
            "temperature": 0.4,
        })
    }
}

/// 从聊天补全响应体中取出模型文本（兼容 OpenAI/Anthropic 两种结构）。
///
/// 失败时返回**句子片段**（如 `is not valid JSON: …`），由调用方拼进自己的
/// 用户可见文案：既有调用方保持原文案，新调用方复用同一段解析逻辑。
pub(crate) fn parse_chat_response(body: &str) -> Result<String, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|error| format!("is not valid JSON: {error}"))?;
    if let Some(text) = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        return Ok(text);
    }
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        return Ok(text.to_owned());
    }
    if let Some(text) = value
        .get("output")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                item.get("content")
                    .and_then(Value::as_array)
                    .and_then(|content| {
                        content
                            .iter()
                            .find_map(|part| part.get("text").and_then(Value::as_str))
                    })
            })
        })
        .map(str::to_owned)
    {
        return Ok(text);
    }
    if let Some(text) = value
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find_map(|item| item.get("text").and_then(Value::as_str))
        })
        .map(str::to_owned)
    {
        return Ok(text);
    }
    Err("has no message text".into())
}

/// 用 Pi 模型配置里第一个带模型的 Provider 调一次聊天补全，返回模型输出文本。
///
/// 这是提示词增强与站点翻译共用的唯一调用通路：Provider 选择、凭据读取、
/// 鉴权头、URL 与请求体的协议差异、响应解析都只在这里实现一次。
pub(crate) fn call_configured_model(
    paths: &AppPaths,
    system: &str,
    user: &str,
    timeout: Duration,
    max_output_tokens: u64,
) -> Result<String, ChatCallError> {
    let provider = list_provider_file(&paths.pi_models_file())
        .map_err(ChatCallError::Local)?
        .into_iter()
        .find(|provider| !provider.models.is_empty())
        .ok_or(ChatCallError::ProviderUnconfigured)?;
    let model_id = provider.models[0].id.clone();
    let api = provider.models[0]
        .api
        .as_deref()
        .map(str::trim)
        .filter(|api| !api.is_empty())
        .unwrap_or(provider.api.as_str())
        .to_owned();
    let base_url = provider
        .base_url
        .as_deref()
        .ok_or(ChatCallError::BaseUrlMissing)?;
    let url = chat_url(base_url, &api).map_err(ChatCallError::Local)?;
    let api_key = provider_api_key(&provider.id).map_err(ChatCallError::Local)?;
    let agent = chat_agent(&provider, timeout).map_err(ChatCallError::Local)?;
    let body = chat_request_body(&api, &model_id, system, user, max_output_tokens);

    let request = agent
        .post(url.as_str())
        .header("Accept", "application/json");
    let request = apply_provider_auth(request, &provider, api_key.as_deref());
    let mut response = request
        .send_json(&body)
        .map_err(|error| ChatCallError::Request(error.to_string()))?;
    let status = response.status().as_u16();
    let body_text = response
        .body_mut()
        .with_config()
        .limit(CHAT_MAX_RESPONSE_BYTES)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(|error| ChatCallError::ResponseRead(error.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(ChatCallError::Http(status));
    }
    parse_chat_response(&body_text).map_err(ChatCallError::ResponseInvalid)
}

fn fetch_provider_models(provider: &ProviderRecord) -> Result<Vec<ProviderModelSummary>, String> {
    let base_url = provider
        .base_url
        .as_deref()
        .ok_or_else(|| "provider has no base URL".to_string())?;
    let url = provider_models_url(base_url, &provider.api)?;
    let api_key = provider_api_key(&provider.id)?;
    let agent = provider_agent(provider)?;
    let request = agent.get(url.as_str()).header("Accept", "application/json");
    let request = apply_provider_auth(request, provider, api_key.as_deref());
    let mut response = request
        .call()
        .map_err(|error| format!("provider model request failed: {error}"))?;
    let status = response.status().as_u16();
    let body = provider_response_body(response.body_mut())?;
    if !(200..300).contains(&status) {
        return Err(format!("provider model request failed: HTTP {status}"));
    }
    parse_provider_models(&body, &provider.id)
}

#[tauri::command]
pub async fn list_provider_models(
    app: tauri::AppHandle,
    request: ProviderModelsRequest,
) -> Result<Vec<ProviderModelSummary>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || list_provider_models_inner(app.state(), request))
        .await
        .map_err(|error| format!("provider model worker failed: {error}"))?
}

fn list_provider_models_inner(
    paths: State<'_, AppPaths>,
    request: ProviderModelsRequest,
) -> Result<Vec<ProviderModelSummary>, String> {
    validate_provider_id(&request.provider_id)?;
    let provider = list_provider_file(&paths.pi_models_file())?
        .into_iter()
        .find(|provider| provider.id == request.provider_id)
        .ok_or_else(|| format!("Pi provider not found: {}", request.provider_id))?;
    fetch_provider_models(&provider)
}

#[tauri::command]
pub async fn list_pi_providers(app: AppHandle) -> Result<Vec<ProviderRecord>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        list_provider_file(&app.state::<AppPaths>().pi_models_file())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn save_pi_provider(
    app: AppHandle,
    request: SaveProviderRequest,
) -> Result<ProviderRecord, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let gate = app.state::<ProviderConfigGate>();
        let _permit = gate
            .0
            .lock()
            .map_err(|_| "Provider configuration lock is poisoned")?;
        save_provider_file(&app.state::<AppPaths>().pi_models_file(), &request)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn delete_pi_provider(
    app: AppHandle,
    request: DeleteProviderRequest,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let gate = app.state::<ProviderConfigGate>();
        let _permit = gate
            .0
            .lock()
            .map_err(|_| "Provider configuration lock is poisoned")?;
        delete_provider_file(&app.state::<AppPaths>().pi_models_file(), &request.id)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::{
        credential_command, list_provider_file, parse_provider_models, provider_models_url,
        save_provider_file, validate_headers, validate_proxy, ConfiguredModel, SaveProviderRequest,
    };
    use serde_json::json;

    fn model() -> ConfiguredModel {
        ConfiguredModel {
            id: "gpt-5".into(),
            name: "GPT-5".into(),
            reasoning: true,
            input: vec!["text".into()],
            context_window: 200_000,
            max_tokens: 32_000,
            thinking_levels: vec!["off".into(), "low".into(), "high".into()],
            cost: None,
            api: None,
        }
    }

    #[test]
    fn parses_provider_model_catalog_and_ignores_invalid_entries() {
        let models = parse_provider_models(
            r#"{"data":[{"id":"gpt-5","name":"GPT-5","context_window":200000,"max_tokens":32000,"reasoning":true,"modalities":["text","image"]},{"id":"bad\nmodel"},{"name":"missing-id"}]}"#,
            "openai",
        )
        .expect("provider model response should parse");

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-5");
        assert_eq!(models[0].context_window, Some(200_000));
        assert_eq!(models[0].reasoning, Some(true));
        assert_eq!(models[0].input, vec!["text", "image"]);
    }

    #[test]
    fn rejects_unsafe_provider_request_options() {
        assert!(validate_proxy(Some("http://127.0.0.1:7890")).is_ok());
        assert!(validate_proxy(Some("http://proxy.example.com:8080")).is_err());
        assert!(validate_headers(&std::collections::BTreeMap::from([(
            "x-client".into(),
            "deeppi".into(),
        )]))
        .is_ok());
        assert!(validate_headers(&std::collections::BTreeMap::from([(
            "x\n-name".into(),
            "bad".into(),
        )]))
        .is_err());
    }

    #[test]
    fn appends_models_to_provider_base_url() {
        assert_eq!(
            provider_models_url("https://api.openai.com/v1", "openai-responses")
                .expect("OpenAI URL should be valid")
                .as_str(),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            provider_models_url("https://api.anthropic.com", "anthropic-messages")
                .expect("Anthropic URL should be valid")
                .as_str(),
            "https://api.anthropic.com/v1/models"
        );
    }

    #[test]
    fn reads_pi_models_with_default_metadata() {
        let root =
            std::env::temp_dir().join(format!("deeppi-minimal-models-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("test directory should exist");
        let path = root.join("models.json");
        std::fs::write(
            &path,
            br#"{"providers":{"local":{"api":"openai-completions","models":[{"id":"llama"}]}}}"#,
        )
        .expect("fixture should be written");

        let providers = list_provider_file(&path).expect("minimal model should be readable");
        assert_eq!(providers[0].models[0].name, "llama");
        assert_eq!(providers[0].models[0].input, vec!["text"]);
        assert_eq!(providers[0].models[0].context_window, 128_000);

        std::fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[test]
    fn saves_provider_and_preserves_unknown_fields() {
        let root = std::env::temp_dir().join(format!("deeppi-models-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("test directory should exist");
        let path = root.join("models.json");
        let fixture = serde_json::to_vec_pretty(&json!({
            "providers": {"openai": {"custom": true}},
            "unknownTopLevel": {"kept": true}
        }))
        .expect("fixture should serialize");
        std::fs::write(&path, fixture).expect("fixture should be written");

        save_provider_file(
            &path,
            &SaveProviderRequest {
                id: "openai".into(),
                name: "OpenAI".into(),
                api: "openai-responses".into(),
                base_url: Some("https://api.openai.com/v1".into()),
                headers: std::collections::BTreeMap::from([("x-client".into(), "deeppi".into())]),
                proxy: Some("http://127.0.0.1:7890".into()),
                models: vec![model()],
            },
        )
        .expect("provider should save");

        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("file should read"))
                .expect("file should parse");
        assert_eq!(value["unknownTopLevel"]["kept"], true);
        assert_eq!(value["providers"]["openai"]["custom"], true);
        assert_eq!(
            value["providers"]["openai"]["headers"]["x-client"],
            "deeppi"
        );
        assert_eq!(
            value["providers"]["openai"]["proxy"],
            "http://127.0.0.1:7890"
        );
        let command = credential_command("openai").expect("credential helper path should resolve");
        assert!(value["providers"]["openai"]["apiKey"]
            .as_str()
            .is_some_and(|api_key| api_key == command));
        assert_eq!(
            list_provider_file(&path)
                .expect("provider should list")
                .len(),
            1
        );

        std::fs::remove_dir_all(root).expect("test directory should be removed");
    }
}
