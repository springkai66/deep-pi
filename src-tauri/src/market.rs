use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::message::msg;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{State, Url};

const MAX_QUERY_LENGTH: usize = 100;
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
/// models.dev 全量目录约 4.5 MB，读取上限独立于 `MAX_RESPONSE_BYTES`（其它命令仍用 4 MiB）。
const MAX_MODELS_RESPONSE_BYTES: u64 = 12 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// 全量目录（约 4.5 MB）下载可能超过默认请求超时。
const MODELS_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const CACHE_TTL: Duration = Duration::from_secs(600);
const PI_PACKAGES_URL: &str = "https://pi.dev/packages";
const MAX_PACKAGE_QUERY_LENGTH: usize = 214;
const PI_PACKAGES_PAGE_SIZE: usize = 50;
/// pi.dev 模型目录页（HTML 抓取已被 models.dev `api.json` 取代，仅为兼容保留）。
#[allow(dead_code)]
const PI_MODELS_URL: &str = "https://pi.dev/models";
/// models.dev 全量模型目录（结构化 JSON）。
const MODELS_DEV_URL: &str = "https://models.dev/api.json";
/// models.dev 站点链接前缀：模型页形如 `https://models.dev/<providerKey>/<modelKey>`。
const MODELS_DEV_SITE_URL: &str = "https://models.dev";
const NPM_REGISTRY_URL: &str = "https://registry.npmjs.org";
const MCP_REGISTRY_URL: &str = "https://registry.modelcontextprotocol.io/v0/servers";
const MAX_MCP_RESULTS: usize = 24;
/// 模型搜索结果上限（沿用原先 HTML 抓取的语义）。
const MAX_MODEL_RESULTS: usize = 200;
/// 模型目录缓存 key 固定：缓存 `api.json` 全量 JSON，查询时本地过滤。
const MODELS_CATALOG_CACHE_KEY: &str = MODELS_DEV_URL;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiPackage {
    pub name: String,
    pub description: String,
    pub types: Vec<String>,
    pub downloads: u64,
    pub published_at: i64,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiPackagePage {
    pub packages: Vec<PiPackage>,
    pub next_page: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageMetadata {
    pub version: String,
    pub license: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiModelSummary {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub path: String,
    pub context_window: Option<u64>,
    pub input_cost: Option<f64>,
    pub output_cost: Option<f64>,
    pub cache_read_cost: Option<f64>,
    pub cache_write_cost: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    pub input: Option<f64>,
    pub output: Option<f64>,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiModelProfile {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub api: Option<String>,
    pub base_url: Option<String>,
    pub reasoning: bool,
    pub input: Vec<String>,
    pub context_window: Option<u64>,
    pub max_tokens: Option<u64>,
    pub thinking_levels: Vec<String>,
    pub cost: Option<ModelCost>,
}

#[derive(Default)]
pub struct MarketCache {
    entries: Mutex<HashMap<String, (Instant, PiPackagePage)>>,
}

impl MarketCache {
    fn get(&self, query: &str) -> Option<PiPackagePage> {
        let entries = self.entries.lock().ok()?;
        let (created, page) = entries.get(query)?;
        (created.elapsed() < CACHE_TTL).then(|| page.clone())
    }

    fn insert(&self, query: String, page: PiPackagePage) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|_, (created, _)| created.elapsed() < CACHE_TTL);
            entries.insert(query, (Instant::now(), page));
        }
    }
}

/// models.dev `api.json` 全量响应缓存（key 固定，过滤在本地做，避免每个关键词都下载 4.5 MB）。
#[derive(Default)]
pub struct ModelCatalogCache {
    entries: Mutex<HashMap<String, (Instant, String)>>,
}

impl ModelCatalogCache {
    fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.lock().ok()?;
        let (created, body) = entries.get(key)?;
        (created.elapsed() < CACHE_TTL).then(|| body.clone())
    }

    fn insert(&self, key: String, body: String) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|_, (created, _)| created.elapsed() < CACHE_TTL);
            entries.insert(key, (Instant::now(), body));
        }
    }
}

#[derive(Default)]
pub struct McpRegistryCache {
    entries: Mutex<HashMap<String, (Instant, Vec<McpRegistryEntry>)>>,
}

impl McpRegistryCache {
    fn get(&self, key: &str) -> Option<Vec<McpRegistryEntry>> {
        let entries = self.entries.lock().ok()?;
        let (created, models) = entries.get(key)?;
        (created.elapsed() < CACHE_TTL).then(|| models.clone())
    }

    fn insert(&self, key: String, models: Vec<McpRegistryEntry>) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|_, (created, _)| created.elapsed() < CACHE_TTL);
            entries.insert(key, (Instant::now(), models));
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryEntry {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub repository: Option<String>,
    pub remote_url: Option<String>,
    pub package: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryRequest {
    query: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRequest {
    query: String,
    #[serde(default = "first_package_page")]
    page: u32,
}

fn first_package_page() -> u32 {
    1
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageMetadataRequest {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSearchRequest {
    query: String,
    provider: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfileRequest {
    path: String,
    provider: String,
    model_id: String,
}

fn http_agent_with_timeout(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .https_only(true)
        .proxy(crate::proxy::ureq_proxy())
        .build()
        .new_agent()
}

fn read_response_with_limit(body: &mut ureq::Body, limit: u64) -> Result<String, String> {
    body.with_config()
        .limit(limit)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(|error| format!("failed to read market response: {error}"))
}

pub(crate) fn http_agent() -> ureq::Agent {
    http_agent_with_timeout(REQUEST_TIMEOUT)
}

pub(crate) fn read_response(body: &mut ureq::Body) -> Result<String, String> {
    read_response_with_limit(body, MAX_RESPONSE_BYTES)
}

fn html_attribute(tag: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=\"");
    let start = tag.find(&prefix)? + prefix.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_owned())
}

pub(crate) fn decode_entities(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find(';').map(|offset| start + offset) else {
            out.push_str(&rest[start..]);
            return out;
        };
        let entity = &rest[start + 1..end];
        match decode_entity(entity) {
            Some(character) => out.push(character),
            None => out.push_str(&rest[start..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// 单个实体（`&` 与 `;` 之间的内容）→ 字符；未知实体返回 `None`（原样保留）。
fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        numeric => {
            let code = numeric
                .strip_prefix("#x")
                .or_else(|| numeric.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| {
                    numeric
                        .strip_prefix('#')
                        .and_then(|digits| digits.parse().ok())
                })?;
            char::from_u32(code)
        }
    }
}

pub(crate) fn strip_tags(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    decode_entities(&text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn model_cell(body: &str, label: &str) -> Option<String> {
    let marker = format!("data-label=\"{label}\"");
    let marker_start = body.find(&marker)?;
    let content_start = body[marker_start..].find('>')? + marker_start + 1;
    let content_end = body[content_start..].find('<')? + content_start;
    Some(strip_tags(&body[content_start..content_end]))
}

fn first_anchor_text(body: &str) -> Option<String> {
    let anchor_start = body.find("<a")?;
    let content_start = body[anchor_start..].find('>')? + anchor_start + 1;
    let content_end = body[content_start..].find("</a>")? + content_start;
    Some(strip_tags(&body[content_start..content_end]))
}

fn parse_decimal(value: Option<String>) -> Option<f64> {
    value?.replace(['$', ','], "").parse::<f64>().ok()
}

fn parse_integer(value: Option<String>) -> Option<u64> {
    value?.replace(',', "").parse::<u64>().ok()
}

fn models_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

/// models.dev 的 `cost` 对象 → `ModelCost`（`cost` 缺失或非对象时为 `None`，字段缺失或非数字时为 `None`）。
fn models_cost(cost: Option<&Value>) -> Option<ModelCost> {
    let cost = cost.and_then(Value::as_object)?;
    Some(ModelCost {
        input: models_number(cost.get("input")),
        output: models_number(cost.get("output")),
        cache_read: models_number(cost.get("cache_read")),
        cache_write: models_number(cost.get("cache_write")),
    })
}

/// `modalities.<key>` → 字符串数组（缺失或非数组时为空）。
fn models_modalities(model: &Value, key: &str) -> Vec<String> {
    model
        .get("modalities")
        .and_then(|modalities| modalities.get(key))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// `limit.context` / `limit.output`；非整数数字（如 `1000000.0`）也能取整。
fn models_limit(model: &Value, key: &str) -> Option<u64> {
    let value = model.get("limit")?.get(key)?;
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|value| value as u64)
    })
}

/// models.dev `api.json` 顶层归一化：对象时以键为 provider key，数组时用元素 `id`。
/// 顶层既不是对象也不是数组时返回空列表（表现为空结果，不报错）。
fn models_dev_providers(value: &Value) -> Vec<(String, &Value)> {
    match value {
        Value::Object(root) => root
            .iter()
            .filter(|(_, provider)| provider.is_object())
            .map(|(key, provider)| (key.clone(), provider))
            .collect(),
        Value::Array(root) => root
            .iter()
            .enumerate()
            .map(|(index, provider)| {
                let key = provider
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("provider-{index}"));
                (key, provider)
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// 供应商的 `models` 字段 → `(modelKey, modelObject)`；`models` 非对象或非数组时为空。
fn models_dev_models(provider: &Value) -> Vec<(String, &Value)> {
    if let Some(models) = provider.get("models").and_then(Value::as_object) {
        return models
            .iter()
            .filter(|(_, model)| model.is_object())
            .map(|(key, model)| (key.clone(), model))
            .collect();
    }
    provider
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .enumerate()
                .filter(|(_, model)| model.is_object())
                .map(|(index, model)| (index.to_string(), model))
                .collect()
        })
        .unwrap_or_default()
}

/// 模型 `id`；缺失或为空时用 `<providerKey>/<modelKey>` 拼。
fn models_dev_model_id(provider_key: &str, model_key: &str, model: &Value) -> String {
    json_string(model.get("id"))
        .map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| format!("{provider_key}/{model_key}"))
}

fn models_dev_summary(provider_key: &str, model_key: &str, model: &Value) -> PiModelSummary {
    let id = models_dev_model_id(provider_key, model_key, model);
    let name = json_string(model.get("name"))
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| id.clone());
    let cost = models_cost(model.get("cost"));
    PiModelSummary {
        provider: provider_key.to_owned(),
        id,
        name,
        path: format!("{MODELS_DEV_SITE_URL}/{provider_key}/{model_key}"),
        context_window: models_limit(model, "context"),
        input_cost: cost.as_ref().and_then(|cost| cost.input),
        output_cost: cost.as_ref().and_then(|cost| cost.output),
        cache_read_cost: cost.as_ref().and_then(|cost| cost.cache_read),
        cache_write_cost: cost.as_ref().and_then(|cost| cost.cache_write),
    }
}

/// 结果稳定排序：provider → name → id。
fn models_dev_sort(models: &mut [PiModelSummary]) {
    models.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
}

/// 解析 models.dev `api.json`（全量目录）并本地过滤，语义沿用原 pi.dev 抓取：
/// `query` 为空返回全部，非空时对 `provider`/`id`/`name` 大小写不敏感匹配；
/// `provider_filter` 按 provider 精确（大小写不敏感）过滤；结果按 provider → name 排序，
/// 上限 `MAX_MODEL_RESULTS` 条。
pub fn parse_models_dev_catalog(
    body: &str,
    query: &str,
    provider_filter: Option<&str>,
) -> Vec<PiModelSummary> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let query = query.trim().to_ascii_lowercase();
    let provider_filter = provider_filter
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    let mut models = Vec::new();
    for (provider_key, provider) in models_dev_providers(&value) {
        if provider_filter
            .as_ref()
            .is_some_and(|filter| !provider_key.to_ascii_lowercase().eq(filter))
        {
            continue;
        }
        for (model_key, model) in models_dev_models(provider) {
            let summary = models_dev_summary(&provider_key, &model_key, model);
            if !query.is_empty() {
                let searchable = format!("{} {} {}", summary.provider, summary.id, summary.name)
                    .to_ascii_lowercase();
                if !searchable.contains(&query) {
                    continue;
                }
            }
            models.push(summary);
        }
    }
    models_dev_sort(&mut models);
    models.truncate(MAX_MODEL_RESULTS);
    models
}
/// 从 models.dev `api.json` 取单个模型的完整信息（`pi_model_profile` 命令）。
pub fn parse_models_dev_profile(
    body: &str,
    provider: &str,
    model_id: &str,
) -> Result<PiModelProfile, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("models.dev catalog is invalid: {error}"))?;
    let providers = models_dev_providers(&value);
    let (provider_key, provider_value) = providers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(provider))
        .ok_or_else(|| format!("models.dev provider not found: {provider}"))?;
    let model = models_dev_models(provider_value)
        .into_iter()
        .find(|(model_key, model)| {
            let id = models_dev_model_id(provider_key, model_key, model);
            id.eq_ignore_ascii_case(model_id) || model_key.eq_ignore_ascii_case(model_id)
        })
        .map(|(_, model)| model)
        .ok_or_else(|| format!("models.dev model not found: {provider}/{model_id}"))?;
    let reasoning = model
        .get("reasoning")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // models.dev 没有 `thinkingLevelMap`；按 reasoning 推导，与前端
    // `PiProviderSettings.svelte:509` 的既有约定一致。
    let thinking_levels = if reasoning {
        ["off", "minimal", "low", "medium", "high"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        Vec::new()
    };
    Ok(PiModelProfile {
        provider: provider.to_owned(),
        id: model_id.to_owned(),
        name: json_string(model.get("name"))
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| model_id.to_owned()),
        api: json_string(model.get("api")).or_else(|| {
            model
                .get("provider")
                .and_then(|provider| json_string(provider.get("api")))
        }),
        // models.dev 供应商级 `api` 字段是 base URL。
        base_url: json_string(provider_value.get("api")),
        reasoning,
        input: models_modalities(model, "input"),
        context_window: models_limit(model, "context"),
        max_tokens: models_limit(model, "output"),
        thinking_levels,
        cost: models_cost(model.get("cost")),
    })
}

/// pi.dev 模型行（`data-model-row`）解析；搜索主路径已改为 models.dev `api.json`，仅为兼容保留。
#[allow(dead_code)]
pub fn parse_pi_models(
    html: &str,
    query: &str,
    provider_filter: Option<&str>,
) -> Vec<PiModelSummary> {
    let query = query.trim().to_ascii_lowercase();
    let provider_filter = provider_filter
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    let mut models = Vec::new();
    let mut cursor = 0;

    while models.len() < 200 {
        let Some(relative_start) = html[cursor..].find("<tr") else {
            break;
        };
        let start = cursor + relative_start;
        let Some(relative_open_end) = html[start..].find('>') else {
            break;
        };
        let open_end = start + relative_open_end + 1;
        let opening = &html[start..open_end];
        let Some(relative_close) = html[open_end..].find("</tr>") else {
            break;
        };
        let close = open_end + relative_close;
        cursor = close + "</tr>".len();

        if html_attribute(opening, "data-model-row").as_deref() != Some("true") {
            continue;
        }
        let Some(provider) = html_attribute(opening, "data-model-provider") else {
            continue;
        };
        let Some(id) = html_attribute(opening, "data-model-id") else {
            continue;
        };
        let name = first_anchor_text(&html[open_end..close])
            .filter(|value| !value.is_empty())
            .or_else(|| html_attribute(opening, "data-model-name"))
            .unwrap_or_else(|| id.clone());
        let searchable = format!("{provider} {id} {name}").to_ascii_lowercase();
        if provider_filter
            .as_ref()
            .is_some_and(|filter| !provider.to_ascii_lowercase().eq(filter))
            || (!query.is_empty() && !searchable.contains(&query))
        {
            continue;
        }

        let body = &html[open_end..close];
        let path = html_attribute(body, "data-model-path")
            .unwrap_or_else(|| format!("/models/{provider}/{id}"));
        models.push(PiModelSummary {
            provider,
            id,
            name,
            path,
            context_window: parse_integer(model_cell(body, "Context")),
            input_cost: parse_decimal(model_cell(body, "Input /M")),
            output_cost: parse_decimal(model_cell(body, "Output /M")),
            cache_read_cost: parse_decimal(model_cell(body, "Cache read /M")),
            cache_write_cost: parse_decimal(model_cell(body, "Cache write /M")),
        });
    }
    models
}

/// pi.dev 原始配置块解析（HTML 抓取已弃用，仅为兼容与回归测试保留）。
#[allow(dead_code)]
fn raw_model_configuration(html: &str) -> Result<Value, String> {
    let start = html
        .find("<pre class=\"raw-data-panel\">")
        .ok_or_else(|| "Pi model detail has no configuration block".to_string())?;
    let content_start = html[start..]
        .find('>')
        .map(|offset| start + offset + 1)
        .ok_or_else(|| "Pi model configuration is malformed".to_string())?;
    let content_end = html[content_start..]
        .find("</pre>")
        .map(|offset| content_start + offset)
        .ok_or_else(|| "Pi model configuration is malformed".to_string())?;
    let json = strip_tags(&html[content_start..content_end]);
    serde_json::from_str(&json)
        .map_err(|error| format!("Pi model configuration is invalid: {error}"))
}

fn json_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_owned)
}

fn json_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

#[allow(dead_code)]
fn thinking_level_rank(level: &str) -> (usize, &str) {
    let rank = match level {
        "off" => 0,
        "minimal" => 1,
        "low" => 2,
        "medium" => 3,
        "high" => 4,
        "xhigh" => 5,
        _ => 6,
    };
    (rank, level)
}

/// pi.dev 详情页配置解析（HTML 抓取已弃用，仅为兼容与回归测试保留）。
#[allow(dead_code)]
pub fn parse_model_profile(
    html: &str,
    provider: &str,
    model_id: &str,
) -> Result<PiModelProfile, String> {
    let configuration = raw_model_configuration(html)?;
    let provider_config = configuration
        .get("providers")
        .and_then(|providers| providers.get(provider))
        .ok_or_else(|| format!("Pi model provider not found: {provider}"))?;
    let model = provider_config
        .get("models")
        .and_then(Value::as_array)
        .and_then(|models| {
            models
                .iter()
                .find(|model| model.get("id").and_then(Value::as_str) == Some(model_id))
        })
        .ok_or_else(|| format!("Pi model not found: {provider}/{model_id}"))?;
    let reasoning = model
        .get("reasoning")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut thinking_levels = model
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
    if reasoning && thinking_levels.is_empty() {
        thinking_levels = ["off", "minimal", "low", "medium", "high"]
            .into_iter()
            .map(str::to_owned)
            .collect();
    }
    thinking_levels
        .sort_by(|left, right| thinking_level_rank(left).cmp(&thinking_level_rank(right)));

    let cost = model
        .get("cost")
        .and_then(Value::as_object)
        .map(|cost| ModelCost {
            input: cost.get("input").and_then(Value::as_f64),
            output: cost.get("output").and_then(Value::as_f64),
            cache_read: cost.get("cacheRead").and_then(Value::as_f64),
            cache_write: cost.get("cacheWrite").and_then(Value::as_f64),
        });
    let input = model
        .get("input")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    Ok(PiModelProfile {
        provider: provider.to_owned(),
        id: model_id.to_owned(),
        name: json_string(model.get("name")).unwrap_or_else(|| model_id.to_owned()),
        api: json_string(model.get("api")).or_else(|| json_string(provider_config.get("api"))),
        base_url: json_string(provider_config.get("baseUrl")),
        reasoning,
        input,
        context_window: json_u64(model.get("contextWindow")),
        max_tokens: json_u64(model.get("maxTokens")),
        thinking_levels,
        cost,
    })
}

fn package_catalog_error(detail: &str) -> String {
    format!("Pi package catalog HTML is invalid: {detail}")
}

/// 仅扫描目录使用的、非嵌套的同名元素；缺闭合标签不能成为空结果。
fn catalog_elements<'a>(html: &'a str, tag: &str) -> Result<Vec<(&'a str, &'a str)>, String> {
    let marker = format!("<{tag}");
    let closing = format!("</{tag}>");
    let mut rest = html;
    let mut elements = Vec::new();
    while let Some(start) = rest.find(&marker) {
        rest = &rest[start..];
        if !rest[marker.len()..]
            .starts_with(|character: char| character.is_ascii_whitespace() || character == '>')
        {
            rest = &rest[marker.len()..];
            continue;
        }
        let open_end = rest
            .find('>')
            .ok_or_else(|| package_catalog_error("unterminated opening tag"))?
            + 1;
        let close = rest[open_end..]
            .find(&closing)
            .map(|offset| open_end + offset)
            .ok_or_else(|| package_catalog_error("missing closing tag"))?;
        let content = &rest[open_end..close];
        if content.match_indices(&marker).any(|(start, _)| {
            content[start + marker.len()..]
                .starts_with(|character: char| character.is_ascii_whitespace() || character == '>')
        }) {
            return Err(package_catalog_error("nested or unclosed catalog element"));
        }
        elements.push((&rest[..open_end], content));
        rest = &rest[close + closing.len()..];
    }
    Ok(elements)
}

fn catalog_has_class(opening: &str, class: &str) -> bool {
    html_attribute(opening, "class")
        .is_some_and(|classes| classes.split_whitespace().any(|value| value == class))
}

fn parse_catalog_package(opening: &str, body: &str) -> Result<PiPackage, String> {
    let headings = catalog_elements(body, "h3")?;
    let (_, heading) = headings
        .iter()
        .find(|(opening, _)| catalog_has_class(opening, "packages-name"))
        .ok_or_else(|| package_catalog_error("package heading is missing"))?;
    let anchors = catalog_elements(heading, "a")?;
    let (anchor, label) = anchors
        .first()
        .ok_or_else(|| package_catalog_error("package link is missing"))?;
    let name = strip_tags(label);
    validate_package_name(&name).map_err(|_| package_catalog_error("package name is invalid"))?;
    let href = html_attribute(anchor, "href")
        .map(|value| decode_entities(&value))
        .ok_or_else(|| package_catalog_error("package link has no href"))?;
    let path = href.split(['?', '#']).next().unwrap_or_default();
    if path != format!("/packages/{name}")
        || html_attribute(opening, "data-package-name")
            .is_some_and(|value| decode_entities(&value) != name)
    {
        return Err(package_catalog_error("package name and link disagree"));
    }
    let descriptions = catalog_elements(body, "p")?;
    let description = descriptions
        .iter()
        .find(|(opening, _)| catalog_has_class(opening, "packages-desc"))
        .map(|(_, content)| strip_tags(content))
        .ok_or_else(|| package_catalog_error("package description is missing"))?;
    // 类型来自官网，不按 npm 关键词推断；空类型仍是目录中的有效包。
    let types = html_attribute(opening, "data-package-types")
        .ok_or_else(|| package_catalog_error("package types are missing"))?
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let downloads = html_attribute(opening, "data-package-downloads")
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| package_catalog_error("package downloads are invalid"))?;
    // 官网 data-package-date 为 Unix 毫秒，与 UI 的 Intl.DateTimeFormat 约定一致。
    let published_at = html_attribute(opening, "data-package-date")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value >= 0)
        .ok_or_else(|| package_catalog_error("package publication date is invalid"))?;
    Ok(PiPackage {
        name,
        description,
        types,
        downloads,
        published_at,
        path: path.to_owned(),
    })
}

/// 只读取 All packages 区域，保持官网排序；近期发布侧栏不是查询结果。
pub fn parse_pi_packages(html: &str, current_page: u32) -> Result<PiPackagePage, String> {
    let sections = catalog_elements(html, "section")?;
    let indexes: Vec<_> = sections
        .iter()
        .filter(|(opening, _)| catalog_has_class(opening, "packages-index-card"))
        .collect();
    if indexes.len() != 1 || current_page == 0 {
        return Err(package_catalog_error(
            "package index is missing or ambiguous",
        ));
    }
    let (_, index) = indexes[0];
    let packages = catalog_elements(index, "article")?
        .into_iter()
        .map(|(opening, body)| parse_catalog_package(opening, body))
        .collect::<Result<Vec<_>, _>>()?;
    let empty = catalog_elements(index, "p")?
        .iter()
        .any(|(opening, content)| {
            catalog_has_class(opening, "packages-empty")
                && strip_tags(content) == "No packages match this filter."
        });
    if packages.is_empty() != empty || packages.len() > PI_PACKAGES_PAGE_SIZE {
        return Err(package_catalog_error(
            "package results are missing or inconsistent",
        ));
    }
    let base = Url::parse(PI_PACKAGES_URL).expect("constant Pi catalog URL");
    let mut next_page = None;
    for (opening, content) in catalog_elements(index, "a")? {
        if !catalog_has_class(opening, "pagination-link") || strip_tags(content) != "Next →" {
            continue;
        }
        let href = html_attribute(opening, "href")
            .ok_or_else(|| package_catalog_error("next page link is missing"))?;
        let url = base
            .join(&decode_entities(&href))
            .map_err(|_| package_catalog_error("next page URL is invalid"))?;
        let pages: Vec<_> = url.query_pairs().filter(|(key, _)| key == "page").collect();
        let page = pages
            .first()
            .and_then(|(_, value)| value.parse::<u32>().ok());
        if url.origin() != base.origin()
            || url.path() != base.path()
            || pages.len() != 1
            || page.is_none()
            || page != current_page.checked_add(1)
            || next_page.is_some()
            || empty
        {
            return Err(package_catalog_error("next page link is inconsistent"));
        }
        next_page = page;
    }
    Ok(PiPackagePage {
        packages,
        next_page,
    })
}

pub fn validate_package_name(name: &str) -> Result<&str, String> {
    if name.is_empty() || name.len() > 214 || name.starts_with('.') || name.starts_with('-') {
        return Err("invalid npm package name".into());
    }
    if name.chars().any(|character| {
        character.is_control()
            || character.is_whitespace()
            || !character.is_ascii()
            || !matches!(character, '@' | '/' | '-' | '_' | '.' | '0'..='9' | 'a'..='z' | 'A'..='Z')
    }) {
        return Err("invalid npm package name".into());
    }
    if name.starts_with('@') {
        let Some((scope, package)) = name.split_once('/') else {
            return Err("scoped npm package must contain a package name".into());
        };
        if scope.len() < 2 || package.is_empty() || package.contains('/') {
            return Err("invalid scoped npm package name".into());
        }
    } else if name.contains('/') {
        return Err("unscoped npm package cannot contain a slash".into());
    }
    Ok(name)
}

fn pi_package_search_url(query: &str, page: u32) -> Result<Url, String> {
    if query.len() > MAX_PACKAGE_QUERY_LENGTH || query.chars().any(char::is_control) || page == 0 {
        return Err("package search query or page is invalid".into());
    }
    let mut url = Url::parse(PI_PACKAGES_URL).expect("constant Pi catalog URL");
    url.query_pairs_mut()
        .append_pair("name", query.trim())
        .append_pair("sort", "downloads")
        .append_pair("page", &page.to_string());
    Ok(url)
}

fn fetch_pi_package_page(url: &Url, page: u32) -> Result<PiPackagePage, String> {
    let body = crate::retry::retry_network(|_attempt| {
        let mut response = http_agent()
            .get(url.as_str())
            .header("Accept", "text/html")
            .call()
            .map_err(|error| crate::retry::AttemptFailure {
                message: format!("Pi package catalog request failed: {error}"),
                retryable: crate::retry::is_retryable_ureq_error(&error),
                status: crate::retry::status_of_ureq_error(&error),
                retry_after: None,
            })?;
        read_response(response.body_mut()).map_err(crate::retry::AttemptFailure::retryable)
    })
    .map_err(|failure| failure.message)?;
    parse_pi_packages(&body, page)
}

fn registry_string(object: &Value, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub fn parse_mcp_registry(body: &str) -> Vec<McpRegistryEntry> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let Some(servers) = value.get("servers").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for wrapper in servers {
        if entries.len() >= MAX_MCP_RESULTS {
            break;
        }
        let Some(server) = wrapper.get("server") else {
            continue;
        };
        let Some(name) = registry_string(server, "name") else {
            continue;
        };
        if name.len() > 200 {
            continue;
        }
        let repository = server
            .get("repository")
            .and_then(|repo| registry_string(repo, "url"));
        let remote_url = server
            .get("remotes")
            .and_then(Value::as_array)
            .and_then(|remotes| {
                remotes.iter().find_map(|remote| {
                    remote
                        .get("url")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                })
            });
        let package = server
            .get("packages")
            .and_then(Value::as_array)
            .and_then(|packages| {
                packages.iter().find_map(|package| {
                    let identifier = registry_string(package, "identifier")?;
                    (identifier.len() <= 214).then_some(identifier)
                })
            });
        entries.push(McpRegistryEntry {
            name,
            description: registry_string(server, "description").unwrap_or_default(),
            version: registry_string(server, "version"),
            repository,
            remote_url,
            package,
        });
    }
    entries
}

fn fetch_mcp_registry(query: &str) -> Result<Vec<McpRegistryEntry>, String> {
    let query = query.trim();
    if query.len() > MAX_QUERY_LENGTH || query.chars().any(char::is_control) {
        return Err(msg("market.mcp_query_invalid"));
    }
    let mut url = Url::parse(MCP_REGISTRY_URL)
        .map_err(|error| format!("invalid MCP registry URL: {error}"))?;
    if !query.is_empty() {
        url.query_pairs_mut().append_pair("search", query);
    }
    url.query_pairs_mut()
        .append_pair("limit", &MAX_MCP_RESULTS.to_string());
    let body = crate::retry::retry_network(|_attempt| {
        let mut response = http_agent().get(url.as_str()).call().map_err(|error| {
            crate::retry::AttemptFailure {
                message: format!("MCP registry request failed: {error}"),
                retryable: crate::retry::is_retryable_ureq_error(&error),
                status: crate::retry::status_of_ureq_error(&error),
                retry_after: None,
            }
        })?;
        read_response(response.body_mut()).map_err(crate::retry::AttemptFailure::retryable)
    })
    .map_err(|failure| failure.message)?;
    Ok(parse_mcp_registry(&body))
}

#[tauri::command]
pub async fn search_mcp_registry(
    app: tauri::AppHandle,
    request: McpRegistryRequest,
) -> Result<Vec<McpRegistryEntry>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let cache = app.state::<McpRegistryCache>();
        let query = request.query.trim().to_owned();
        if let Some(entries) = cache.get(&query) {
            return Ok(entries);
        }
        let entries = fetch_mcp_registry(&query)?;
        cache.insert(query, entries.clone());
        Ok(entries)
    })
    .await
    .map_err(|error| format!("MCP registry worker failed: {error}"))?
}

pub fn fetch_package_metadata(name: &str) -> Result<PackageMetadata, String> {
    validate_package_name(name)?;
    let encoded = name.replace('/', "%2f");
    let url = format!("{NPM_REGISTRY_URL}/{encoded}");
    let body =
        crate::retry::retry_network(|_attempt| {
            let mut response = http_agent().get(url.clone()).call().map_err(|error| {
                crate::retry::AttemptFailure {
                    message: format!("npm metadata request failed: {error}"),
                    retryable: crate::retry::is_retryable_ureq_error(&error),
                    status: crate::retry::status_of_ureq_error(&error),
                    retry_after: None,
                }
            })?;
            read_response(response.body_mut()).map_err(crate::retry::AttemptFailure::retryable)
        })
        .map_err(|failure| failure.message)?;
    let value: Value =
        serde_json::from_str(&body).map_err(|error| format!("npm metadata is invalid: {error}"))?;
    let version = value
        .get("dist-tags")
        .and_then(|tags| tags.get("latest"))
        .and_then(Value::as_str)
        .filter(|version| is_valid_version(version))
        .ok_or_else(|| "npm metadata has no valid latest version".to_string())?;
    let license = value
        .get("versions")
        .and_then(|versions| versions.get(version))
        .and_then(|latest| latest.get("license"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(PackageMetadata {
        version: version.to_owned(),
        license,
    })
}

fn is_valid_version(version: &str) -> bool {
    let mut parts = version.splitn(2, '-');
    let numbers = parts.next().unwrap_or_default().split('.');
    numbers.clone().count() == 3
        && numbers.into_iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
        && parts.next().is_none_or(|suffix| {
            !suffix.is_empty()
                && suffix.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
                })
        })
}

/// provider / model id 身份校验（大小写与字符检查），不做网络请求。
fn validate_model_identity<'a>(
    provider: &'a str,
    model_id: &'a str,
) -> Result<(&'a str, &'a str), String> {
    let provider = provider.trim();
    let model_id = model_id.trim();
    if provider.is_empty()
        || model_id.is_empty()
        || provider.len() > MAX_QUERY_LENGTH
        || model_id.len() > MAX_QUERY_LENGTH
        || provider.chars().any(char::is_control)
        || model_id.chars().any(char::is_control)
    {
        return Err("Pi model identity is invalid".into());
    }
    Ok((provider, model_id))
}

/// `path` 仅作身份检查（数据改由 models.dev 目录提供）：兼容 models.dev 站点链接
/// 与旧的 `/models/<provider>/<id>` 形态。
fn validate_model_path(path: &str) -> Result<&str, String> {
    let path = path.trim();
    Ok(path)
}

/// 从 models.dev 拉取 `api.json` 全量目录（可选进程内缓存，key 固定）。
fn fetch_models_dev_catalog(cache: Option<&ModelCatalogCache>) -> Result<String, String> {
    if let Some(cache) = cache {
        if let Some(body) = cache.get(MODELS_CATALOG_CACHE_KEY) {
            return Ok(body);
        }
    }
    let body = crate::retry::retry_network(|_attempt| {
        let mut response = http_agent_with_timeout(MODELS_REQUEST_TIMEOUT)
            .get(MODELS_DEV_URL)
            .call()
            .map_err(|error| crate::retry::AttemptFailure {
                message: format!("models.dev catalog request failed: {error}"),
                retryable: crate::retry::is_retryable_ureq_error(&error),
                status: crate::retry::status_of_ureq_error(&error),
                retry_after: None,
            })?;
        read_response_with_limit(response.body_mut(), MAX_MODELS_RESPONSE_BYTES).map_err(|error| {
            crate::retry::AttemptFailure {
                message: format!("models.dev catalog request failed: {error}"),
                retryable: true,
                status: None,
                retry_after: None,
            }
        })
    })
    .map_err(|failure| failure.message)?;
    if let Some(cache) = cache {
        cache.insert(MODELS_CATALOG_CACHE_KEY.to_owned(), body.clone());
    }
    Ok(body)
}

/// 搜索 Pi 模型（数据源：models.dev `api.json`）；无缓存直连版本，供测试/脚本使用，
/// 命令路径走带缓存的 `search_pi_models_inner`。
#[allow(dead_code)]
pub fn fetch_pi_models(
    query: &str,
    provider_filter: Option<&str>,
) -> Result<Vec<PiModelSummary>, String> {
    let query = query.trim();
    if query.len() > MAX_QUERY_LENGTH || query.chars().any(char::is_control) {
        return Err("model search query is invalid".into());
    }
    if provider_filter.is_some_and(|provider| {
        provider.len() > MAX_QUERY_LENGTH
            || provider
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
    }) {
        return Err("model provider filter is invalid".into());
    }
    let body = fetch_models_dev_catalog(None)?;
    Ok(parse_models_dev_catalog(&body, query, provider_filter))
}

/// 单个模型详情（数据源：models.dev `api.json`）；带目录缓存的版本。
fn fetch_model_profile_cached(
    cache: &ModelCatalogCache,
    path: &str,
    provider: &str,
    model_id: &str,
) -> Result<PiModelProfile, String> {
    let (provider, model_id) = validate_model_identity(provider, model_id)?;
    let _ = validate_model_path(path)?;
    let body = fetch_models_dev_catalog(Some(cache))?;
    parse_models_dev_profile(&body, provider, model_id)
}

/// 单个模型详情（数据源：models.dev `api.json`）；无缓存直连版本，命令路径走
/// `fetch_model_profile_cached`。
#[allow(dead_code)]
pub fn fetch_model_profile(
    path: &str,
    provider: &str,
    model_id: &str,
) -> Result<PiModelProfile, String> {
    let (provider, model_id) = validate_model_identity(provider, model_id)?;
    let _ = validate_model_path(path)?;
    let body = fetch_models_dev_catalog(None)?;
    parse_models_dev_profile(&body, provider, model_id)
}

#[tauri::command]
pub async fn search_pi_models(
    app: tauri::AppHandle,
    request: ModelSearchRequest,
) -> Result<Vec<PiModelSummary>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || search_pi_models_inner(app.state(), request))
        .await
        .map_err(|error| format!("model search worker failed: {error}"))?
}

fn search_pi_models_inner(
    cache: State<'_, ModelCatalogCache>,
    request: ModelSearchRequest,
) -> Result<Vec<PiModelSummary>, String> {
    let query = request.query.trim();
    if query.len() > MAX_QUERY_LENGTH || query.chars().any(char::is_control) {
        return Err("model search query is invalid".into());
    }
    let provider = request
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|provider| !provider.is_empty());
    if provider.is_some_and(|provider| {
        provider.len() > MAX_QUERY_LENGTH
            || provider
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
    }) {
        return Err("model provider filter is invalid".into());
    }
    // 缓存的是 models.dev 全量 JSON（key 固定），过滤在本地做，避免每个关键词都下载 4.5 MB。
    let body = fetch_models_dev_catalog(Some(&cache))?;
    Ok(parse_models_dev_catalog(&body, query, provider))
}

#[tauri::command]
pub async fn pi_model_profile(
    app: tauri::AppHandle,
    request: ModelProfileRequest,
) -> Result<PiModelProfile, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let cache = app.state::<ModelCatalogCache>();
        fetch_model_profile_cached(&cache, &request.path, &request.provider, &request.model_id)
    })
    .await
    .map_err(|error| format!("model profile worker failed: {error}"))?
}

#[tauri::command]
pub async fn search_pi_packages(
    app: tauri::AppHandle,
    request: PackageRequest,
) -> Result<PiPackagePage, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        search_pi_packages_with(&app.state::<MarketCache>(), request, fetch_pi_package_page)
    })
    .await
    .map_err(|error| format!("package search worker failed: {error}"))?
}

fn search_pi_packages_with(
    cache: &MarketCache,
    request: PackageRequest,
    fetch: impl FnOnce(&Url, u32) -> Result<PiPackagePage, String>,
) -> Result<PiPackagePage, String> {
    let url = pi_package_search_url(&request.query, request.page)?;
    let key = url.as_str().to_owned();
    if let Some(page) = cache.get(&key) {
        return Ok(page);
    }
    let page = fetch(&url, request.page)?;
    cache.insert(key, page.clone());
    Ok(page)
}

#[tauri::command]
pub async fn pi_package_metadata(
    request: PackageMetadataRequest,
) -> Result<PackageMetadata, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_package_metadata(&request.name))
        .await
        .map_err(|error| format!("package metadata worker failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::{
        parse_mcp_registry, parse_model_profile, parse_models_dev_catalog,
        parse_models_dev_profile, parse_pi_models, parse_pi_packages, validate_package_name,
        MAX_MODEL_RESULTS,
    };
    use serde_json::json;

    #[test]
    fn parses_mcp_registry_entries() {
        let body = r#"{
          "servers": [
            {"server": {"name": "io.example/context7", "description": "Docs lookup",
              "version": "1.2.3", "repository": {"url": "https://github.com/x/y"},
              "remotes": [{"type": "streamable-http", "url": "https://mcp.example.com/mcp"}],
              "packages": [{"registry_type": "npm", "identifier": "@modelcontextprotocol/server-x"}]}},
            {"server": {}},
            {"other": true}
          ]
        }"#;

        let entries = parse_mcp_registry(body);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "io.example/context7");
        assert_eq!(entries[0].description, "Docs lookup");
        assert_eq!(entries[0].version.as_deref(), Some("1.2.3"));
        assert_eq!(
            entries[0].remote_url.as_deref(),
            Some("https://mcp.example.com/mcp")
        );
        assert_eq!(
            entries[0].package.as_deref(),
            Some("@modelcontextprotocol/server-x")
        );
    }

    #[test]
    fn parse_mcp_registry_tolerates_invalid_payloads() {
        assert!(parse_mcp_registry("not json").is_empty());
        assert!(parse_mcp_registry(r#"{"servers": "nope"}"#).is_empty());
    }

    #[test]
    fn parses_official_model_rows() {
        let html = r#"
          <tr data-model-row="true" data-model-name="GPT 4" data-model-id="gpt-4" data-model-provider="openai">
            <th><a data-model-path="/models/openai/gpt-4">GPT-4</a></th>
            <td data-label="Context">8,192</td>
            <td data-label="Input /M">$30</td>
            <td data-label="Output /M">$60</td>
            <td data-label="Cache read /M">$0</td>
            <td data-label="Cache write /M">$0</td>
          </tr>
        "#;

        let models = parse_pi_models(html, "", None);

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].provider, "openai");
        assert_eq!(models[0].id, "gpt-4");
        assert_eq!(models[0].name, "GPT-4");
        assert_eq!(models[0].context_window, Some(8_192));
        assert_eq!(models[0].input_cost, Some(30.0));
        assert_eq!(models[0].path, "/models/openai/gpt-4");
    }

    #[test]
    fn reads_model_profile_from_pi_configuration() {
        let html = r#"<pre class="raw-data-panel">{
          &quot;providers&quot;: {&quot;openai&quot;: {&quot;api&quot;: &quot;openai-responses&quot;,
          &quot;baseUrl&quot;: &quot;https://api.openai.com/v1&quot;,
          &quot;models&quot;: [{&quot;id&quot;: &quot;gpt-5&quot;, &quot;name&quot;: &quot;GPT-5&quot;, &quot;reasoning&quot;: true,
          &quot;input&quot;: [&quot;text&quot;, &quot;image&quot;], &quot;contextWindow&quot;: 200000,
          &quot;maxTokens&quot;: 32000, &quot;thinkingLevelMap&quot;: {&quot;off&quot;: null, &quot;low&quot;: &quot;low&quot;, &quot;high&quot;: &quot;high&quot;},
          &quot;cost&quot;: {&quot;input&quot;: 1, &quot;output&quot;: 5, &quot;cacheRead&quot;: 0, &quot;cacheWrite&quot;: 0}}]}}
        }</pre>"#;

        let profile = parse_model_profile(html, "openai", "gpt-5").expect("profile should parse");

        assert_eq!(profile.api.as_deref(), Some("openai-responses"));
        assert_eq!(profile.context_window, Some(200000));
        assert_eq!(profile.thinking_levels, vec!["low", "high"]);
        assert_eq!(profile.input, vec!["text", "image"]);
    }

    #[test]
    #[ignore = "requires network access to models.dev"]
    fn fetches_live_models_dev_catalog_and_profile() {
        let models = super::fetch_pi_models("gpt-4", Some("openai"))
            .expect("Pi model catalog should be reachable");
        let model = models
            .iter()
            .find(|model| model.id == "gpt-4")
            .expect("gpt-4 should be in the official catalog");
        let profile = super::fetch_model_profile(&model.path, &model.provider, &model.id)
            .expect("Pi model detail should be reachable");
        assert_eq!(profile.id, "gpt-4");
        assert_eq!(profile.context_window, Some(8_192));
    }

    #[test]
    fn rejects_unsafe_npm_package_names() {
        assert!(validate_package_name("pi-tools").is_ok());
        assert!(validate_package_name("@scope/pi-tools").is_ok());
        assert!(validate_package_name("pi-tools && whoami").is_err());
        assert!(validate_package_name("https://example.com").is_err());
    }

    const CATALOG_ADVISOR: &str = include_str!("fixtures/pi-catalog-advisor.html");
    const CATALOG_PAGE_2: &str = include_str!("fixtures/pi-catalog-page-2.html");
    const CATALOG_TYPES: &str = include_str!("fixtures/pi-catalog-types.html");
    const CATALOG_EMPTY: &str = include_str!("fixtures/pi-catalog-empty.html");

    fn package_request(query: &str, page: u32) -> super::PackageRequest {
        super::PackageRequest {
            query: query.into(),
            page,
        }
    }

    #[test]
    fn package_search_uses_official_encoded_name_downloads_and_page() {
        let query = "@scope/tools & advice+中文";
        let url = super::pi_package_search_url(query, 2).unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("pi.dev"));
        assert_eq!(url.path(), "/packages");
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(params.len(), 3);
        assert_eq!(params["name"], query);
        assert_eq!(params["sort"], "downloads");
        assert_eq!(params["page"], "2");
        assert!(url.as_str().contains("%40scope%2Ftools"));
        assert!(url.as_str().contains("%26"));
        assert!(url.as_str().contains("%2B"));
        let request: super::PackageRequest = serde_json::from_value(json!({"query": ""})).unwrap();
        assert_eq!(request.page, 1);
        assert_eq!(
            super::pi_package_search_url(&request.query, request.page)
                .unwrap()
                .as_str(),
            "https://pi.dev/packages?name=&sort=downloads&page=1"
        );
        assert!(super::pi_package_search_url("advisor", 0).is_err());
        assert!(super::pi_package_search_url("ad\nvisor", 1).is_err());
        assert!(super::pi_package_search_url(&"a".repeat(215), 1).is_err());
    }

    #[test]
    fn catalog_advisor_results_preserve_metadata_and_exclude_recent_sidebar() {
        let page = parse_pi_packages(CATALOG_ADVISOR, 1).unwrap();
        assert_eq!(page.next_page, None);
        assert_eq!(
            page.packages
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["pi-advisor-flow", "pi-advisor"]
        );
        let advisor = &page.packages[1];
        assert_eq!(advisor.types, ["extension"]);
        assert_eq!(advisor.downloads, 322);
        assert_eq!(advisor.published_at, 1_781_260_041_886);
        assert_eq!(advisor.path, "/packages/pi-advisor");
        assert_eq!(advisor.description, "Pi extension package that adds a Claude-style advisor tool for strategic guidance during complex agent tasks.");
        let json = serde_json::to_value(&page).unwrap();
        assert_eq!(json["nextPage"], serde_json::Value::Null);
        assert_eq!(json["packages"][1]["publishedAt"], 1_781_260_041_886_i64);
    }

    #[test]
    fn catalog_preserves_scoped_untyped_packages_and_next_page() {
        let page = parse_pi_packages(CATALOG_PAGE_2, 2).unwrap();
        assert_eq!(page.next_page, Some(3));
        assert_eq!(page.packages.len(), 2);
        assert_eq!(page.packages[0].name, "@braintrust/pi-extension");
        assert_eq!(page.packages[0].path, "/packages/@braintrust/pi-extension");
        assert_eq!(page.packages[0].downloads, 25_850);
        assert_eq!(page.packages[0].published_at, 1_791_303_924_079);
        assert!(page.packages.iter().all(|package| package.types.is_empty()));
        assert_eq!(serde_json::to_value(page).unwrap()["nextPage"], 3);
    }

    #[test]
    fn catalog_keeps_skills_themes_multiple_types_and_official_order() {
        // A description-only query match must survive: no local name/keyword filter.
        let page = super::search_pi_packages_with(
            &super::MarketCache::default(),
            package_request("methodology", 1),
            |url, number| {
                assert_eq!(
                    url.query_pairs().find(|(key, _)| key == "name").unwrap().1,
                    "methodology"
                );
                parse_pi_packages(CATALOG_TYPES, number)
            },
        )
        .unwrap();
        assert_eq!(
            page.packages
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            [
                "bigpowers",
                "@agimon-ai/doompi-web-components",
                "pi-open-tui"
            ]
        );
        assert_eq!(page.packages[0].types, ["skill"]);
        assert_eq!(page.packages[1].types, ["theme"]);
        assert_eq!(page.packages[2].types, ["extension", "theme"]);
        // Even a changed server ordering passes through without being sorted locally.
        let html = CATALOG_TYPES.replace(
            "data-package-downloads=\"130483\"",
            "data-package-downloads=\"0\"",
        );
        assert_eq!(
            parse_pi_packages(&html, 1).unwrap().packages[0].name,
            "bigpowers"
        );
    }

    #[test]
    fn catalog_decodes_entities_and_nested_description_text() {
        let html = CATALOG_ADVISOR.replace(
            "Advanced Executor/Advisor flow for Pi, fully configurable and extendable.",
            "Tools &amp; <strong>helpers</strong> &quot;advisor&quot; &#39;ok&#39; &#x1F680;",
        );
        let page = parse_pi_packages(&html, 1).unwrap();
        assert_eq!(
            page.packages[0].description,
            "Tools & helpers \"advisor\" 'ok' 🚀"
        );
    }

    #[test]
    fn catalog_requires_explicit_empty_state_and_rejects_malformed_pages() {
        let page = parse_pi_packages(CATALOG_EMPTY, 1).unwrap();
        assert!(page.packages.is_empty());
        assert_eq!(page.next_page, None);
        // Target the main catalog so partial corruption reaches the parser seam.
        let (catalog_prefix, catalog) = CATALOG_ADVISOR
            .split_once("packages-index-card\">")
            .unwrap();
        let missing_card_close = format!(
            "{catalog_prefix}packages-index-card\">{}",
            catalog.replacen("</article>", "", 1)
        );
        let missing_catalog_close = format!(
            "{catalog_prefix}packages-index-card\">{}",
            catalog.replacen("</section>", "", 1)
        );
        for (case, html) in [
            String::new(),
            "<html><p>Service unavailable</p></html>".into(),
            "<p class=\"packages-empty\">No packages match this filter.</p>".into(),
            CATALOG_EMPTY.replace("No packages match this filter.", "Service unavailable"),
            CATALOG_ADVISOR.replace("</article>", ""),
            missing_card_close,
            CATALOG_ADVISOR.replace("</section>", ""),
            missing_catalog_close,
            CATALOG_ADVISOR.replace(
                "data-package-downloads=\"322\"",
                "data-package-downloads=\"oops\"",
            ),
            CATALOG_ADVISOR.replace(
                "href=\"/packages/pi-advisor?name=advisor\"",
                "href=\"/packages/other\"",
            ),
            format!("{CATALOG_EMPTY}{CATALOG_EMPTY}"),
        ]
        .into_iter()
        .enumerate()
        {
            let result = parse_pi_packages(&html, 1);
            assert!(
                result.is_err(),
                "accepted malformed catalog case {case}: {result:?}"
            );
            let error = result.unwrap_err();
            assert!(
                error.starts_with("Pi package catalog HTML is invalid:"),
                "{error}"
            );
        }
    }

    #[test]
    fn catalog_rejects_invalid_next_links_and_inconsistent_empty_results() {
        for href in [
            "https://example.com/packages?page=3",
            "/packages?page=1",
            "/packages?page=3&amp;page=4",
            "/packages?page=oops",
            "/other?page=3",
        ] {
            let html = CATALOG_PAGE_2.replace(
                "href=\"/packages?page=3\">Next",
                &format!("href=\"{href}\">Next"),
            );
            assert!(
                parse_pi_packages(&html, 2).is_err(),
                "accepted next link: {href}"
            );
        }
        let html = CATALOG_ADVISOR.replace(
            "<h2>All packages</h2>",
            "<h2>All packages</h2><p class=\"packages-empty\">No packages match this filter.</p>",
        );
        assert!(parse_pi_packages(&html, 1).is_err());
        let empty_with_next = CATALOG_EMPTY.replace(
            "<p class=\"packages-empty\">No packages match this filter.</p>",
            "<p class=\"packages-empty\">No packages match this filter.</p><a class=\"pagination-link\" href=\"/packages?page=2\">Next →</a>",
        );
        assert!(parse_pi_packages(&empty_with_next, 1).is_err());
        let html = CATALOG_PAGE_2.replace("/packages?page=3", "/packages?page=3&amp;name=a%26b");
        assert_eq!(parse_pi_packages(&html, 2).unwrap().next_page, Some(3));
    }

    #[test]
    fn package_cache_keys_query_and_page_and_preserves_pagination() {
        let cache = super::MarketCache::default();
        let first =
            super::search_pi_packages_with(&cache, package_request("advisor", 1), |_, page| {
                parse_pi_packages(CATALOG_ADVISOR, page)
            })
            .unwrap();
        assert_eq!(
            super::search_pi_packages_with(&cache, package_request(" advisor ", 1), |_, _| panic!(
                "first page should be cached"
            ))
            .unwrap(),
            first
        );
        let second =
            super::search_pi_packages_with(&cache, package_request("advisor", 2), |_, page| {
                parse_pi_packages(CATALOG_PAGE_2, page)
            })
            .unwrap();
        assert_eq!(second.next_page, Some(3));
        assert_eq!(
            super::search_pi_packages_with(&cache, package_request("advisor", 2), |_, _| panic!(
                "second page should be cached"
            ))
            .unwrap(),
            second
        );
        let empty =
            super::search_pi_packages_with(&cache, package_request("different", 1), |_, page| {
                parse_pi_packages(CATALOG_EMPTY, page)
            })
            .unwrap();
        assert!(empty.packages.is_empty());
        assert_eq!(
            super::search_pi_packages_with(&cache, package_request("different", 1), |_, _| panic!(
                "explicit empty results may be cached"
            ))
            .unwrap(),
            empty
        );
    }

    #[test]
    fn package_search_errors_are_not_cached_and_expired_pages_are_refetched() {
        let cache = super::MarketCache::default();
        assert!(
            super::search_pi_packages_with(&cache, package_request("advisor", 1), |_, _| Err(
                "catalog unavailable".into()
            ))
            .is_err()
        );
        assert!(super::search_pi_packages_with(
            &cache,
            package_request("advisor", 1),
            |_, page| parse_pi_packages("<html>broken</html>", page)
        )
        .is_err());
        let page =
            super::search_pi_packages_with(&cache, package_request("advisor", 1), |_, page| {
                parse_pi_packages(CATALOG_ADVISOR, page)
            })
            .unwrap();
        assert_eq!(page.packages.len(), 2);
        let key = super::pi_package_search_url("advisor", 1)
            .unwrap()
            .to_string();
        cache.entries.lock().unwrap().get_mut(&key).unwrap().0 =
            std::time::Instant::now() - super::CACHE_TTL - std::time::Duration::from_secs(1);
        assert!(cache.get(&key).is_none());
        let refreshed =
            super::search_pi_packages_with(&cache, package_request("advisor", 1), |_, page| {
                parse_pi_packages(CATALOG_EMPTY, page)
            })
            .unwrap();
        assert!(refreshed.packages.is_empty());
    }

    #[test]
    #[ignore = "requires network access to the official pi.dev catalog"]
    fn fetches_live_official_catalog_advisor_and_second_page() {
        let fetch = |query, page| {
            super::fetch_pi_package_page(&super::pi_package_search_url(query, page).unwrap(), page)
                .unwrap()
        };
        let advisor = fetch("advisor", 1);
        assert!(advisor
            .packages
            .iter()
            .any(|package| package.name == "pi-advisor"));
        assert!(advisor
            .packages
            .iter()
            .any(|package| package.name == "pi-advisor-flow"));
        for unrelated in [
            "@aliou/pi-processes",
            "@braintrust/pi-extension",
            "pi-ui-kit",
            "bigpowers",
        ] {
            assert!(!advisor
                .packages
                .iter()
                .any(|package| package.name == unrelated));
        }
        let first = fetch("", 1);
        assert_eq!(first.packages.len(), 50);
        assert_eq!(first.next_page, Some(2));
        let second = fetch("", 2);
        assert_eq!(second.packages.len(), 50);
        assert_eq!(second.next_page, Some(3));
        assert!(!second
            .packages
            .iter()
            .any(|package| first.packages.iter().any(|p| p.name == package.name)));
        assert!(fetch("deeppi-no-match-20261008-5f092d", 1)
            .packages
            .is_empty());
    }

    // —— models.dev 目录/详情解析（离线边界用例） ——
    /// models.dev `api.json` 小样本：两个供应商、三种边界（完整/缺字段/畸形）。
    const MODELS_DEV_FIXTURE: &str = r#"{
      "acme": {
        "id": "acme", "name": "Acme", "models": {
          "alpha-1": {
            "id": "acme/alpha-1", "name": "Alpha 1", "reasoning": true,
            "modalities": { "input": ["text", "image"], "output": ["text"] },
            "limit": { "context": 200000, "output": 8192 },
            "cost": { "input": 1.5, "output": 6, "cache_read": 0.15, "cache_write": 1.9 }
          },
          "bare": { "name": "Bare Model" },
          "broken-cost": {
            "id": "acme/broken-cost", "cost": { "input": "free", "output": null }
          }
        }
      },
      "beta-labs": {
        "name": "Beta Labs", "models": {
          "gamma": { "id": "beta-labs/gamma", "reasoning": false, "limit": { "context": 128000 } }
        }
      }
    }"#;

    #[test]
    fn maps_models_dev_catalog_fields_and_optional_values() {
        let models = parse_models_dev_catalog(MODELS_DEV_FIXTURE, "", None);
        // provider → name 排序：acme 的三条在前（alpha-1 / bare / broken-cost 按 name 排）
        assert_eq!(models.len(), 4);
        let alpha = models
            .iter()
            .find(|model| model.id == "acme/alpha-1")
            .unwrap();
        assert_eq!(alpha.provider, "acme");
        assert_eq!(alpha.name, "Alpha 1");
        assert_eq!(alpha.path, "https://models.dev/acme/alpha-1");
        assert_eq!(alpha.context_window, Some(200_000));
        assert_eq!(alpha.input_cost, Some(1.5));
        assert_eq!(alpha.output_cost, Some(6.0));
        assert_eq!(alpha.cache_read_cost, Some(0.15));
        assert_eq!(alpha.cache_write_cost, Some(1.9));

        // 缺 id → 用 provider/model 拼；缺全部可选字段 → None / 回落
        let bare = models
            .iter()
            .find(|model| model.name == "Bare Model")
            .unwrap();
        assert_eq!(bare.id, "acme/bare");
        assert_eq!(bare.context_window, None);
        assert_eq!(bare.input_cost, None);
        assert_eq!(bare.cache_write_cost, None);

        // cost 里出现非数字（字符串 / null）→ 对应字段 None，不 panic
        let broken = models
            .iter()
            .find(|model| model.id == "acme/broken-cost")
            .unwrap();
        assert_eq!(broken.input_cost, None);
        assert_eq!(broken.output_cost, None);

        let gamma = models
            .iter()
            .find(|model| model.id == "beta-labs/gamma")
            .unwrap();
        assert_eq!(gamma.provider, "beta-labs");
        assert_eq!(gamma.context_window, Some(128_000));
    }

    #[test]
    fn filters_models_dev_catalog_by_query_and_provider() {
        let all = parse_models_dev_catalog(MODELS_DEV_FIXTURE, "", None);
        assert_eq!(all.len(), 4);

        // query 命中 name / id / provider 三类，大小写不敏感
        assert_eq!(
            parse_models_dev_catalog(MODELS_DEV_FIXTURE, "alpha", None).len(),
            1
        );
        assert_eq!(
            parse_models_dev_catalog(MODELS_DEV_FIXTURE, "BETA-LABS", None).len(),
            1
        );
        assert_eq!(
            parse_models_dev_catalog(MODELS_DEV_FIXTURE, "acme/", None).len(),
            3
        );
        assert!(parse_models_dev_catalog(MODELS_DEV_FIXTURE, "missing", None).is_empty());

        // provider_filter 精确匹配（大小写不敏感），空串等同未指定
        assert_eq!(
            parse_models_dev_catalog(MODELS_DEV_FIXTURE, "", Some("ACME")).len(),
            3
        );
        assert_eq!(
            parse_models_dev_catalog(MODELS_DEV_FIXTURE, "", Some("beta-labs")).len(),
            1
        );
        assert_eq!(
            parse_models_dev_catalog(MODELS_DEV_FIXTURE, "", Some("  ")).len(),
            4
        );
        assert!(parse_models_dev_catalog(MODELS_DEV_FIXTURE, "", Some("nope")).is_empty());
    }

    #[test]
    fn accepts_models_dev_array_shape_and_caps_results() {
        // 顶层是数组时用元素 id 作 provider key
        let array = json!([
            { "id": "solo", "name": "Solo", "models": { "one": { "id": "solo/one" } } }
        ])
        .to_string();
        let models = parse_models_dev_catalog(&array, "", None);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].provider, "solo");
        assert_eq!(models[0].id, "solo/one");

        // 超过 MAX_MODEL_RESULTS 时截断
        let mut big = serde_json::Map::new();
        let mut provider_models = serde_json::Map::new();
        for index in 0..(MAX_MODEL_RESULTS + 25) {
            provider_models.insert(
                format!("model-{index:04}"),
                json!({ "id": format!("big/model-{index:04}") }),
            );
        }
        big.insert(
            "big".to_owned(),
            json!({ "id": "big", "models": provider_models }),
        );
        let body = serde_json::Value::Object(big).to_string();
        assert_eq!(
            parse_models_dev_catalog(&body, "", None).len(),
            MAX_MODEL_RESULTS
        );
    }

    #[test]
    fn tolerates_malformed_models_dev_catalog() {
        // 非法 JSON / 空对象 / models 非对象 / 供应商值非对象 → 空结果且不 panic
        assert!(parse_models_dev_catalog("not json", "", None).is_empty());
        assert!(parse_models_dev_catalog("{}", "", None).is_empty());
        assert!(parse_models_dev_catalog(r#"{"p":{"models":"nope"}}"#, "", None).is_empty());
        assert!(parse_models_dev_catalog(r#"{"p":"nope"}"#, "", None).is_empty());
        assert!(parse_models_dev_catalog("[]", "", None).is_empty());
    }

    #[test]
    fn reads_models_dev_profile_with_reasoning_and_modalities() {
        let profile = parse_models_dev_profile(MODELS_DEV_FIXTURE, "acme", "acme/alpha-1")
            .expect("profile should resolve by id");
        assert_eq!(profile.name, "Alpha 1");
        assert!(profile.reasoning);
        assert_eq!(profile.context_window, Some(200_000));
        assert_eq!(profile.max_tokens, Some(8192));
        assert_eq!(profile.input, vec!["text".to_owned(), "image".to_owned()]);
        // reasoning = true → 按前端既有约定给五档
        assert_eq!(profile.thinking_levels.len(), 5);
        let cost = profile.cost.expect("cost should be present");
        assert_eq!(cost.input, Some(1.5));
        assert_eq!(cost.cache_write, Some(1.9));

        // 也能用 model key（而非完整 id）解析；provider 大小写不敏感
        let by_key = parse_models_dev_profile(MODELS_DEV_FIXTURE, "ACME", "alpha-1")
            .expect("profile should resolve by model key");
        assert_eq!(by_key.id, "alpha-1");

        // reasoning = false → 空 thinking levels
        let gamma = parse_models_dev_profile(MODELS_DEV_FIXTURE, "beta-labs", "gamma")
            .expect("gamma should resolve");
        assert!(!gamma.reasoning);
        assert!(gamma.thinking_levels.is_empty());
        assert!(gamma.cost.is_none());
    }

    #[test]
    fn rejects_unknown_or_broken_models_dev_profile_requests() {
        assert!(parse_models_dev_profile(MODELS_DEV_FIXTURE, "nope", "alpha-1").is_err());
        assert!(parse_models_dev_profile(MODELS_DEV_FIXTURE, "acme", "missing").is_err());
        assert!(parse_models_dev_profile("not json", "acme", "alpha-1").is_err());
    }
}
