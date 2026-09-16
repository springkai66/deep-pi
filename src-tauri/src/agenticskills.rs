//! AgenticSkills 市场（https://agenticskills.io）后端。
//!
//! 站点是 Next.js 应用，没有公开 JSON API，数据全部从 HTML 里解析：
//! - 列表页（`/skills`、`/mcp`）里的 JSON-LD `<script type="application/ld+json">`
//!   给出全量目录（slug + name），正文里的精选卡片额外带作者/描述/标签/安装量等。
//! - 详情页把数据塞在 Next.js flight 数据 `self.__next_f.push([1,"…"])` 里，
//!   反转义后按平衡括号扫描出目标对象再用 serde_json 解析。

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{State, Url};

use crate::{
    agent_config::{self, ConfigScope, MAX_SKILL_DESCRIPTION_LENGTH},
    app_paths::AppPaths,
    market::{decode_entities, http_agent, read_response, strip_tags},
    message::{msg, msg_with},
};

const AGENTIC_SKILLS_URL: &str = "https://agenticskills.io/skills";
const AGENTIC_MCP_URL: &str = "https://agenticskills.io/mcp";
const CACHE_TTL: Duration = Duration::from_secs(600);
const MAX_RESULTS: usize = 200;
const MAX_SLUG_LENGTH: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticSkillEntry {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub installs: Option<String>,
    pub quality: Option<String>,
    pub featured: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticSkillDetail {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub author: Option<String>,
    pub author_url: Option<String>,
    pub tags: Vec<String>,
    pub platforms: Vec<String>,
    pub license: Option<String>,
    pub quality: Option<String>,
    pub last_updated: Option<String>,
    pub github_url: Option<String>,
    pub skill_md_url: Option<String>,
    pub install_command: Option<String>,
    pub source_url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticMcpEntry {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub transport: Vec<String>,
    pub official: bool,
    pub requires_api_key: bool,
    pub popularity: Option<String>,
    pub tags: Vec<String>,
    pub featured: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticMcpSnippet {
    pub label: String,
    pub file: Option<String>,
    pub code: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticMcpDetail {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub author: Option<String>,
    pub author_url: Option<String>,
    pub category: Option<String>,
    pub official: bool,
    pub trust_level: Option<String>,
    pub transport: Vec<String>,
    pub requires_api_key: bool,
    pub website_url: Option<String>,
    pub config_source: Option<String>,
    pub popularity: Option<String>,
    pub tags: Vec<String>,
    pub snippets: Vec<AgenticMcpSnippet>,
    pub source_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgenticQuery {
    pub query: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgenticSlug {
    pub slug: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgenticSkillInstall {
    pub slug: String,
    pub scope: ConfigScope,
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgenticMcpInstall {
    pub slug: String,
    #[serde(default)]
    pub name: Option<String>,
    pub scope: ConfigScope,
    #[serde(default)]
    pub project_path: Option<String>,
}

/// 列表页缓存：与 `market::McpRegistryCache` 同样的 TTL 策略。
#[derive(Default)]
pub struct AgenticCatalogCache {
    skills: Mutex<Option<(Instant, Vec<AgenticSkillEntry>)>>,
    mcp: Mutex<Option<(Instant, Vec<AgenticMcpEntry>)>>,
}

impl AgenticCatalogCache {
    fn skills(&self) -> Option<Vec<AgenticSkillEntry>> {
        let guard = self.skills.lock().ok()?;
        let (created, entries) = guard.as_ref()?;
        (created.elapsed() < CACHE_TTL).then(|| entries.clone())
    }

    fn store_skills(&self, entries: Vec<AgenticSkillEntry>) {
        if let Ok(mut guard) = self.skills.lock() {
            *guard = Some((Instant::now(), entries));
        }
    }

    fn mcp(&self) -> Option<Vec<AgenticMcpEntry>> {
        let guard = self.mcp.lock().ok()?;
        let (created, entries) = guard.as_ref()?;
        (created.elapsed() < CACHE_TTL).then(|| entries.clone())
    }

    fn store_mcp(&self, entries: Vec<AgenticMcpEntry>) {
        if let Ok(mut guard) = self.mcp.lock() {
            *guard = Some((Instant::now(), entries));
        }
    }
}

pub fn validate_slug(slug: &str) -> Result<&str, String> {
    let trimmed = slug.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_SLUG_LENGTH
        || !trimmed.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(msg("market.agenticskills_slug_invalid"));
    }
    Ok(trimmed)
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).and_then(non_empty)
}

fn json_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn json_string_list(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().and_then(non_empty))
                .collect()
        })
        .unwrap_or_default()
}

fn author_fields(value: &Value) -> (Option<String>, Option<String>) {
    let Some(author) = value.get("author") else {
        return (None, None);
    };
    if let Some(name) = author.as_str() {
        return (non_empty(name), None);
    }
    (json_string(author, "name"), json_string(author, "url"))
}

fn slug_from_url(url: &str) -> Option<String> {
    let cleaned = url.split(['?', '#']).next().unwrap_or(url);
    let slug = cleaned.trim_end_matches('/').rsplit('/').next()?;
    (!slug.is_empty()).then(|| slug.to_owned())
}

fn json_ld_entries(html: &str) -> Vec<(String, String)> {
    let Some(marker) = html.find("application/ld+json") else {
        return Vec::new();
    };
    let Some(content_start) = html[marker..].find('>').map(|offset| marker + offset + 1) else {
        return Vec::new();
    };
    let Some(content_end) = html[content_start..]
        .find("</script>")
        .map(|offset| content_start + offset)
    else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&html[content_start..content_end]) else {
        return Vec::new();
    };
    let pages: Vec<&Value> = match value.as_array() {
        Some(items) => items.iter().collect(),
        None => vec![&value],
    };
    for page in pages {
        let mut elements: Vec<&Value> = Vec::new();
        if let Some(entity) = page.get("mainEntity") {
            match entity {
                Value::Array(lists) => {
                    for list in lists {
                        if let Some(items) = list.get("itemListElement").and_then(Value::as_array) {
                            elements.extend(items.iter());
                        }
                    }
                }
                _ => {
                    if let Some(items) = entity.get("itemListElement").and_then(Value::as_array) {
                        elements.extend(items.iter());
                    }
                }
            }
        }
        let mut entries = Vec::new();
        for element in elements {
            let Some(slug) = element
                .get("url")
                .and_then(Value::as_str)
                .and_then(slug_from_url)
            else {
                continue;
            };
            let Some(name) = element.get("name").and_then(Value::as_str) else {
                continue;
            };
            let name = decode_entities(name).trim().to_owned();
            if name.is_empty() || validate_slug(&slug).is_err() {
                continue;
            }
            entries.push((slug, name));
        }
        if !entries.is_empty() {
            return entries;
        }
    }
    Vec::new()
}

/// 遍历列表页里指向 `/<kind>/<slug>` 的精选卡片。
fn each_card(html: &str, kind: &str, mut apply: impl FnMut(&str, &str)) {
    let needle = format!("href=\"/{kind}/");
    let mut cursor = 0;
    while let Some(relative) = html[cursor..].find(&needle) {
        let href = cursor + relative;
        cursor = href + needle.len();
        let Some(anchor_start) = html[..href].rfind("<a ") else {
            continue;
        };
        let Some(open_end) = html[anchor_start..]
            .find('>')
            .map(|offset| anchor_start + offset)
        else {
            continue;
        };
        if !html[anchor_start..=open_end].contains("group block") {
            continue;
        }
        let slug_start = href + needle.len();
        let Some(quote) = html[slug_start..]
            .find('"')
            .map(|offset| slug_start + offset)
        else {
            continue;
        };
        let slug = &html[slug_start..quote];
        if validate_slug(slug).is_err() {
            continue;
        }
        let Some(close) = html[open_end..]
            .find("</a>")
            .map(|offset| open_end + offset)
        else {
            continue;
        };
        cursor = close + "</a>".len();
        apply(slug, &html[open_end + 1..close]);
    }
}

struct Pill {
    raw: String,
    text: String,
}

fn pills(body: &str) -> Vec<Pill> {
    let mut result = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = body[cursor..].find("<span") {
        let start = cursor + relative;
        let Some(open_end) = body[start..].find('>').map(|offset| start + offset) else {
            break;
        };
        cursor = open_end + 1;
        let Some(close) = body[open_end..]
            .find("</span>")
            .map(|offset| open_end + offset)
        else {
            continue;
        };
        result.push(Pill {
            raw: body[start..close + "</span>".len()].to_owned(),
            text: visible_text(&body[open_end + 1..close]),
        });
    }
    result
}
/// 去掉标签并还原 HTML 实体（站点正文里会出现 `&#x27;` 这类实体）。
fn visible_text(html: &str) -> String {
    decode_entities(&strip_tags(html))
}

fn element_text(body: &str, tag: &str) -> Option<String> {
    let open = body.find(&format!("<{tag}"))?;
    let content_start = body[open..].find('>')? + open + 1;
    let content_end = body[content_start..].find(&format!("</{tag}>"))? + content_start;
    non_empty(&visible_text(&body[content_start..content_end]))
}

fn paragraph_text(body: &str, marker: &str) -> Option<String> {
    let at = body.find(marker)?;
    let content_start = body[at..].find('>')? + at + 1;
    let content_end = body[content_start..].find("</p>")? + content_start;
    non_empty(&visible_text(&body[content_start..content_end]))
}

fn strip_by_prefix(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("by") {
        return None;
    }
    let rest = if trimmed.len() > 2 && trimmed[..2].eq_ignore_ascii_case("by") {
        &trimmed[2..]
    } else {
        trimmed
    };
    non_empty(rest)
}

fn description_text(body: &str) -> Option<String> {
    let at = body.find("line-clamp-2")?;
    let content_start = body[at..].find('>')? + at + 1;
    let content_end = body[content_start..].find("</p>")? + content_start;
    non_empty(&visible_text(&body[content_start..content_end]))
}

fn star_value(body: &str) -> Option<String> {
    let at = body.find("lucide-star")?;
    let svg_end = body[at..].find("</svg>")? + at + "</svg>".len();
    let value_end = body[svg_end..].find('<')? + svg_end;
    non_empty(&visible_text(&body[svg_end..value_end]))
}

fn popularity_text(body: &str) -> Option<String> {
    let marker = "text-xs text-[var(--mute-2)]";
    let mut cursor = 0;
    while let Some(relative) = body[cursor..].find(marker) {
        let at = cursor + relative;
        let Some(open_end) = body[at..].find('>').map(|offset| at + offset + 1) else {
            break;
        };
        cursor = open_end;
        let Some(value_end) = body[open_end..].find('<').map(|offset| open_end + offset) else {
            break;
        };
        let text = visible_text(&body[open_end..value_end]);
        if text.contains("visitors/wk") {
            return non_empty(&text);
        }
    }
    None
}

fn apply_skill_card(entry: &mut AgenticSkillEntry, body: &str) {
    if let Some(name) = element_text(body, "h3") {
        entry.name = name;
    }
    if entry.author.is_none() {
        entry.author = paragraph_text(body, "text-xs text-[var(--mute-2)]")
            .and_then(|text| strip_by_prefix(&text));
    }
    if entry.description.is_none() {
        entry.description = description_text(body);
    }
    let pills = pills(body);
    if entry.tags.is_empty() {
        entry.tags = pills
            .iter()
            .filter(|pill| pill.raw.contains("tracking-[0.08em]"))
            .filter_map(|pill| non_empty(&pill.text))
            .collect();
    }
    if entry.installs.is_none() {
        entry.installs = star_value(body);
    }
    if entry.quality.is_none() {
        entry.quality = pills
            .iter()
            .find(|pill| pill.raw.contains("tracking-[0.12em]") && pill.text.contains("-rank"))
            .and_then(|pill| non_empty(&pill.text.replace("-rank", "")));
    }
    if !entry.featured {
        entry.featured = body.contains(">Featured</span>");
    }
}

fn apply_mcp_card(entry: &mut AgenticMcpEntry, body: &str) {
    if let Some(name) = element_text(body, "h3") {
        entry.name = name;
    }
    if entry.author.is_none() {
        entry.author = paragraph_text(body, "text-xs text-[var(--mute-2)]")
            .and_then(|text| strip_by_prefix(&text));
    }
    if entry.description.is_none() {
        entry.description = description_text(body);
    }
    let pills = pills(body);
    if entry.transport.is_empty() {
        entry.transport = pills
            .iter()
            .filter(|pill| pill.raw.contains("lucide-plug"))
            .filter_map(|pill| non_empty(&pill.text))
            .collect();
    }
    if !entry.requires_api_key {
        entry.requires_api_key = pills.iter().any(|pill| pill.raw.contains("lucide-key"));
    }
    // 卡片里无图标的 pill 是「分类 / 语言」这类元信息，最后一个通常是分类。
    let plain: Vec<String> = pills
        .iter()
        .filter(|pill| pill.raw.contains("bg-white"))
        .filter_map(|pill| non_empty(&pill.text))
        .collect();
    if entry.category.is_none() {
        if let Some(category) = plain.last() {
            entry.category = Some(category.clone());
        }
    }
    if entry.tags.is_empty() && plain.len() > 1 {
        entry.tags = plain[..plain.len() - 1].to_vec();
    }
    if !entry.official {
        entry.official = pills.iter().any(|pill| pill.text == "Official");
    }
    if entry.popularity.is_none() {
        entry.popularity = popularity_text(body).or_else(|| star_value(body));
    }
    if !entry.featured {
        entry.featured = body.contains(">Featured</span>");
    }
}

pub fn parse_skill_catalog(html: &str) -> Vec<AgenticSkillEntry> {
    let mut entries: Vec<AgenticSkillEntry> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (slug, name) in json_ld_entries(html) {
        if index.contains_key(&slug) {
            continue;
        }
        index.insert(slug.clone(), entries.len());
        entries.push(AgenticSkillEntry {
            slug,
            name,
            ..AgenticSkillEntry::default()
        });
    }
    each_card(html, "skills", |slug, body| {
        let position = match index.get(slug) {
            Some(position) => *position,
            None => {
                let position = entries.len();
                entries.push(AgenticSkillEntry {
                    slug: slug.to_owned(),
                    name: slug.to_owned(),
                    ..AgenticSkillEntry::default()
                });
                index.insert(slug.to_owned(), position);
                position
            }
        };
        apply_skill_card(&mut entries[position], body);
    });
    entries
}

pub fn parse_mcp_catalog(html: &str) -> Vec<AgenticMcpEntry> {
    let mut entries: Vec<AgenticMcpEntry> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (slug, name) in json_ld_entries(html) {
        if index.contains_key(&slug) {
            continue;
        }
        index.insert(slug.clone(), entries.len());
        entries.push(AgenticMcpEntry {
            slug,
            name,
            ..AgenticMcpEntry::default()
        });
    }
    each_card(html, "mcp", |slug, body| {
        let position = match index.get(slug) {
            Some(position) => *position,
            None => {
                let position = entries.len();
                entries.push(AgenticMcpEntry {
                    slug: slug.to_owned(),
                    name: slug.to_owned(),
                    ..AgenticMcpEntry::default()
                });
                index.insert(slug.to_owned(), position);
                position
            }
        };
        apply_mcp_card(&mut entries[position], body);
    });
    entries
}

/// 把 `self.__next_f.push([1,"…"])` 的字符串字面量逐段反转义后拼成 flight 数据。
fn flight_payload(html: &str) -> String {
    const MARKER: &str = "self.__next_f.push([1,";
    let mut payload = String::new();
    let mut cursor = 0;
    while let Some(relative) = html[cursor..].find(MARKER) {
        let start = cursor + relative + MARKER.len();
        let bytes = html.as_bytes();
        if bytes.get(start) != Some(&b'"') {
            cursor = start;
            continue;
        }
        let mut end = start + 1;
        while end < bytes.len() {
            match bytes[end] {
                b'\\' => end += 2,
                b'"' => break,
                _ => end += 1,
            }
        }
        if end >= bytes.len() {
            break;
        }
        if let Some(literal) = html.get(start..=end) {
            if let Ok(chunk) = serde_json::from_str::<String>(literal) {
                payload.push_str(&chunk);
            }
        }
        cursor = end + 1;
    }
    payload
}

fn json_object_at(text: &str, brace: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.get(brace) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut index = brace;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            match byte {
                b'\\' => index += 1,
                b'"' => in_string = false,
                _ => {}
            }
        } else {
            match byte {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return text.get(brace..=index);
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    None
}

/// 找到 flight 数据里第一个「含 slug/name 的对象」，例如 `"skill":{…}` / `"server":{…}`。
fn find_detail_object(payload: &str, marker: &str) -> Option<Value> {
    let mut cursor = 0;
    while let Some(relative) = payload[cursor..].find(marker) {
        let mut index = cursor + relative + marker.len();
        let bytes = payload.as_bytes();
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) != Some(&b'{') {
            cursor = index;
            continue;
        }
        cursor = index + 1;
        let Some(text) = json_object_at(payload, index) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        if json_string(&value, "slug").is_some() && json_string(&value, "name").is_some() {
            return Some(value);
        }
    }
    None
}

fn skill_detail_from_value(value: &Value) -> AgenticSkillDetail {
    let slug = json_string(value, "slug").unwrap_or_default();
    let (author, author_url) = author_fields(value);
    AgenticSkillDetail {
        source_url: format!("{AGENTIC_SKILLS_URL}/{slug}"),
        slug,
        name: json_string(value, "name").unwrap_or_default(),
        description: json_string(value, "description"),
        long_description: json_string(value, "longDescription"),
        author,
        author_url,
        tags: json_string_list(value, "tags"),
        platforms: json_string_list(value, "platforms"),
        license: json_string(value, "license"),
        quality: json_string(value, "quality"),
        last_updated: json_string(value, "lastUpdated"),
        github_url: json_string(value, "githubUrl"),
        skill_md_url: json_string(value, "skillMdUrl"),
        install_command: json_string(value, "installCommand"),
    }
}

fn mcp_detail_from_value(value: &Value) -> AgenticMcpDetail {
    let slug = json_string(value, "slug").unwrap_or_default();
    let (author, author_url) = author_fields(value);
    let snippets = value
        .get("config")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let code = json_string(item, "code")?;
                    Some(AgenticMcpSnippet {
                        label: json_string(item, "label").unwrap_or_default(),
                        file: json_string(item, "file"),
                        code,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let source_url = format!("{AGENTIC_MCP_URL}/{slug}");
    AgenticMcpDetail {
        slug,
        name: json_string(value, "name").unwrap_or_default(),
        description: json_string(value, "description"),
        long_description: json_string(value, "longDescription"),
        author,
        author_url,
        category: json_string(value, "category"),
        official: json_bool(value, "isOfficial"),
        trust_level: json_string(value, "trustLevel"),
        transport: json_string_list(value, "transport"),
        requires_api_key: json_bool(value, "requiresApiKey"),
        website_url: json_string(value, "websiteUrl"),
        config_source: json_string(value, "configSource"),
        popularity: json_string(value, "popularity"),
        tags: json_string_list(value, "tags"),
        snippets,
        source_url,
    }
}

pub fn parse_skill_detail(html: &str) -> Option<AgenticSkillDetail> {
    let payload = flight_payload(html);
    let value = find_detail_object(&payload, "\"skill\":")?;
    Some(skill_detail_from_value(&value))
}

pub fn parse_mcp_detail(html: &str) -> Option<AgenticMcpDetail> {
    let payload = flight_payload(html);
    let value = find_detail_object(&payload, "\"server\":")?;
    Some(mcp_detail_from_value(&value))
}

/// 把站点给出的配置片段解析成可写入 mcp.json 的服务器对象。
///
/// 支持 `"mcpServers": { … }`（缺外层花括号）、`{"mcpServers": {…}}` 以及
/// VS Code 风格的 `{"servers": {…}}` / 直接的 `{"command":…}` / `{"url":…}`。
/// 站点配置片段里有的是 CLI 命令（终端里复制粘贴的形式），这里折成 mcp 配置对象。
///
/// 支持 `npx mcp-remote <url>`、`npx/bunx/uvx/docker …`、裸 URL，以及
/// `claude mcp add <name> [-e K=V] [--header "K: V"] [--transport http] [--] <url|命令…>`。
/// 片段里的占位符（如 `YOUR_API_KEY`、`$GITHUB_PAT`）原样保留，交由用户填写。
fn cli_snippet_config(code: &str) -> Option<Value> {
    let line = code
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))?;
    let tokens = shell_tokens(line)?;
    let first = tokens.first()?.as_str();
    if first == "claude"
        && tokens.get(1).map(String::as_str) == Some("mcp")
        && tokens.get(2).map(String::as_str) == Some("add")
    {
        return claude_mcp_add(&tokens[3..]);
    }
    if matches!(
        first,
        "npx" | "bunx" | "pnpx" | "uvx" | "uv" | "docker" | "deno" | "node"
    ) {
        // 只接受看起来真的在启动 MCP 服务的命令行；`npx ctx7 setup` 这类
        // 「初始化/安装」命令写进 mcp.json 只会把安装器当成服务器来跑。
        let looks_like_server = tokens[1..]
            .iter()
            .any(|token| token.to_ascii_lowercase().contains("mcp"));
        return looks_like_server.then(|| json_command(first, &tokens[1..]));
    }
    if tokens.len() == 1 && is_http_url(first) {
        return Some(serde_json::json!({ "url": first }));
    }
    None
}

fn json_command(command: &str, args: &[String]) -> Value {
    serde_json::json!({ "command": command, "args": args })
}

fn claude_mcp_add(rest: &[String]) -> Option<Value> {
    // 跳过服务名（`claude mcp add <name> …`）。
    let mut index = usize::from(rest.first().is_some_and(|token| !token.starts_with('-')));
    let mut env = serde_json::Map::new();
    let mut headers = serde_json::Map::new();
    let mut tail: Vec<String> = Vec::new();
    let mut passthrough = false;
    while index < rest.len() {
        let token = rest[index].as_str();
        if passthrough {
            tail.push(rest[index].clone());
            index += 1;
            continue;
        }
        match token {
            "--" => {
                passthrough = true;
                index += 1;
            }
            "--transport" | "-t" | "--scope" | "-s" => index += 2,
            "-e" | "--env" => {
                if let Some((key, value)) =
                    rest.get(index + 1).and_then(|pair| pair.split_once('='))
                {
                    env.insert(key.to_owned(), Value::String(value.to_owned()));
                }
                index += 2;
            }
            "--header" | "-H" => {
                if let Some((key, value)) =
                    rest.get(index + 1).and_then(|pair| pair.split_once(':'))
                {
                    headers.insert(
                        key.trim().to_owned(),
                        Value::String(value.trim().to_owned()),
                    );
                }
                index += 2;
            }
            other if other.starts_with('-') => index += 1,
            _ => {
                tail.push(rest[index].clone());
                index += 1;
            }
        }
    }
    // `--transport http <name> <url>` 形式的 URL 排在服务名之后：只要尾部出现 URL
    // 就按远程服务器写入（服务名只用于命名，配置里不需要它）。
    let mut config = serde_json::Map::new();
    match tail.iter().find(|token| is_http_url(token)) {
        Some(url) => {
            config.insert("url".to_owned(), Value::String(url.clone()));
        }
        None => {
            let command = tail.first()?;
            config.insert("command".to_owned(), Value::String(command.clone()));
            if tail.len() > 1 {
                config.insert(
                    "args".to_owned(),
                    Value::Array(tail[1..].iter().cloned().map(Value::String).collect()),
                );
            }
        }
    }
    if !env.is_empty() {
        config.insert("env".to_owned(), Value::Object(env));
    }
    if !headers.is_empty() {
        config.insert("headers".to_owned(), Value::Object(headers));
    }
    Some(Value::Object(config))
}

fn is_http_url(token: &str) -> bool {
    token.starts_with("https://") || token.starts_with("http://")
}

/// 按 shell 引号规则切词（`--header "A: B"` 要算一个词）；引号不闭合时返回 `None`。
fn shell_tokens(line: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut quoted = false;
    for character in line.chars() {
        match quote {
            Some(open) => {
                if character == open {
                    quote = None;
                } else {
                    current.push(character);
                }
            }
            None if character == '"' || character == '\'' => {
                quote = Some(character);
                quoted = true;
            }
            None if character.is_whitespace() => {
                if quoted || !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                    quoted = false;
                }
            }
            None => current.push(character),
        }
    }
    if quote.is_some() {
        return None;
    }
    if quoted || !current.is_empty() {
        tokens.push(current);
    }
    Some(tokens)
}

pub fn parse_mcp_snippet(code: &str) -> Result<Value, String> {
    let trimmed = code.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed)
        .or_else(|_| serde_json::from_str::<Value>(&format!("{{{trimmed}}}")))
    {
        if let Some(entry) = server_entry(&value) {
            return Ok(entry);
        }
        if has_endpoint(&value) {
            return Ok(value);
        }
    }
    // 站点上不少条目给的是 CLI 片段（`npx mcp-remote <url>`、`claude mcp add …`），
    // 它们同样能折成可写入 mcp.json 的 url/command 配置。
    if let Some(config) = cli_snippet_config(trimmed) {
        return Ok(config);
    }
    Err(msg("market.agentic_mcp_config_unparsed"))
}

fn has_endpoint(value: &Value) -> bool {
    value.is_object() && (value.get("command").is_some() || value.get("url").is_some())
}

fn server_entry(value: &Value) -> Option<Value> {
    for key in ["mcpServers", "servers"] {
        let Some(servers) = value.get(key).and_then(Value::as_object) else {
            continue;
        };
        if let Some(entry) = servers.values().find(|entry| has_endpoint(entry)) {
            return Some(entry.clone());
        }
        if let Some(entry) = servers.values().find(|entry| entry.is_object()) {
            return Some(entry.clone());
        }
    }
    None
}

fn request_html(url: &str) -> Result<String, String> {
    let mut response = http_agent().get(url).call().map_err(|error| {
        msg_with(
            "market.agenticskills_request_failed",
            &[("error", &error.to_string())],
        )
    })?;
    read_response(response.body_mut()).map_err(|error| {
        msg_with(
            "market.agenticskills_request_failed",
            &[("error", error.as_str())],
        )
    })
}

fn fetch_skill_catalog() -> Result<Vec<AgenticSkillEntry>, String> {
    let html = request_html(AGENTIC_SKILLS_URL)?;
    let entries = parse_skill_catalog(&html);
    if entries.is_empty() {
        return Err(msg("market.agenticskills_detail_missing"));
    }
    Ok(entries)
}

fn fetch_mcp_catalog() -> Result<Vec<AgenticMcpEntry>, String> {
    let html = request_html(AGENTIC_MCP_URL)?;
    let entries = parse_mcp_catalog(&html);
    if entries.is_empty() {
        return Err(msg("market.agenticskills_detail_missing"));
    }
    Ok(entries)
}

fn fetch_skill_detail(slug: &str) -> Result<AgenticSkillDetail, String> {
    let slug = validate_slug(slug)?.to_owned();
    let html = request_html(&format!("{AGENTIC_SKILLS_URL}/{slug}"))?;
    let detail =
        parse_skill_detail(&html).ok_or_else(|| msg("market.agenticskills_detail_missing"))?;
    if detail.slug != slug {
        return Err(msg("market.agenticskills_detail_missing"));
    }
    Ok(detail)
}

fn fetch_mcp_detail(slug: &str) -> Result<AgenticMcpDetail, String> {
    let slug = validate_slug(slug)?.to_owned();
    let html = request_html(&format!("{AGENTIC_MCP_URL}/{slug}"))?;
    let detail =
        parse_mcp_detail(&html).ok_or_else(|| msg("market.agenticskills_detail_missing"))?;
    if detail.slug != slug {
        return Err(msg("market.agenticskills_detail_missing"));
    }
    Ok(detail)
}

fn download_skill_markdown(source: &str) -> Result<String, String> {
    let url = Url::parse(source).map_err(|_| msg("market.agentic_skill_no_source"))?;
    if url.scheme() != "https" {
        return Err(msg("market.agentic_skill_no_source"));
    }
    let mut response = http_agent().get(source).call().map_err(|error| {
        msg_with(
            "market.agenticskills_request_failed",
            &[("error", &error.to_string())],
        )
    })?;
    read_response(response.body_mut()).map_err(|error| {
        msg_with(
            "market.agenticskills_request_failed",
            &[("error", error.as_str())],
        )
    })
}

fn cached_skill_catalog(cache: &AgenticCatalogCache) -> Result<Vec<AgenticSkillEntry>, String> {
    if let Some(entries) = cache.skills() {
        return Ok(entries);
    }
    let entries = fetch_skill_catalog()?;
    cache.store_skills(entries.clone());
    Ok(entries)
}

fn cached_mcp_catalog(cache: &AgenticCatalogCache) -> Result<Vec<AgenticMcpEntry>, String> {
    if let Some(entries) = cache.mcp() {
        return Ok(entries);
    }
    let entries = fetch_mcp_catalog()?;
    cache.store_mcp(entries.clone());
    Ok(entries)
}

fn filter_skills(entries: Vec<AgenticSkillEntry>, query: &str) -> Vec<AgenticSkillEntry> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return entries.into_iter().take(MAX_RESULTS).collect();
    }
    entries
        .into_iter()
        .filter(|entry| {
            [
                Some(entry.slug.as_str()),
                Some(entry.name.as_str()),
                entry.description.as_deref(),
                entry.author.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|value| value.to_lowercase().contains(&needle))
        })
        .take(MAX_RESULTS)
        .collect()
}

fn filter_mcp(entries: Vec<AgenticMcpEntry>, query: &str) -> Vec<AgenticMcpEntry> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return entries.into_iter().take(MAX_RESULTS).collect();
    }
    entries
        .into_iter()
        .filter(|entry| {
            [
                Some(entry.slug.as_str()),
                Some(entry.name.as_str()),
                entry.description.as_deref(),
                entry.author.as_deref(),
                entry.category.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|value| value.to_lowercase().contains(&needle))
        })
        .take(MAX_RESULTS)
        .collect()
}

/// 技能名 = 经过校验的 slug（字符集与 `agent_config::validate_entry_name` 兼容）。
fn skill_name_for(slug: &str) -> Result<&str, String> {
    let slug = validate_slug(slug)?;
    agent_config::validate_entry_name(slug)?;
    Ok(slug)
}

/// 服务名 = 请求里的 name（若给）否则 slug。
fn mcp_server_name(slug: &str, requested: Option<&str>) -> Result<String, String> {
    let slug = validate_slug(slug)?;
    let name = requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(slug);
    agent_config::validate_entry_name(name)?;
    Ok(name.to_owned())
}

fn truncate_bytes(value: &str, max_length: usize) -> String {
    let mut result = String::new();
    for character in value.chars() {
        if result.len() + character.len_utf8() > max_length {
            break;
        }
        result.push(character);
    }
    result
}

fn skill_description(detail: &AgenticSkillDetail, fallback: &str) -> String {
    match detail.description.as_deref().and_then(non_empty) {
        Some(description) if description.len() > MAX_SKILL_DESCRIPTION_LENGTH => {
            truncate_bytes(&description, MAX_SKILL_DESCRIPTION_LENGTH)
        }
        Some(description) => description,
        None => fallback.to_owned(),
    }
}

fn install_skill(paths: &AppPaths, request: &AgenticSkillInstall) -> Result<String, String> {
    let slug = validate_slug(&request.slug)?.to_owned();
    let name = skill_name_for(&slug)?.to_owned();
    let detail = fetch_skill_detail(&slug)?;
    let source = detail
        .skill_md_url
        .as_deref()
        .ok_or_else(|| msg("market.agentic_skill_no_source"))?;
    let content = download_skill_markdown(source)?;
    let description = skill_description(&detail, &name);
    agent_config::save_skill_content(
        paths,
        request.scope,
        request.project_path.as_deref(),
        &name,
        &description,
        &content,
    )?;
    Ok(name)
}

/// 从站点的多个配置片段里挑第一个能解析成服务器对象的片段。
fn config_from_snippets(snippets: &[AgenticMcpSnippet]) -> Result<Value, String> {
    if snippets.is_empty() {
        return Err(msg("market.agentic_mcp_no_config"));
    }
    snippets
        .iter()
        .find_map(|snippet| parse_mcp_snippet(&snippet.code).ok())
        .ok_or_else(|| msg("market.agentic_mcp_config_unparsed"))
}

fn install_mcp(paths: &AppPaths, request: &AgenticMcpInstall) -> Result<String, String> {
    let slug = validate_slug(&request.slug)?.to_owned();
    let detail = fetch_mcp_detail(&slug)?;
    let config = config_from_snippets(&detail.snippets)?;
    let name = mcp_server_name(&slug, request.name.as_deref())?;
    agent_config::save_mcp_server_config(
        paths,
        request.scope,
        request.project_path.as_deref(),
        &name,
        &config,
    )?;
    Ok(name)
}

#[tauri::command]
pub async fn search_agentic_skills(
    app: tauri::AppHandle,
    request: AgenticQuery,
) -> Result<Vec<AgenticSkillEntry>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let cache = app.state::<AgenticCatalogCache>();
        let entries = cached_skill_catalog(&cache)?;
        Ok(filter_skills(entries, &request.query))
    })
    .await
    .map_err(|error| format!("agentic skill search worker failed: {error}"))?
}

#[tauri::command]
pub async fn agentic_skill_detail(request: AgenticSlug) -> Result<AgenticSkillDetail, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_skill_detail(&request.slug))
        .await
        .map_err(|error| format!("agentic skill detail worker failed: {error}"))?
}

#[tauri::command]
pub async fn install_agentic_skill(
    paths: State<'_, AppPaths>,
    request: AgenticSkillInstall,
) -> Result<String, String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || install_skill(&paths, &request))
        .await
        .map_err(|error| format!("agentic skill install worker failed: {error}"))?
}

#[tauri::command]
pub async fn search_agentic_mcp(
    app: tauri::AppHandle,
    request: AgenticQuery,
) -> Result<Vec<AgenticMcpEntry>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let cache = app.state::<AgenticCatalogCache>();
        let entries = cached_mcp_catalog(&cache)?;
        Ok(filter_mcp(entries, &request.query))
    })
    .await
    .map_err(|error| format!("agentic MCP search worker failed: {error}"))?
}

#[tauri::command]
pub async fn agentic_mcp_detail(request: AgenticSlug) -> Result<AgenticMcpDetail, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_mcp_detail(&request.slug))
        .await
        .map_err(|error| format!("agentic MCP detail worker failed: {error}"))?
}

#[tauri::command]
pub async fn install_agentic_mcp(
    paths: State<'_, AppPaths>,
    request: AgenticMcpInstall,
) -> Result<String, String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || install_mcp(&paths, &request))
        .await
        .map_err(|error| format!("agentic MCP install worker failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::{
        agent_config, config_from_snippets, filter_mcp, filter_skills, mcp_server_name,
        parse_mcp_catalog, parse_mcp_detail, parse_mcp_snippet, parse_skill_catalog,
        parse_skill_detail, skill_description, skill_name_for, truncate_bytes, validate_slug,
        AgenticMcpInstall, AgenticMcpSnippet, AgenticSkillInstall, ConfigScope,
    };
    use crate::app_paths::AppPaths;
    use crate::message::message_code;
    use std::fs;
    use std::path::PathBuf;

    const SKILL_CATALOG_HTML: &str = r#"
      <html><body>
      <script type="application/ld+json">[{"@context":"https://schema.org","@type":"CollectionPage",
        "name":"AI Agent Skills - The Curated Directory","url":"https://agenticskills.io/skills",
        "mainEntity":{"@type":"ItemList","numberOfItems":3,"itemListElement":[
          {"@type":"ListItem","position":1,"url":"https://agenticskills.io/skills/taste-skill","name":"Taste Skill"},
          {"@type":"ListItem","position":2,"url":"https://agenticskills.io/skills/seo-aeo-best-practices","name":"SEO &amp; AEO Best Practices"},
          {"@type":"ListItem","position":3,"url":"https://agenticskills.io/skills/emil-design-eng","name":"Emil Design Engineering"}
        ]}}]</script>
      </body></html>
    "#;

    const SKILL_CARD_HTML: &str = r#"
      <a class="group block" href="/skills/taste-skill"><div class="relative h-full p-6 border-r border-b border-[var(--line)] bg-white">
      <div class="absolute top-4 right-4"><span class="as-ink-pill px-2.5 py-1">Featured</span></div>
      <div class="flex items-center gap-2 mb-1"><h3 class="text-base font-semibold text-[var(--ink)] font-sans transition-colors">Taste Skill</h3></div>
      <p class="text-xs text-[var(--mute-2)] mb-1">by <!-- -->Leonxlnx</p>
      <p class="text-sm text-[var(--mute-1)] leading-relaxed mt-3 mb-5 line-clamp-2">Anti-slop frontend skill that infers a design direction from the brief.</p>
      <div class="flex flex-wrap gap-1.5 mb-5"><span class="text-[10px] font-mono uppercase tracking-[0.08em] text-[var(--mute-2)] bg-[var(--paper-3)] px-2 py-0.5 border border-[var(--line)]">Claude Code</span><span class="text-[10px] font-mono uppercase tracking-[0.08em] text-[var(--mute-2)] bg-[var(--paper-3)] px-2 py-0.5 border border-[var(--line)]">Multi</span></div>
      <div class="flex items-center justify-between pt-4 border-t border-[var(--line)]"><div class="flex items-center gap-4"><span class="flex items-center gap-1.5 text-xs text-[var(--mute-2)]"><svg class="lucide lucide-star w-3.5 h-3.5" aria-hidden="true"><path d="M11.525 2.295"></path></svg>87.0K</span><span class="text-[10px] font-mono uppercase tracking-[0.12em] text-white bg-[var(--ink)] px-2 py-0.5 border border-[var(--ink)]">S<!-- -->-rank</span></div></div>
      </div></a>
    "#;

    const MCP_CATALOG_HTML: &str = r#"
      <script type="application/ld+json">{"@context":"https://schema.org","@type":"CollectionPage",
        "url":"https://agenticskills.io/mcp","mainEntity":{"@type":"ItemList","numberOfItems":2,"itemListElement":[
          {"@type":"ListItem","position":1,"url":"https://agenticskills.io/mcp/notion","name":"Notion"},
          {"@type":"ListItem","position":2,"url":"https://agenticskills.io/mcp/markitdown","name":"markitdown"}
        ]}}</script>
      <a class="group block h-full" href="/mcp/notion"><div class="relative h-full p-6 bg-white flex flex-col">
      <div class="flex items-start justify-between gap-4 mb-3"><div class="min-w-0 flex-1"><div class="flex items-center gap-2"><h3 class="text-base font-semibold text-[var(--ink)] font-sans">Notion</h3></div>
      <p class="text-xs text-[var(--mute-2)] mt-1">by <!-- -->Notion</p></div>
      <div class="flex shrink-0 flex-col items-end gap-1.5"><span class="as-ink-pill px-2.5 py-1">Featured</span><span class="text-[10px] font-mono uppercase tracking-[0.12em] px-2 py-0.5 border flex items-center gap-1 bg-[var(--ink)] text-white border-[var(--ink)]"><svg class="lucide lucide-shield-check w-3 h-3"></svg>Official</span></div></div>
      <p class="text-sm text-[var(--mute-1)] leading-relaxed mb-5 line-clamp-2">Pages, databases, search, and comments via Notion&#x27;s hosted MCP.</p>
      <div class="flex flex-wrap gap-1.5 mb-5"><span class="text-[10px] font-mono uppercase tracking-[0.08em] text-[var(--mute-2)] bg-[var(--paper-3)] px-2 py-0.5 border border-[var(--line)] flex items-center gap-1"><svg class="lucide lucide-plug w-2.5 h-2.5"></svg>Streamable HTTP</span><span class="text-[10px] font-mono uppercase tracking-[0.08em] text-[var(--mute-1)] bg-[var(--paper-3)] px-2 py-0.5 border border-[var(--line)] flex items-center gap-0.5"><svg class="lucide lucide-key w-2.5 h-2.5"></svg>API Key</span><span class="text-[10px] font-mono uppercase tracking-[0.08em] text-[var(--mute-1)] bg-white px-2 py-0.5 border border-[var(--line)]">Productivity &amp; PM</span></div>
      <div class="flex items-center justify-between pt-4 border-t border-[var(--line)] mt-auto"><div class="flex items-center gap-4"><span class="text-xs text-[var(--mute-2)]">~36K visitors/wk</span></div></div>
      </div></a>
    "#;

    // 真实详情页片段（来自 agenticskills.io 样本，长文本已裁剪）。
    const SKILL_DETAIL_OBJECT: &str = r#"{"slug":"taste-skill","name":"Taste Skill","description":"Anti-slop frontend skill that infers a design direction from the brief and ships interfaces that don't look templated.","longDescription":"Ships as design-taste-frontend upstream. Built for landing pages, portfolios, and redesigns.","author":{"name":"Leonxlnx","username":"Leonxlnx","url":"https://github.com/Leonxlnx"},"category":"Design & UI/UX","tags":["design","frontend","landing-page","redesign"],"platforms":["claude-code","codex","cursor","multi-platform"],"githubUrl":"https://github.com/Leonxlnx/taste-skill/tree/main/skills/taste-skill","skillMdUrl":"https://raw.githubusercontent.com/Leonxlnx/taste-skill/main/skills/taste-skill/SKILL.md","quality":"S","isFeatured":true,"license":"MIT","lastUpdated":"2026-05-26","installCommand":"npx skills add Leonxlnx/taste-skill@design-taste-frontend","price":"open-source","addedAt":"2026-08-27"}"#;

    const MCP_DETAIL_OBJECT: &str = r#"{"slug":"github","name":"GitHub","description":"Full GitHub API - repos, issues, PRs, CI/CD, code search, and Dependabot alerts.","longDescription":"The official GitHub MCP server provides comprehensive access to the GitHub platform.","author":{"name":"GitHub","url":"https://github.com"},"category":"Developer Tools","isOfficial":true,"trustLevel":"official","githubStars":27000,"githubUrl":"https://github.com/github/github-mcp-server","websiteUrl":"https://github.com","transport":["stdio","Streamable HTTP"],"requiresApiKey":true,"isFeatured":true,"tags":["git","repos","issues","pull-requests","ci-cd","code-search"],"language":"Go","configSource":"https://github.com/github/github-mcp-server#readme","config":[{"label":"VS Code - remote (OAuth)","file":".vscode/mcp.json","code":"{\n  \"servers\": {\n    \"github\": {\n      \"type\": \"http\",\n      \"url\": \"https://api.githubcopilot.com/mcp/\"\n    }\n  }\n}"},{"label":"Claude Code - local (Docker)","code":"claude mcp add github -e GITHUB_PERSONAL_ACCESS_TOKEN=$GITHUB_PAT -- docker run -i --rm ghcr.io/github/github-mcp-server"}]}"#;

    /// 用真实的 flight 推送形态包住详情对象，避免测试里手写转义表。
    fn flight_html(payload: &str) -> String {
        let literal = serde_json::to_string(payload).expect("flight payload should serialize");
        format!("<html><body><script>self.__next_f.push([1,{literal}])</script></body></html>")
    }

    #[test]
    fn parses_skill_catalog_from_json_ld() {
        let entries = parse_skill_catalog(SKILL_CATALOG_HTML);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].slug, "taste-skill");
        assert_eq!(entries[0].name, "Taste Skill");
        assert_eq!(entries[0].description, None);
        assert_eq!(entries[1].slug, "seo-aeo-best-practices");
        assert_eq!(entries[1].name, "SEO & AEO Best Practices");
        assert_eq!(entries[2].slug, "emil-design-eng");
    }

    #[test]
    fn merges_skill_card_metadata_into_catalog() {
        let html = format!("{SKILL_CATALOG_HTML}{SKILL_CARD_HTML}");
        let entries = parse_skill_catalog(&html);

        assert_eq!(entries.len(), 3);
        let entry = &entries[0];
        assert_eq!(entry.name, "Taste Skill");
        assert_eq!(entry.author.as_deref(), Some("Leonxlnx"));
        assert_eq!(
            entry.description.as_deref(),
            Some("Anti-slop frontend skill that infers a design direction from the brief.")
        );
        assert_eq!(entry.tags, vec!["Claude Code", "Multi"]);
        assert_eq!(entry.installs.as_deref(), Some("87.0K"));
        assert_eq!(entry.quality.as_deref(), Some("S"));
        assert!(entry.featured);
        assert_eq!(entries[1].author, None);
    }

    #[test]
    fn parses_mcp_catalog_cards() {
        let entries = parse_mcp_catalog(MCP_CATALOG_HTML);

        assert_eq!(entries.len(), 2);
        let entry = &entries[0];
        assert_eq!(entry.slug, "notion");
        assert_eq!(entry.name, "Notion");
        assert_eq!(entry.author.as_deref(), Some("Notion"));
        assert_eq!(entry.category.as_deref(), Some("Productivity & PM"));
        assert_eq!(entry.transport, vec!["Streamable HTTP"]);
        assert!(entry.official);
        assert!(entry.requires_api_key);
        assert_eq!(entry.popularity.as_deref(), Some("~36K visitors/wk"));
        assert!(entry.featured);
        assert_eq!(
            entry.description.as_deref(),
            Some("Pages, databases, search, and comments via Notion's hosted MCP.")
        );
        assert!(!entries[1].official);
        assert_eq!(entries[1].description, None);
    }

    #[test]
    fn parses_skill_detail_from_flight_data() {
        let html = flight_html(&format!(
            "3:{{\"skill\":{SKILL_DETAIL_OBJECT},\"related\":[]}}"
        ));

        let detail = parse_skill_detail(&html).expect("skill detail should parse");

        assert_eq!(detail.slug, "taste-skill");
        assert_eq!(detail.name, "Taste Skill");
        assert_eq!(detail.author.as_deref(), Some("Leonxlnx"));
        assert_eq!(
            detail.author_url.as_deref(),
            Some("https://github.com/Leonxlnx")
        );
        assert_eq!(detail.license.as_deref(), Some("MIT"));
        assert_eq!(detail.quality.as_deref(), Some("S"));
        assert_eq!(detail.last_updated.as_deref(), Some("2026-05-26"));
        assert_eq!(
            detail.github_url.as_deref(),
            Some("https://github.com/Leonxlnx/taste-skill/tree/main/skills/taste-skill")
        );
        assert_eq!(
            detail.skill_md_url.as_deref(),
            Some("https://raw.githubusercontent.com/Leonxlnx/taste-skill/main/skills/taste-skill/SKILL.md")
        );
        assert_eq!(
            detail.platforms,
            vec!["claude-code", "codex", "cursor", "multi-platform"]
        );
        assert_eq!(
            detail.tags,
            vec!["design", "frontend", "landing-page", "redesign"]
        );
        assert_eq!(
            detail.source_url,
            "https://agenticskills.io/skills/taste-skill"
        );
        assert_eq!(
            detail.install_command.as_deref(),
            Some("npx skills add Leonxlnx/taste-skill@design-taste-frontend")
        );
    }

    #[test]
    fn parses_mcp_detail_from_flight_data() {
        let html = flight_html(&format!(
            "3:{{\"server\":{MCP_DETAIL_OBJECT},\"related\":[]}}"
        ));

        let detail = parse_mcp_detail(&html).expect("MCP detail should parse");

        assert_eq!(detail.slug, "github");
        assert_eq!(detail.name, "GitHub");
        assert_eq!(detail.author.as_deref(), Some("GitHub"));
        assert_eq!(detail.author_url.as_deref(), Some("https://github.com"));
        assert_eq!(detail.category.as_deref(), Some("Developer Tools"));
        assert!(detail.official);
        assert_eq!(detail.trust_level.as_deref(), Some("official"));
        assert!(detail.requires_api_key);
        assert_eq!(detail.transport, vec!["stdio", "Streamable HTTP"]);
        assert_eq!(detail.popularity, None);
        assert_eq!(detail.source_url, "https://agenticskills.io/mcp/github");
        assert_eq!(detail.snippets.len(), 2);
        assert_eq!(detail.snippets[0].label, "VS Code - remote (OAuth)");
        assert_eq!(detail.snippets[0].file.as_deref(), Some(".vscode/mcp.json"));
        assert!(detail.snippets[0]
            .code
            .contains("\"url\": \"https://api.githubcopilot.com/mcp/\""));
        assert!(detail.snippets[1].code.starts_with("claude mcp add github"));
    }

    #[test]
    fn parses_mcp_detail_without_config_snippets() {
        let html = flight_html(
            "3:{\"server\":{\"slug\":\"notion\",\"name\":\"Notion\",\"isOfficial\":true,\"configSource\":\"https://developers.notion.com/docs/get-started-with-mcp\"},\"related\":[]}",
        );

        let detail = parse_mcp_detail(&html).expect("MCP detail should parse");

        assert!(detail.snippets.is_empty());
        assert_eq!(
            detail.config_source.as_deref(),
            Some("https://developers.notion.com/docs/get-started-with-mcp")
        );
    }

    #[test]
    fn missing_flight_data_is_none() {
        assert_eq!(
            parse_skill_detail("<html><body>no data</body></html>"),
            None
        );
        assert_eq!(parse_mcp_detail("<html><body>no data</body></html>"), None);
        assert!(parse_skill_catalog("<html><body>no json ld</body></html>").is_empty());
        assert!(parse_mcp_catalog("<html><html><body>no json ld</body></html>").is_empty());
    }

    #[test]
    fn picks_first_parsable_config_snippet() {
        assert_eq!(
            message_code(&config_from_snippets(&[]).expect_err("empty snippet list")).as_deref(),
            Some("market.agentic_mcp_no_config")
        );
        let cli_only = vec![AgenticMcpSnippet {
            label: "Claude Code".to_owned(),
            file: None,
            code: "claude mcp add playwright npx @playwright/mcp@latest".to_owned(),
        }];
        let cli_config = config_from_snippets(&cli_only).expect("CLI snippet should parse");
        assert_eq!(cli_config["command"], "npx");
        assert_eq!(cli_config["args"][0], "@playwright/mcp@latest");
        let docs_only = vec![AgenticMcpSnippet {
            label: "Docs".to_owned(),
            file: None,
            code: "Read the setup guide at https://example.com/setup".to_owned(),
        }];
        assert_eq!(
            message_code(&config_from_snippets(&docs_only).expect_err("docs only")).as_deref(),
            Some("market.agentic_mcp_config_unparsed")
        );
        let mixed = vec![
            docs_only[0].clone(),
            AgenticMcpSnippet {
                label: "Standard config (most clients)".to_owned(),
                file: None,
                code: "{\"mcpServers\": {\"playwright\": {\"command\": \"npx\", \"args\": [\"@playwright/mcp@latest\"]}}}"
                    .to_owned(),
            },
        ];
        let config = config_from_snippets(&mixed).expect("second snippet should parse");
        assert_eq!(config["command"], "npx");
        assert_eq!(config["args"][0], "@playwright/mcp@latest");
    }

    #[test]
    fn parses_mcp_snippet_forms() {
        let bare = parse_mcp_snippet(
            "\"mcpServers\": { \"git\": { \"command\": \"uvx\", \"args\": [\"mcp-server-git\", \"--repository\", \"path/to/git/repo\"] } }",
        )
        .expect("symbol-only snippet should parse");
        assert_eq!(bare["command"], "uvx");
        assert_eq!(bare["args"][0], "mcp-server-git");
        assert!(bare.get("mcpServers").is_none());

        let wrapped = parse_mcp_snippet(
            "{\"mcpServers\": {\"playwright\": {\"command\": \"npx\", \"args\": [\"@playwright/mcp@latest\"]}}}",
        )
        .expect("wrapped snippet should parse");
        assert_eq!(wrapped["command"], "npx");
        assert_eq!(wrapped["args"][0], "@playwright/mcp@latest");

        let vscode = parse_mcp_snippet(
            "{\"inputs\": [], \"servers\": {\"github\": {\"type\": \"http\", \"url\": \"https://api.githubcopilot.com/mcp/\"}}}",
        )
        .expect("VS Code style snippet should parse");
        assert_eq!(vscode["url"], "https://api.githubcopilot.com/mcp/");

        // CLI 片段（终端里复制粘贴的形式）也要能折成 mcp 配置，占位符原样保留。
        let cli = parse_mcp_snippet(
            "claude mcp add github -e TOKEN=$PAT -- docker run ghcr.io/github/github-mcp-server",
        )
        .expect("CLI snippet should parse");
        assert_eq!(cli["command"], "docker");
        assert_eq!(cli["args"][0], "run");
        assert_eq!(cli["env"]["TOKEN"], "$PAT");

        let remote = parse_mcp_snippet("npx mcp-remote https://mcp.linear.app/mcp")
            .expect("mcp-remote snippet should parse");
        assert_eq!(remote["command"], "npx");
        assert_eq!(remote["args"][0], "mcp-remote");
        assert_eq!(remote["args"][1], "https://mcp.linear.app/mcp");

        let remote_cli = parse_mcp_snippet(
            "claude mcp add --scope user --header \"Authorization: Bearer YOUR_API_KEY\" --transport http context7 https://mcp.context7.com/mcp",
        )
        .expect("remote CLI snippet should parse");
        assert_eq!(remote_cli["url"], "https://mcp.context7.com/mcp");
        assert_eq!(
            remote_cli["headers"]["Authorization"],
            "Bearer YOUR_API_KEY"
        );

        let docs = parse_mcp_snippet("Read the setup guide at https://example.com/setup")
            .expect_err("docs-only snippet should fail");
        assert_eq!(
            message_code(&docs),
            Some("market.agentic_mcp_config_unparsed")
        );
    }

    #[test]
    fn validates_agentic_slugs() {
        assert_eq!(validate_slug("taste-skill"), Ok("taste-skill"));
        assert_eq!(validate_slug("  github2  "), Ok("github2"));
        assert_eq!(
            message_code(&validate_slug("TasteSkill").expect_err("uppercase is invalid"))
                .as_deref(),
            Some("market.agenticskills_slug_invalid")
        );
        assert!(validate_slug("category/productivity").is_err());
        assert!(validate_slug("").is_err());
        assert!(validate_slug("../escape").is_err());
        assert!(validate_slug(&"a".repeat(65)).is_err());
    }

    #[test]
    fn derives_install_names_from_slug_and_request() {
        assert_eq!(skill_name_for("taste-skill"), Ok("taste-skill"));
        assert!(skill_name_for("Taste Skill").is_err());
        assert_eq!(
            mcp_server_name("context7", None).expect("slug name"),
            "context7"
        );
        assert_eq!(
            mcp_server_name("github", Some("  GitHub-MCP  ")).expect("custom name"),
            "GitHub-MCP"
        );
        assert_eq!(
            message_code(
                &mcp_server_name("github", Some("bad name")).expect_err("spaces rejected")
            )
            .as_deref(),
            Some("agent_config.name_charset")
        );
    }

    #[test]
    fn trims_skill_description_for_frontmatter() {
        assert_eq!(truncate_bytes("ascii", 5), "ascii");
        assert_eq!(truncate_bytes("abcdef", 4), "abcd");
        let long = "x".repeat(600);
        let detail = super::AgenticSkillDetail {
            description: Some(long),
            ..super::AgenticSkillDetail::default()
        };
        assert_eq!(skill_description(&detail, "fallback").len(), 512);
        let empty = super::AgenticSkillDetail::default();
        assert_eq!(skill_description(&empty, "fallback"), "fallback");
    }

    #[test]
    fn filters_entries_locally_by_query() {
        let entries = parse_skill_catalog(SKILL_CATALOG_HTML);
        assert_eq!(filter_skills(entries.clone(), "").len(), 3);
        assert_eq!(filter_skills(entries.clone(), "EMIL").len(), 1);
        assert_eq!(filter_skills(entries.clone(), "seo & aeo").len(), 1);
        assert_eq!(filter_skills(entries.clone(), "taste").len(), 1);
        assert!(filter_skills(entries, "missing").is_empty());

        let mcp = parse_mcp_catalog(MCP_CATALOG_HTML);
        assert_eq!(filter_mcp(mcp.clone(), "notion").len(), 1);
        assert_eq!(filter_mcp(mcp.clone(), "productivity & pm").len(), 1);
        assert_eq!(filter_mcp(mcp, "").len(), 2);
    }

    fn temp_paths(label: &str) -> (AppPaths, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "deeppi-agenticskills-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp root should be created");
        let paths = AppPaths::from_roots(
            root.join("roaming"),
            root.join("local"),
            root.join("project"),
        )
        .expect("app paths should build");
        (paths, root)
    }

    #[test]
    fn installs_local_skill_content_with_reused_validation() {
        let (paths, root) = temp_paths("skill");
        let request = AgenticSkillInstall {
            slug: "demo-skill".to_owned(),
            scope: ConfigScope::Global,
            project_path: None,
        };
        let name = skill_name_for(&request.slug).expect("slug should validate");
        agent_config::save_skill_content(
            &paths,
            request.scope,
            request.project_path.as_deref(),
            name,
            "Demo description",
            "# Demo\n\nBody without frontmatter.\n",
        )
        .expect("skill should be written");

        let file = paths.pi_home.join("skills/demo-skill/SKILL.md");
        let written = fs::read_to_string(&file).expect("SKILL.md should exist");
        assert!(written.starts_with("---\nname: demo-skill\ndescription: Demo description\n---\n"));
        assert!(written.contains("Body without frontmatter."));

        let error = agent_config::save_skill_content(
            &paths,
            ConfigScope::Global,
            None,
            "demo-skill",
            "Demo description",
            &"x".repeat(agent_config::MAX_SKILL_CONTENT_BYTES + 1),
        )
        .expect_err("oversized skill content should fail");
        assert_eq!(
            message_code(&error),
            Some("agent_config.skill_content_length")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installs_snippet_into_project_mcp_json() {
        let (paths, root) = temp_paths("mcp");
        fs::create_dir_all(root.join("project/.pi")).expect(".pi should be created");
        let project = root.join("project").to_string_lossy().to_string();
        let request = AgenticMcpInstall {
            slug: "playwright".to_owned(),
            name: None,
            scope: ConfigScope::Project,
            project_path: Some(project),
        };
        let config = parse_mcp_snippet(
            "{\"mcpServers\": {\"playwright\": {\"command\": \"npx\", \"args\": [\"@playwright/mcp@latest\"]}}}",
        )
        .expect("snippet should parse");
        let name = mcp_server_name(&request.slug, request.name.as_deref()).expect("name");
        agent_config::save_mcp_server_config(
            &paths,
            request.scope,
            request.project_path.as_deref(),
            &name,
            &config,
        )
        .expect("mcp server should be written");

        let content = fs::read_to_string(root.join("project/.pi/mcp.json")).expect("mcp.json");
        let value: serde_json::Value = serde_json::from_str(&content).expect("mcp.json is JSON");
        assert_eq!(value["mcpServers"]["playwright"]["command"], "npx");
        assert_eq!(
            value["mcpServers"]["playwright"]["args"][0],
            "@playwright/mcp@latest"
        );

        let error = parse_mcp_snippet("npx ctx7 setup").expect_err("shell one-liner is not JSON");
        assert_eq!(
            message_code(&error),
            Some("market.agentic_mcp_config_unparsed")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore = "requires network access to agenticskills.io"]
    fn fetches_live_agenticskills_catalogs_and_details() {
        let skills = super::fetch_skill_catalog().expect("skill catalog should be reachable");
        assert!(skills.len() > 100, "expected a full skill catalog");
        let mcp = super::fetch_mcp_catalog().expect("MCP catalog should be reachable");
        assert!(mcp.len() > 100, "expected a full MCP catalog");
        let detail = super::fetch_skill_detail("taste-skill").expect("skill detail");
        assert!(detail.skill_md_url.is_some());
        let detail = super::fetch_mcp_detail("playwright").expect("MCP detail");
        assert!(!detail.snippets.is_empty());
    }
}
