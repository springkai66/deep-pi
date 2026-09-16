//! AgenticSkills 市场（https://agenticskills.io）后端。
//!
//! 站点是 Next.js 应用，没有公开 JSON API，数据来源有三处：
//! - 列表页引用的数据 chunk（`/_next/static/immutable/chunks/<hash>.js`）里内嵌了
//!   全量目录（技能 + MCP 服务器 + 仓库统计），字段最全且带 `category`。chunk 名带
//!   构建哈希，因此先抓列表页 HTML、再按文档顺序倒着试，命中后把 URL 记进缓存。
//! - 列表页里的 JSON-LD `<script type="application/ld+json">`：只有 slug + name，
//!   作为 chunk 不可用时的降级来源（无分类，只写短缓存）。
//! - 详情页把数据塞在 Next.js flight 数据 `self.__next_f.push([1,"…"])` 里，
//!   反转义后按平衡括号扫描出目标对象再用 serde_json 解析（详情命令与安装兜底）。
//! - 工作流（`/workflows`）不在数据 chunk 里：列表页用 JSON-LD `ItemList` 拿全量 slug + name、
//!   用 HTML 卡片补分类 / 难度 / 描述 / 计数；详情页用 JSON-LD `HowTo`（组件 + 步骤 + 起手提示词）。
//!   安装是「一个按钮装齐」：组件逐个走既有技能 / MCP 安装路径，已存在就跳过、失败不中断。
//! - 已安装的工作流记在 `<pi_home>/workflows.json`（项目范围在 `<project>/.pi/workflows.json`）。

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{State, Url};

use crate::{
    agent_config::{self, ConfigScope, MAX_SKILL_DESCRIPTION_LENGTH},
    app_paths::AppPaths,
    market::{decode_entities, http_agent, read_response, strip_tags},
    message::{msg, msg_with},
};

const SITE_ORIGIN: &str = "https://agenticskills.io";
const AGENTIC_SKILLS_URL: &str = "https://agenticskills.io/skills";
const AGENTIC_MCP_URL: &str = "https://agenticskills.io/mcp";
/// 列表页里 chunk 路径的前缀（数据 chunk 与普通组件 chunk 混在一起）。
const CHUNK_PATH_MARKER: &str = "/_next/static/";
const CACHE_TTL: Duration = Duration::from_secs(600);
/// 降级（JSON-LD）结果的短缓存：只挡同一轮界面刷新里的重复请求，很快会重试 chunk。
const FALLBACK_CACHE_TTL: Duration = Duration::from_secs(60);
/// chunk 扫描上限：最多抓 8 个候选、合计 3 MiB，站点改版时也不至于把整页 chunk 拉完。
const MAX_CHUNK_FETCHES: usize = 8;
const MAX_CHUNK_BYTES: usize = 3 * 1024 * 1024;
/// 单个候选对象字面量超过它就只跳过解析（内部对象仍会被扫到），避免去解析整个模块体。
const MAX_OBJECT_LITERAL_BYTES: usize = 128 * 1024;
/// 数据 chunk 至少要解析出这么多条目，普通组件 chunk 达不到这个量级。
const MIN_CHUNK_ENTRIES: usize = 10;
const MAX_RESULTS: usize = 200;
const MAX_SLUG_LENGTH: usize = 64;
/// 工作流列表页（打包好的「技能 + MCP」集合）。
const AGENTIC_WORKFLOWS_URL: &str = "https://agenticskills.io/workflows";
/// 工作流结果上限：站点只有 20 多个，留足本地过滤后的空间。
const MAX_WORKFLOW_RESULTS: usize = 100;
/// 已安装工作流状态文件的上限：条目数与文件大小，防止手改的文件把界面 / 内存撑坏。
const MAX_INSTALLED_WORKFLOWS: usize = 64;
const MAX_WORKFLOW_FILE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticSkillEntry {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub platforms: Vec<String>,
    pub installs: Option<String>,
    /// 原始 star 数（排序用；`installs` 只是显示文本）。
    pub stars: Option<u64>,
    /// 热度统一口径：技能与 `stars` 相同。
    pub heat: Option<u64>,
    pub quality: Option<String>,
    pub license: Option<String>,
    pub last_updated: Option<String>,
    pub github_url: Option<String>,
    pub skill_md_url: Option<String>,
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
    pub long_description: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub transport: Vec<String>,
    pub official: bool,
    pub requires_api_key: bool,
    pub popularity: Option<String>,
    /// 原始 star 数（排序用）。
    pub stars: Option<u64>,
    /// 热度统一口径：`stars` 优先，否则从 `popularity` 文本里解析。
    pub heat: Option<u64>,
    pub website_url: Option<String>,
    pub config_source: Option<String>,
    pub tags: Vec<String>,
    pub snippets: Vec<AgenticMcpSnippet>,
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

/// 工作流列表条目：JSON-LD 给 slug + name，卡片补分类 / 难度 / 描述 / 组件计数。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticWorkflowEntry {
    pub slug: String,
    pub name: String,
    pub category: Option<String>,
    pub level: Option<String>,
    pub description: Option<String>,
    pub skill_count: u32,
    pub mcp_count: u32,
}

/// 工作流里的单个组件：`kind` 由站点 url 路径决定（`/skills/` → `skill`、`/mcp/` → `mcp`）。
/// 既随安装结果出参，也要能从 `workflows.json` 读回。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticWorkflowComponent {
    pub kind: String,
    pub slug: String,
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticWorkflowStep {
    pub name: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticWorkflowDetail {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub level: Option<String>,
    pub setup_time: Option<String>,
    pub components: Vec<AgenticWorkflowComponent>,
    pub steps: Vec<AgenticWorkflowStep>,
    pub kickoff_prompt: Option<String>,
    pub source_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgenticWorkflowInstall {
    pub slug: String,
    pub scope: ConfigScope,
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticWorkflowFailure {
    pub kind: String,
    pub slug: String,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticWorkflowInstallResult {
    pub workflow: String,
    pub skills: Vec<String>,
    pub mcp: Vec<String>,
    pub skipped: Vec<String>,
    pub failures: Vec<AgenticWorkflowFailure>,
}

/// 本地状态文件里的一条「已安装工作流」（`workflows.json` 的 value）。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledWorkflow {
    pub slug: String,
    pub name: String,
    pub category: Option<String>,
    pub skill_count: u32,
    pub mcp_count: u32,
    pub installed_at: String,
    pub components: Vec<AgenticWorkflowComponent>,
}

/// 状态文件里的原始记录：slug 是 map 的键，不重复存；字段缺省时按最宽松的方式读回。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledWorkflowRecord {
    #[serde(default)]
    name: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    skill_count: u32,
    #[serde(default)]
    mcp_count: u32,
    #[serde(default)]
    installed_at: String,
    #[serde(default)]
    components: Vec<AgenticWorkflowComponent>,
}

/// 数据 chunk 里解析出来的目录：技能、MCP 服务器与仓库统计（`repoStars`）。
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CatalogChunk {
    pub(crate) skills: Vec<AgenticSkillEntry>,
    pub(crate) mcp: Vec<AgenticMcpEntry>,
    /// slug → GitHub star 数，站点卡片上的 `87.0K` 就来自它。
    pub(crate) repo_stars: BTreeMap<String, u64>,
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

type CatalogSnapshot<T> = Option<(Instant, Vec<T>, bool)>;

/// 缓存内容：`(写入时间, 目录, 是否降级)`；降级结果只用短 TTL，好尽快重试 chunk。
fn fresh_entries<T: Clone>(snapshot: Option<&(Instant, Vec<T>, bool)>) -> Option<Vec<T>> {
    let (created, entries, degraded) = snapshot?;
    let ttl = if *degraded {
        FALLBACK_CACHE_TTL
    } else {
        CACHE_TTL
    };
    (created.elapsed() < ttl).then(|| entries.clone())
}

/// 列表页缓存：与 `market::McpRegistryCache` 同样的 TTL 策略。
///
/// chunk URL 也一起缓存：命中过一次数据 chunk 之后，同一进程内的后续请求可以直接
/// 抓它（chunk 名带构建哈希，站点发新版之前一直有效），省掉列表页 HTML。
#[derive(Default)]
pub struct AgenticCatalogCache {
    skills: Mutex<CatalogSnapshot<AgenticSkillEntry>>,
    mcp: Mutex<CatalogSnapshot<AgenticMcpEntry>>,
    chunk_url: Mutex<Option<(Instant, String)>>,
}

/// 工作流命令的签名不接收 `AppHandle`（前端已冻结），取不到托管的 `AgenticCatalogCache`，
/// 因此工作流安装自己持有一个进程级目录缓存：装一个工作流时技能与 MCP 组件共用同一份数据。
static WORKFLOW_CATALOG_CACHE: AgenticCatalogCache = AgenticCatalogCache::new();

/// 工作流列表缓存：形态与 `AgenticCatalogCache` 一致（`(写入时间, 结果, 是否降级)`，TTL 600s）。
static WORKFLOW_CACHE: Mutex<CatalogSnapshot<AgenticWorkflowEntry>> = Mutex::new(None);

impl AgenticCatalogCache {
    /// 供模块级 `static` 使用（`Default::default` 不是 `const`）。
    const fn new() -> Self {
        Self {
            skills: Mutex::new(None),
            mcp: Mutex::new(None),
            chunk_url: Mutex::new(None),
        }
    }

    fn skills(&self) -> Option<Vec<AgenticSkillEntry>> {
        let guard = self.skills.lock().ok()?;
        fresh_entries(guard.as_ref())
    }

    fn store_skills(&self, entries: Vec<AgenticSkillEntry>, degraded: bool) {
        if let Ok(mut guard) = self.skills.lock() {
            *guard = Some((Instant::now(), entries, degraded));
        }
    }

    fn mcp(&self) -> Option<Vec<AgenticMcpEntry>> {
        let guard = self.mcp.lock().ok()?;
        fresh_entries(guard.as_ref())
    }

    fn store_mcp(&self, entries: Vec<AgenticMcpEntry>, degraded: bool) {
        if let Ok(mut guard) = self.mcp.lock() {
            *guard = Some((Instant::now(), entries, degraded));
        }
    }

    fn chunk_url(&self) -> Option<String> {
        let guard = self.chunk_url.lock().ok()?;
        let (created, url) = guard.as_ref()?;
        (created.elapsed() < CACHE_TTL).then(|| url.clone())
    }

    fn store_chunk_url(&self, url: &str) {
        if let Ok(mut guard) = self.chunk_url.lock() {
            *guard = Some((Instant::now(), url.to_owned()));
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

// ── 数据 chunk（`/_next/static/**/*.js`）───────────────────────────────────────
//
// chunk 里内嵌了整份目录（技能 + MCP 服务器 + 仓库统计），形如 JS 对象字面量：
//   {slug:"taste-skill",…,skillMdUrl:"…",isFeatured:!0}
// 键不加引号，布尔是 `!0`/`!1`，字符串会出现双引号、单引号与模板字符串（配置片段）。
// 做法：字符串感知地扫出候选对象字面量 → 逐个转 JSON → serde 解析，解析失败的跳过。

/// 字符串感知的括号扫描：`brace` 处必须是 `{`，返回配平到对应 `}` 的切片。
///
/// 单引号与模板字符串也算字符串边界（目录 chunk 里的 `code:` 片段三种引号都用），
/// 所以纯 JSON 的 flight 数据路径也能安全复用。
fn object_literal_at(text: &str, brace: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.get(brace) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    let mut quote = 0u8;
    let mut index = brace;
    while index < bytes.len() {
        let byte = bytes[index];
        if quote != 0 {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if byte == quote {
                quote = 0;
            }
        } else {
            match byte {
                b'"' | b'\'' | b'`' => quote = byte,
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

fn is_js_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_js_ident(byte: u8) -> bool {
    is_js_ident_start(byte) || byte.is_ascii_digit()
}

fn utf8_char_length(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte >> 5 == 0b110 {
        2
    } else if byte >> 4 == 0b1110 {
        3
    } else if byte >> 3 == 0b1_1110 {
        4
    } else {
        1
    }
}

fn slice_starts_with(bytes: &[u8], index: usize, needle: &[u8]) -> bool {
    index + needle.len() <= bytes.len() && &bytes[index..index + needle.len()] == needle
}

/// `{`/`,` 之后的裸标识符键：返回标识符的 `(起点, 终点)`，只有后面紧跟 `:` 才算键。
fn js_key_at(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut index = start;
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    let key_start = index;
    if !bytes.get(index).copied().is_some_and(is_js_ident_start) {
        return None;
    }
    while bytes.get(index).copied().is_some_and(is_js_ident) {
        index += 1;
    }
    let key_end = index;
    let mut colon = index;
    while bytes.get(colon).is_some_and(u8::is_ascii_whitespace) {
        colon += 1;
    }
    (bytes.get(colon) == Some(&b':')).then_some((key_start, key_end))
}

/// 数字字面量的结束位置（含 `46e3` 这种指数写法，JSON 不接受指数）。
fn js_number_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    let digits_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == digits_start {
        return None;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if matches!(bytes.get(index).copied(), Some(b'e' | b'E')) {
        let mut probe = index + 1;
        if matches!(bytes.get(probe).copied(), Some(b'+' | b'-')) {
            probe += 1;
        }
        let exponent_start = probe;
        while bytes.get(probe).is_some_and(u8::is_ascii_digit) {
            probe += 1;
        }
        if probe > exponent_start {
            index = probe;
        }
    }
    Some(index)
}

/// 把一个 JS 字符串字面量按 JSON 规则写进 `out`（`start` 处是引号），返回字面量之后的位置。
///
/// 单引号/模板字符串会换成双引号，里面的裸双引号补上转义；`\'`、`` \` ``、`\xHH`、
/// 真实换行这些 JS 专有转义也一并改写成 JSON 能接受的形式。
fn push_js_string(js: &str, start: usize, out: &mut Vec<u8>) -> usize {
    let bytes = js.as_bytes();
    let quote = bytes[start];
    out.push(b'"');
    let mut index = start + 1;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\\' {
            let Some(next) = bytes.get(index + 1).copied() else {
                break;
            };
            match next {
                b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {
                    out.extend_from_slice(&bytes[index..index + 2]);
                    index += 2;
                }
                b'u' => {
                    let end = (index + 6).min(bytes.len());
                    out.extend_from_slice(&bytes[index..end]);
                    index = end;
                }
                b'x' => {
                    out.extend_from_slice(b"\\u00");
                    let end = (index + 4).min(bytes.len());
                    out.extend_from_slice(&bytes[(index + 2).min(bytes.len())..end]);
                    index += 4;
                }
                b'v' => {
                    out.extend_from_slice(b"\\u000b");
                    index += 2;
                }
                b'0' => {
                    out.extend_from_slice(b"\\u0000");
                    index += 2;
                }
                b'\n' => index += 2,
                b'\r' => {
                    index += if bytes.get(index + 2) == Some(&b'\n') {
                        3
                    } else {
                        2
                    };
                }
                other => {
                    out.push(other);
                    index += 2;
                }
            }
            continue;
        }
        if byte == quote {
            out.push(b'"');
            return index + 1;
        }
        match byte {
            b'"' => {
                out.extend_from_slice(b"\\\"");
                index += 1;
            }
            b'\n' => {
                out.extend_from_slice(b"\\n");
                index += 1;
            }
            b'\r' => {
                out.extend_from_slice(b"\\r");
                index += 1;
            }
            b'\t' => {
                out.extend_from_slice(b"\\t");
                index += 1;
            }
            _ => {
                if byte < 0x20 {
                    out.extend_from_slice(format!("\\u{byte:04x}").as_bytes());
                    index += 1;
                } else {
                    let end = (index + utf8_char_length(byte)).min(bytes.len());
                    out.extend_from_slice(&bytes[index..end]);
                    index = end;
                }
            }
        }
    }
    // 引号不闭合：补一个引号收尾，交给 serde 判定失败。
    out.push(b'"');
    bytes.len()
}

/// JS 对象字面量 → JSON 文本（结果由调用方再用 serde 解析）。
///
/// - 裸标识符键补引号：只处理紧跟 `{`/`,` 且以 `:` 结尾的标识符；
/// - 字符串支持双引号、单引号与模板字符串；
/// - `!0`/`!1` → `true`/`false`，`undefined` → `null`；
/// - `46e3` 这类指数写法换成十进制（JSON 不接受指数）。
pub(crate) fn js_literals_to_json(js: &str) -> String {
    let bytes = js.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(js.len() + 16);
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' || byte == b'\'' || byte == b'`' {
            index = push_js_string(js, index, &mut out);
            continue;
        }
        if byte == b'!' && matches!(bytes.get(index + 1).copied(), Some(b'0' | b'1')) {
            // `!0` → true、`!1` → false（站点 chunk 里的布尔写法）。
            out.extend_from_slice(if bytes[index + 1] == b'0' {
                b"true"
            } else {
                b"false"
            });
            index += 2;
            continue;
        }
        if byte == b'{' || byte == b',' {
            out.push(byte);
            index += 1;
            if let Some((key_start, key_end)) = js_key_at(bytes, index) {
                out.push(b'"');
                out.extend_from_slice(&bytes[key_start..key_end]);
                out.push(b'"');
                index = key_end;
            }
            continue;
        }
        if byte == b'-' || byte.is_ascii_digit() {
            if let Some(end) = js_number_end(bytes, index) {
                let literal = &js[index..end];
                if literal.contains('e') || literal.contains('E') {
                    if let Ok(value) = literal.parse::<f64>() {
                        out.extend_from_slice(format!("{value}").as_bytes());
                    } else {
                        out.extend_from_slice(literal.as_bytes());
                    }
                } else {
                    out.extend_from_slice(literal.as_bytes());
                }
                index = end;
                continue;
            }
        }
        if slice_starts_with(bytes, index, b"undefined")
            && index
                .checked_sub(1)
                .map_or(true, |prev| !is_js_ident(bytes[prev]))
            && bytes
                .get(index + 9)
                .copied()
                .map_or(true, |next| !is_js_ident(next))
        {
            out.extend_from_slice(b"null");
            index += 9;
            continue;
        }
        let end = (index + utf8_char_length(byte)).min(bytes.len());
        out.extend_from_slice(&bytes[index..end]);
        index = end;
    }
    String::from_utf8(out).unwrap_or_default()
}

/// 逐个取出 JS 文本里的候选对象字面量；解析失败的直接跳过（不 panic）。
///
/// 扫描本身是字符串感知的，所以字符串里的花括号不会被当成对象开头。超过
/// `MAX_OBJECT_LITERAL_BYTES` 的对象只跳过解析，它内部的对象照样会被扫到。
fn scan_object_literals(js: &str) -> Vec<Value> {
    let bytes = js.as_bytes();
    let mut values: Vec<Value> = Vec::new();
    let mut quote = 0u8;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if quote != 0 {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if byte == quote {
                quote = 0;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' | b'\'' | b'`' => quote = byte,
            b'{' => {
                if let Some(text) = object_literal_at(js, index) {
                    if text.len() <= MAX_OBJECT_LITERAL_BYTES {
                        if let Ok(value) = serde_json::from_str::<Value>(&js_literals_to_json(text))
                        {
                            values.push(value);
                        }
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
    values
}

fn has_any_key(value: &Value, keys: &[&str]) -> bool {
    keys.iter().any(|key| value.get(*key).is_some())
}

/// 技能条目的标志字段（MCP 条目没有 `skillMdUrl`/`platforms`，两条规则互斥）。
fn is_skill_object(value: &Value) -> bool {
    json_string(value, "slug").is_some()
        && has_any_key(value, &["skillMdUrl", "platforms", "installCommand"])
}

/// MCP 条目的标志字段；结果数组与导航/分类对象（只有 slug + name）会被排除。
fn is_mcp_object(value: &Value) -> bool {
    json_string(value, "slug").is_some()
        && has_any_key(
            value,
            &[
                "transport",
                "configSource",
                "trustLevel",
                "config",
                "websiteUrl",
                "requiresApiKey",
                "language",
            ],
        )
}

/// 文本里的原始计数（热度排序用）：`~36K visitors/wk` → 36000、`1.2M` → 1200000。
///
/// 只认开头的 `\d+(\.\d+)?[KM]?` 形态（可带 `~` 前缀与 `,` 千分位，小数最多 6 位）；
/// 没有数字、负数、带非计数后缀（如 `46e3`）与超出 `u64` 的值一律 `None`——不用 0 冒充。
fn parse_count_text(text: &str) -> Option<u64> {
    /// 小数最多取这么多位：`10u128.pow` 与尾数拼接都不至于失控。
    const MAX_FRACTION_DIGITS: usize = 6;
    let trimmed = text.trim();
    let trimmed = trimmed.strip_prefix('~').unwrap_or(trimmed).trim_start();
    let bytes = trimmed.as_bytes();
    let mut index = 0usize;
    let mut digits = String::new();
    while let Some(byte) = bytes.get(index).copied() {
        if byte.is_ascii_digit() {
            digits.push(byte as char);
        } else if byte != b',' {
            break;
        }
        index += 1;
    }
    if digits.is_empty() {
        return None;
    }
    let mut fraction = String::new();
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while fraction.len() < MAX_FRACTION_DIGITS
            && bytes
                .get(index)
                .copied()
                .is_some_and(|byte| byte.is_ascii_digit())
        {
            fraction.push(bytes[index] as char);
            index += 1;
        }
    }
    let multiplier: u128 = match bytes.get(index).copied() {
        Some(b'K' | b'k') => {
            index += 1;
            1_000
        }
        Some(b'M' | b'm') => {
            index += 1;
            1_000_000
        }
        _ => 1,
    };
    // 指数写法（`46e3`）由 JS 字面量 → JSON 那条路径折成十进制，这里不认。
    if bytes
        .get(index)
        .copied()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    let mantissa = format!("{digits}{fraction}").parse::<u128>().ok()?;
    let scale = 10u128.pow(fraction.len() as u32);
    let value = mantissa.checked_mul(multiplier)? / scale;
    u64::try_from(value).ok()
}

/// JSON 字段里的原始计数：数字直接 `as_u64`，字符串按 `parse_count_text` 解析
/// （JS 字面量里的指数写法在 `js_literals_to_json` 时已经折成十进制）。
fn json_count(value: &Value, key: &str) -> Option<u64> {
    match value.get(key)? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => parse_count_text(text),
        _ => None,
    }
}

/// 复刻站点 `formatStarCount`：`>=1e6` → `1.2M`，`>=1e3` → `87.0K`，0 → 空串。
pub(crate) fn format_stars(stars: u64) -> String {
    if stars == 0 {
        return String::new();
    }
    if stars >= 1_000_000 {
        format!("{:.1}M", stars as f64 / 1_000_000.0)
    } else if stars >= 1_000 {
        format!("{:.1}K", stars as f64 / 1_000.0)
    } else {
        stars.to_string()
    }
}

fn skill_entry_from_object(value: &Value, repo_stars: &BTreeMap<String, u64>) -> AgenticSkillEntry {
    let slug = json_string(value, "slug").unwrap_or_default();
    let (author, _) = author_fields(value);
    // 原始 star 数：条目自身的 `githubStars` / `repoStars` 优先，其次仓库统计映射。
    let stars = json_count(value, "githubStars")
        .or_else(|| json_count(value, "repoStars"))
        .or_else(|| repo_stars.get(&slug).copied());
    AgenticSkillEntry {
        name: json_string(value, "name").unwrap_or_else(|| slug.clone()),
        slug,
        description: json_string(value, "description"),
        long_description: json_string(value, "longDescription"),
        author,
        category: json_string(value, "category"),
        tags: json_string_list(value, "tags"),
        platforms: json_string_list(value, "platforms"),
        installs: stars.map(format_stars).and_then(|text| non_empty(&text)),
        stars,
        heat: stars,
        quality: json_string(value, "quality"),
        license: json_string(value, "license"),
        last_updated: json_string(value, "lastUpdated"),
        github_url: json_string(value, "githubUrl"),
        skill_md_url: json_string(value, "skillMdUrl"),
        featured: json_bool(value, "isFeatured"),
    }
}

/// 站点条目里的 `config` 数组就是可写入 mcp.json 的片段。
fn mcp_snippets(value: &Value) -> Vec<AgenticMcpSnippet> {
    value
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
        .unwrap_or_default()
}

fn mcp_entry_from_object(value: &Value) -> AgenticMcpEntry {
    let slug = json_string(value, "slug").unwrap_or_default();
    let (author, _) = author_fields(value);
    let stars = json_count(value, "githubStars");
    // 热度统一口径：有星用星，否则取 `popularity` 文本里的第一个计数。
    let heat = stars.or_else(|| {
        json_string(value, "popularity")
            .as_deref()
            .and_then(parse_count_text)
    });
    AgenticMcpEntry {
        name: json_string(value, "name").unwrap_or_else(|| slug.clone()),
        slug,
        description: json_string(value, "description"),
        long_description: json_string(value, "longDescription"),
        author,
        category: json_string(value, "category"),
        transport: json_string_list(value, "transport"),
        official: json_bool(value, "isOfficial"),
        requires_api_key: json_bool(value, "requiresApiKey"),
        popularity: json_string(value, "popularity"),
        stars,
        heat,
        website_url: json_string(value, "websiteUrl"),
        config_source: json_string(value, "configSource"),
        tags: json_string_list(value, "tags"),
        snippets: mcp_snippets(value),
        featured: json_bool(value, "isFeatured"),
    }
}

/// 数据 chunk 里的「仓库统计」映射：`let s={"taste-skill":{repoStars:87002,…}}`。
fn collect_repo_stars(value: &Value, stars: &mut BTreeMap<String, u64>) {
    let Some(map) = value.as_object() else {
        return;
    };
    if map.contains_key("slug") {
        return;
    }
    for (key, entry) in map {
        let Some(count) = entry.get("repoStars").and_then(Value::as_u64) else {
            continue;
        };
        if validate_slug(key).is_ok() {
            stars.insert(key.clone(), count);
        }
    }
}

/// 解析数据 chunk：技能、MCP 服务器与仓库统计。
///
/// 只有带目录字段的对象才收（导航/分类/组件里的对象会被跳过），解析失败的对象
/// 全部忽略——chunk 里大部分花括号其实是普通 JS 代码。
pub(crate) fn parse_catalog_chunk(js: &str) -> CatalogChunk {
    let objects = scan_object_literals(js);
    let mut repo_stars: BTreeMap<String, u64> = BTreeMap::new();
    for value in &objects {
        collect_repo_stars(value, &mut repo_stars);
    }
    // 仓库统计在同一个 chunk 的另一段里，因此先收集再生成条目（`installs` 要用它）。
    let mut skills: Vec<AgenticSkillEntry> = Vec::new();
    let mut mcp: Vec<AgenticMcpEntry> = Vec::new();
    let mut seen_skills: HashSet<String> = HashSet::new();
    let mut seen_mcp: HashSet<String> = HashSet::new();
    for value in &objects {
        let Some(slug) = json_string(value, "slug") else {
            continue;
        };
        if is_skill_object(value) {
            if seen_skills.insert(slug) {
                skills.push(skill_entry_from_object(value, &repo_stars));
            }
        } else if is_mcp_object(value) && seen_mcp.insert(slug) {
            mcp.push(mcp_entry_from_object(value));
        }
    }
    CatalogChunk {
        skills,
        mcp,
        repo_stars,
    }
}

/// 从列表页 HTML 里按文档顺序取出全部 `/_next/static/**/*.js` 的绝对 URL（去重）。
///
/// Next 的 flight 数据里路径是 `\/_next\/…`，先把 `\/` 还原再扫；页面上绝对写法与
/// 相对写法会归一到同一个 URL（相对写法用站点域名补全）。
pub(crate) fn catalog_chunk_candidates(html: &str) -> Vec<String> {
    let normalized = html.replace("\\/", "/");
    let bytes = normalized.as_bytes();
    let mut urls: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative) = normalized[cursor..].find(CHUNK_PATH_MARKER) {
        let start = cursor + relative;
        cursor = start + CHUNK_PATH_MARKER.len();
        let mut end = start;
        while bytes.get(end).copied().is_some_and(is_chunk_url_byte) {
            end += 1;
        }
        let path = &normalized[start..end];
        if !path.ends_with(".js") {
            continue;
        }
        let origin = page_origin(&normalized[..start]).unwrap_or(SITE_ORIGIN);
        let url = format!("{origin}{path}");
        if !urls.contains(&url) {
            urls.push(url);
        }
    }
    urls
}

fn is_chunk_url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'~')
}

/// chunk 路径之前紧挨着的域名（HTML 里两种写法都有）；不是绝对 URL 时返回 None。
fn page_origin(prefix: &str) -> Option<&str> {
    ["https://", "http://"].into_iter().find_map(|scheme| {
        let index = prefix.rfind(scheme)?;
        let origin = &prefix[index..];
        let host = &origin[scheme.len()..];
        (!host.is_empty()
            && !host.chars().any(|character| {
                character.is_whitespace()
                    || matches!(character, '"' | '\'' | '<' | '>' | '/' | '\\')
            }))
        .then_some(origin)
    })
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

// ── 工作流列表页（`/workflows`）──────────────────────────────────────────────
//
// 工作流是打包好的技能集合（有时含 MCP 服务器），数据不在目录 chunk 里：
// JSON-LD `ItemList` 给全量 slug + name，HTML 卡片再补分类 / 难度 / 描述 / 计数。

fn is_workflow_level(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "beginner" | "intermediate" | "advanced"
    )
}

/// 卡片与详情页都有的「分类 / 难度」徽标：容器里的前两个 span。
/// 难度用已知取值兜底判定，顺序反过来时也能认出来。
fn workflow_badges(body: &str) -> (Option<String>, Option<String>) {
    let Some(at) = body.find("flex items-center gap-2 mb-") else {
        return (None, None);
    };
    let region = &body[at..];
    // 徽标容器里只有两个 span，后面第一个 `</div>` 就是它的收尾。
    let end = region.find("</div>").unwrap_or(region.len().min(1024));
    let badges: Vec<String> = pills(&region[..end])
        .into_iter()
        .filter_map(|pill| non_empty(&pill.text))
        .collect();
    match (badges.first().cloned(), badges.get(1).cloned()) {
        (Some(category), Some(level)) if is_workflow_level(&category) => {
            (Some(level), Some(category))
        }
        (category, level) => (category, level),
    }
}

/// 卡片底部的计数：`5 skills` / `4 MCP`（`<!-- -->` 注释会被 `strip_tags` 去掉）。
fn workflow_counts(body: &str) -> (u32, u32) {
    let mut skills = 0;
    let mut mcp = 0;
    for pill in pills(body) {
        let Some((number, label)) = pill.text.trim().split_once(char::is_whitespace) else {
            continue;
        };
        let Ok(count) = number.parse::<u32>() else {
            continue;
        };
        let label = label.trim().to_ascii_lowercase();
        if label.starts_with("skill") {
            skills = count;
        } else if label.starts_with("mcp") {
            mcp = count;
        }
    }
    (skills, mcp)
}

fn apply_workflow_card(entry: &mut AgenticWorkflowEntry, body: &str) {
    if let Some(name) = element_text(body, "h2") {
        entry.name = name;
    }
    if entry.description.is_none() {
        entry.description = paragraph_text(body, "line-clamp-3");
    }
    let (category, level) = workflow_badges(body);
    entry.category = category;
    entry.level = level;
    let (skills, mcp) = workflow_counts(body);
    entry.skill_count = skills;
    entry.mcp_count = mcp;
}

/// 列表页解析：JSON-LD 拿全量 21 条，卡片补富信息（卡片没有的条目保持默认计数）。
pub(crate) fn parse_workflow_catalog(html: &str) -> Vec<AgenticWorkflowEntry> {
    let mut entries: Vec<AgenticWorkflowEntry> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (slug, name) in json_ld_entries(html) {
        if index.contains_key(&slug) {
            continue;
        }
        index.insert(slug.clone(), entries.len());
        entries.push(AgenticWorkflowEntry {
            slug,
            name,
            ..AgenticWorkflowEntry::default()
        });
    }
    each_card(html, "workflows", |slug, body| {
        let position = match index.get(slug) {
            Some(position) => *position,
            None => {
                let position = entries.len();
                entries.push(AgenticWorkflowEntry {
                    slug: slug.to_owned(),
                    name: slug.to_owned(),
                    ..AgenticWorkflowEntry::default()
                });
                index.insert(slug.to_owned(), position);
                position
            }
        };
        apply_workflow_card(&mut entries[position], body);
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
        let Some(text) = object_literal_at(payload, index) else {
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
    let snippets = mcp_snippets(value);
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

// ── 工作流详情页（`/workflows/<slug>`）───────────────────────────────────────
//
// 详情页的 JSON-LD 是 `[HowTo, BreadcrumbList, WebPage]`：组件 / 步骤 / 耗时在 HowTo 里，
// 展示名用 WebPage 的（HowTo 的 name 是 SEO 标题「How to Set Up the … Workflow」）。

/// 逐个解析页面里的 `application/ld+json` 脚本块（数组会摊平）。
pub(crate) fn json_ld_objects(html: &str) -> Vec<Value> {
    let mut values: Vec<Value> = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = html[cursor..].find("application/ld+json") {
        let marker = cursor + relative;
        let Some(content_start) = html[marker..].find('>').map(|offset| marker + offset + 1) else {
            break;
        };
        let Some(content_end) = html[content_start..]
            .find("</script>")
            .map(|offset| content_start + offset)
        else {
            break;
        };
        cursor = content_end;
        let Ok(value) = serde_json::from_str::<Value>(&html[content_start..content_end]) else {
            continue;
        };
        match value {
            Value::Array(items) => values.extend(items),
            single => values.push(single),
        }
    }
    values
}

/// 详情页 `WebPage` 节点里的真实工作流名（与列表页条目一致，比 HowTo 的 SEO 标题适合展示）。
fn workflow_page_name(objects: &[Value], slug: &str) -> Option<String> {
    let suffix = format!("/workflows/{slug}");
    objects.iter().find_map(|value| {
        if value.get("@type").and_then(Value::as_str) != Some("WebPage") {
            return None;
        }
        let url = json_string(value, "url")?;
        url.trim_end_matches('/')
            .ends_with(&suffix)
            .then(|| json_string(value, "name"))
            .flatten()
    })
}

/// `tool[]` → 组件：`kind` 只看 url 路径，其它路径（如 `/tools/…`）忽略。
fn workflow_component_from_tool(item: &Value) -> Option<AgenticWorkflowComponent> {
    let url = json_string(item, "url")?;
    let path = url.split(['?', '#']).next().unwrap_or(&url);
    let kind = if path.contains("/skills/") {
        "skill"
    } else if path.contains("/mcp/") {
        "mcp"
    } else {
        return None;
    };
    let slug = validate_slug(&slug_from_url(path)?).ok()?.to_owned();
    Some(AgenticWorkflowComponent {
        kind: kind.to_owned(),
        name: json_string(item, "name").unwrap_or_else(|| slug.clone()),
        slug,
        url,
    })
}

fn workflow_components(howto: &Value) -> Vec<AgenticWorkflowComponent> {
    let mut components: Vec<AgenticWorkflowComponent> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let tools = howto.get("tool").and_then(Value::as_array);
    for item in tools.into_iter().flatten() {
        let Some(component) = workflow_component_from_tool(item) else {
            continue;
        };
        if seen.insert((component.kind.clone(), component.slug.clone())) {
            components.push(component);
        }
    }
    components
}

fn workflow_steps(howto: &Value) -> Vec<AgenticWorkflowStep> {
    let steps = howto.get("step").and_then(Value::as_array);
    steps
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let name = json_string(item, "name").unwrap_or_default();
            let text = json_string(item, "text").unwrap_or_default();
            (!name.is_empty() || !text.is_empty()).then_some(AgenticWorkflowStep { name, text })
        })
        .collect()
}

/// 步骤文本里的起手提示词：`… Prompt: "…"`（全角 / 半角冒号都认，引号必须是双引号）。
/// 没有 Prompt、或引号不闭合 → `None`。
fn kickoff_prompt_from_text(text: &str) -> Option<String> {
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find("Prompt") {
        let at = cursor + relative + "Prompt".len();
        cursor = at;
        let rest = text[at..].trim_start();
        let Some(rest) = rest.strip_prefix(':').or_else(|| rest.strip_prefix('：')) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else {
            return None;
        };
        let prompt = rest[..end].trim();
        if !prompt.is_empty() {
            return Some(prompt.to_owned());
        }
    }
    None
}

/// 详情页解析：HowTo 缺失（站点改版 / 404 页）→ `None`。
pub(crate) fn parse_workflow_detail(html: &str, slug: &str) -> Option<AgenticWorkflowDetail> {
    let objects = json_ld_objects(html);
    let howto = objects
        .iter()
        .find(|value| value.get("@type").and_then(Value::as_str) == Some("HowTo"))?;
    let steps = workflow_steps(howto);
    let kickoff_prompt = steps
        .iter()
        .find_map(|step| kickoff_prompt_from_text(&step.text));
    let (category, level) = workflow_badges(html);
    Some(AgenticWorkflowDetail {
        slug: slug.to_owned(),
        name: workflow_page_name(&objects, slug)
            .or_else(|| json_string(howto, "name"))
            .unwrap_or_else(|| slug.to_owned()),
        description: json_string(howto, "description"),
        category,
        level,
        setup_time: json_string(howto, "totalTime"),
        components: workflow_components(howto),
        steps,
        kickoff_prompt,
        source_url: format!("{AGENTIC_WORKFLOWS_URL}/{slug}"),
    })
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

/// 抓一个已知的 chunk URL 并解析，命中判据同 `chunk_is_usable`。
fn chunk_from_url(url: &str) -> Option<CatalogChunk> {
    let js = request_html(url).ok()?;
    let chunk = parse_catalog_chunk(&js);
    chunk_is_usable(&chunk).then_some(chunk)
}

fn chunk_is_usable(chunk: &CatalogChunk) -> bool {
    chunk.skills.len() >= MIN_CHUNK_ENTRIES || chunk.mcp.len() >= MIN_CHUNK_ENTRIES
}

/// 发现并抓取数据 chunk：先试缓存里记下的 URL，否则按文档顺序倒着试列表页里的候选
/// （数据 chunk 靠近末尾），命中后连同 URL 一起返回。
fn fetch_catalog_chunk(page_url: &str, known_url: Option<&str>) -> Option<(String, CatalogChunk)> {
    if let Some(url) = known_url {
        // chunk 名带构建哈希：缓存里的 URL 只要还能抓到内容就仍然有效。
        if let Some(chunk) = chunk_from_url(url) {
            return Some((url.to_owned(), chunk));
        }
    }
    let html = request_html(page_url).ok()?;
    let mut candidates = catalog_chunk_candidates(&html);
    candidates.reverse();
    let mut budget = MAX_CHUNK_BYTES;
    for url in candidates.into_iter().take(MAX_CHUNK_FETCHES) {
        let Ok(js) = request_html(&url) else {
            continue;
        };
        if js.len() > budget {
            break;
        }
        budget -= js.len();
        // 数据 chunk 必然含这两类字段之一，否则连转换都省掉。
        if !js.contains("skillMdUrl:") && !js.contains("mcpServers") {
            continue;
        }
        let chunk = parse_catalog_chunk(&js);
        if chunk_is_usable(&chunk) {
            return Some((url, chunk));
        }
    }
    None
}

/// 取数据 chunk：命中后把 URL 记进缓存，同一进程内的后续请求可以直接抓它。
fn load_catalog_chunk(cache: &AgenticCatalogCache, page_url: &str) -> Option<CatalogChunk> {
    let known = cache.chunk_url();
    let (url, chunk) = fetch_catalog_chunk(page_url, known.as_deref())?;
    cache.store_chunk_url(&url);
    Some(chunk)
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

/// 技能目录：缓存 → 数据 chunk（带分类）→ JSON-LD 降级（无分类，只写短缓存）。
fn cached_skill_catalog(cache: &AgenticCatalogCache) -> Result<Vec<AgenticSkillEntry>, String> {
    if let Some(entries) = cache.skills() {
        return Ok(entries);
    }
    if let Some(chunk) = load_catalog_chunk(cache, AGENTIC_SKILLS_URL) {
        if !chunk.skills.is_empty() {
            cache.store_skills(chunk.skills.clone(), false);
            return Ok(chunk.skills);
        }
    }
    let entries = fetch_skill_catalog()?;
    cache.store_skills(entries.clone(), true);
    Ok(entries)
}

/// MCP 目录：与技能同源（同一个数据 chunk 同时含两者），策略一致。
fn cached_mcp_catalog(cache: &AgenticCatalogCache) -> Result<Vec<AgenticMcpEntry>, String> {
    if let Some(entries) = cache.mcp() {
        return Ok(entries);
    }
    if let Some(chunk) = load_catalog_chunk(cache, AGENTIC_MCP_URL) {
        if !chunk.mcp.is_empty() {
            cache.store_mcp(chunk.mcp.clone(), false);
            return Ok(chunk.mcp);
        }
    }
    let entries = fetch_mcp_catalog()?;
    cache.store_mcp(entries.clone(), true);
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
                entry.long_description.as_deref(),
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

fn skill_description_text(description: Option<&str>, fallback: &str) -> String {
    match description.and_then(non_empty) {
        Some(description) if description.len() > MAX_SKILL_DESCRIPTION_LENGTH => {
            truncate_bytes(&description, MAX_SKILL_DESCRIPTION_LENGTH)
        }
        Some(description) => description,
        None => fallback.to_owned(),
    }
}

/// 安装源：优先用目录条目里已有的 `skillMdUrl`（chunk 数据），没有再去抓详情页。
fn skill_install_source(
    cache: &AgenticCatalogCache,
    slug: &str,
) -> Result<(String, Option<String>), String> {
    let cached = cache.skills().and_then(|entries| {
        entries
            .into_iter()
            .find(|entry| entry.slug == slug)
            .and_then(|entry| entry.skill_md_url.map(|url| (url, entry.description)))
    });
    if let Some(source) = cached {
        return Ok(source);
    }
    let detail = fetch_skill_detail(slug)?;
    let source = detail
        .skill_md_url
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| msg("market.agentic_skill_no_source"))?;
    Ok((source, detail.description))
}

fn install_skill(
    paths: &AppPaths,
    cache: &AgenticCatalogCache,
    request: &AgenticSkillInstall,
) -> Result<String, String> {
    let slug = validate_slug(&request.slug)?.to_owned();
    let name = skill_name_for(&slug)?.to_owned();
    let (source, description) = skill_install_source(cache, &slug)?;
    let content = download_skill_markdown(&source)?;
    let description = skill_description_text(description.as_deref(), &name);
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

/// 安装用配置片段：优先用目录条目里的 `snippets`（chunk 数据），为空再抓详情页。
fn mcp_install_snippets(
    cache: &AgenticCatalogCache,
    slug: &str,
) -> Result<Vec<AgenticMcpSnippet>, String> {
    let cached = cache.mcp().and_then(|entries| {
        entries
            .into_iter()
            .find(|entry| entry.slug == slug)
            .map(|entry| entry.snippets)
            .filter(|snippets| !snippets.is_empty())
    });
    match cached {
        Some(snippets) => Ok(snippets),
        None => Ok(fetch_mcp_detail(slug)?.snippets),
    }
}

fn install_mcp(
    paths: &AppPaths,
    cache: &AgenticCatalogCache,
    request: &AgenticMcpInstall,
) -> Result<String, String> {
    let slug = validate_slug(&request.slug)?.to_owned();
    // 片段解析失败仍然给明确错误码（`market.agentic_mcp_no_config` /
    // `market.agentic_mcp_config_unparsed`），写入前必须拿到可用的最新配置。
    let config = config_from_snippets(&mcp_install_snippets(cache, &slug)?)?;
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

// ── 工作流抓取 / 安装 / 本地状态 ─────────────────────────────────────────────

fn fetch_workflow_detail(slug: &str) -> Result<AgenticWorkflowDetail, String> {
    let slug = validate_slug(slug)?.to_owned();
    let html = request_html(&format!("{AGENTIC_WORKFLOWS_URL}/{slug}"))?;
    parse_workflow_detail(&html, &slug).ok_or_else(|| msg("market.agenticskills_detail_missing"))
}

/// 工作流列表：缓存（TTL 600s）→ 列表页；站点改版导致解析为空时按详情缺失报错。
fn cached_workflow_catalog() -> Result<Vec<AgenticWorkflowEntry>, String> {
    if let Ok(guard) = WORKFLOW_CACHE.lock() {
        if let Some(entries) = fresh_entries(guard.as_ref()) {
            return Ok(entries);
        }
    }
    let html = request_html(AGENTIC_WORKFLOWS_URL)?;
    let entries = parse_workflow_catalog(&html);
    if entries.is_empty() {
        return Err(msg("market.agenticskills_detail_missing"));
    }
    if let Ok(mut guard) = WORKFLOW_CACHE.lock() {
        *guard = Some((Instant::now(), entries.clone(), false));
    }
    Ok(entries)
}

fn filter_workflows(entries: Vec<AgenticWorkflowEntry>, query: &str) -> Vec<AgenticWorkflowEntry> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return entries.into_iter().take(MAX_WORKFLOW_RESULTS).collect();
    }
    entries
        .into_iter()
        .filter(|entry| {
            [
                Some(entry.slug.as_str()),
                Some(entry.name.as_str()),
                entry.description.as_deref(),
                entry.category.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|value| value.to_lowercase().contains(&needle))
        })
        .take(MAX_WORKFLOW_RESULTS)
        .collect()
}

/// 逐个组件安装：`install` 返回 `Ok(true)` 新装、`Ok(false)` 已存在（跳过）、`Err` 失败。
/// 单个组件失败只记进 `failures`，不影响其它组件。
fn install_workflow_components<F>(
    components: &[AgenticWorkflowComponent],
    mut install: F,
) -> (
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Vec<AgenticWorkflowFailure>,
)
where
    F: FnMut(&AgenticWorkflowComponent) -> Result<bool, String>,
{
    let mut skills: Vec<String> = Vec::new();
    let mut mcp: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut failures: Vec<AgenticWorkflowFailure> = Vec::new();
    for component in components {
        match install(component) {
            Ok(true) if component.kind == "mcp" => mcp.push(component.slug.clone()),
            Ok(true) => skills.push(component.slug.clone()),
            Ok(false) => skipped.push(component.slug.clone()),
            Err(error) => failures.push(AgenticWorkflowFailure {
                kind: component.kind.clone(),
                slug: component.slug.clone(),
                error,
            }),
        }
    }
    (skills, mcp, skipped, failures)
}

/// 组件是否已在目标位置：技能看目录、MCP 看 `mcp.json` 里的同名服务。
fn workflow_component_installed(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
    component: &AgenticWorkflowComponent,
) -> Result<bool, String> {
    match component.kind.as_str() {
        "skill" => {
            let name = skill_name_for(&component.slug)?.to_owned();
            agent_config::skill_dir_exists(paths, scope, project_path, &name)
        }
        "mcp" => {
            let name = mcp_server_name(&component.slug, None)?;
            agent_config::mcp_server_exists(paths, scope, project_path, &name)
        }
        _ => Err(msg("market.agenticskills_detail_missing")),
    }
}

/// 安装一个组件：已存在 → `Ok(false)`（不覆盖）；新装 → `Ok(true)`。
/// 数据来源优先目录条目（`skill_md_url` / `snippets`），没有才回退到详情页（见既有安装函数）。
fn install_workflow_component(
    paths: &AppPaths,
    cache: &AgenticCatalogCache,
    request: &AgenticWorkflowInstall,
    component: &AgenticWorkflowComponent,
) -> Result<bool, String> {
    if workflow_component_installed(
        paths,
        request.scope,
        request.project_path.as_deref(),
        component,
    )? {
        return Ok(false);
    }
    let project_path = request.project_path.clone();
    match component.kind.as_str() {
        "skill" => {
            install_skill(
                paths,
                cache,
                &AgenticSkillInstall {
                    slug: component.slug.clone(),
                    scope: request.scope,
                    project_path,
                },
            )?;
            Ok(true)
        }
        "mcp" => {
            // 服务名固定用 slug：与详情里的组件、状态文件里的条目一一对应，便于界面匹配。
            install_mcp(
                paths,
                cache,
                &AgenticMcpInstall {
                    slug: component.slug.clone(),
                    name: None,
                    scope: request.scope,
                    project_path,
                },
            )?;
            Ok(true)
        }
        _ => Err(msg("market.agenticskills_detail_missing")),
    }
}

/// 安装时间：与任务记录一致，用 Unix epoch 毫秒的十进制字符串（可排序、可 `new Date`）。
fn installed_at_now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_default()
}

/// 工作流状态文件：全局 `<pi_home>/workflows.json`、项目 `<project>/.pi/workflows.json`。
fn workflows_file(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
) -> Result<PathBuf, String> {
    Ok(agent_config::scope_pi_dir(paths, scope, project_path)?.join("workflows.json"))
}

/// 文件大小超限（手改 / 异常写入）时不读内容，避免把内存撑坏。
fn read_small_text_file(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_WORKFLOW_FILE_BYTES as u64 {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// 读取状态文件：文件缺失 / 读失败 / JSON 非法 / 结构不对一律当空表（界面不因此报错）。
fn read_installed_workflow_file(path: &Path) -> BTreeMap<String, InstalledWorkflow> {
    let Some(content) = read_small_text_file(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&content) else {
        return BTreeMap::new();
    };
    let Some(records) = value.get("workflows").and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    records
        .iter()
        .filter_map(|(slug, raw)| {
            if validate_slug(slug).is_err() {
                return None;
            }
            let record: InstalledWorkflowRecord = serde_json::from_value(raw.clone()).ok()?;
            let name = non_empty(&record.name).unwrap_or_else(|| slug.clone());
            Some((
                slug.clone(),
                InstalledWorkflow {
                    slug: slug.clone(),
                    name,
                    category: record.category,
                    skill_count: record.skill_count,
                    mcp_count: record.mcp_count,
                    installed_at: record.installed_at,
                    components: record.components,
                },
            ))
        })
        .collect()
}

/// 已安装工作流（按 slug 排序，最多 `MAX_INSTALLED_WORKFLOWS` 条）。
/// 作用域路径非法（项目范围缺 `projectPath`）才报错，其余情况返回空数组。
fn read_installed_workflows(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
) -> Result<Vec<InstalledWorkflow>, String> {
    let path = workflows_file(paths, scope, project_path)?;
    Ok(read_installed_workflow_file(&path)
        .into_values()
        .take(MAX_INSTALLED_WORKFLOWS)
        .collect())
}

/// 删掉 `installedAt` 最旧的一条（时间不可解析时按最旧处理，同级按 slug）。
fn remove_oldest_installed_workflow(workflows: &mut BTreeMap<String, InstalledWorkflow>) {
    let mut entries: Vec<(u64, String)> = workflows
        .iter()
        .map(|(slug, workflow)| {
            (
                workflow.installed_at.parse::<u64>().unwrap_or(0),
                slug.clone(),
            )
        })
        .collect();
    entries.sort();
    if let Some((_, slug)) = entries.first() {
        workflows.remove(slug);
    }
}

/// 序列化前的上限保护：条目数 64、文件大小 256 KiB，超出按 `installedAt` 从旧到新丢弃。
fn installed_workflows_json(
    workflows: &BTreeMap<String, InstalledWorkflow>,
) -> Result<Vec<u8>, String> {
    let mut kept = workflows.clone();
    loop {
        while kept.len() > MAX_INSTALLED_WORKFLOWS {
            remove_oldest_installed_workflow(&mut kept);
        }
        let bytes = serde_json::to_vec_pretty(&serde_json::json!({ "workflows": &kept }))
            .map_err(|error| format!("failed to serialize workflows: {error}"))?;
        if bytes.len() <= MAX_WORKFLOW_FILE_BYTES {
            return Ok(bytes);
        }
        if kept.is_empty() {
            return Err(msg("agent_config.file_too_large"));
        }
        remove_oldest_installed_workflow(&mut kept);
    }
}

/// 记录一个已安装的工作流（覆盖同 slug 的旧记录），原子写入。
fn store_installed_workflow(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
    workflow: InstalledWorkflow,
) -> Result<(), String> {
    let path = workflows_file(paths, scope, project_path)?;
    let mut workflows = read_installed_workflow_file(&path);
    workflows.insert(workflow.slug.clone(), workflow);
    let content = installed_workflows_json(&workflows)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create workflows directory: {error}"))?;
    }
    let mut file = AtomicWriteFile::open(&path)
        .map_err(|error| format!("failed to open workflows file: {error}"))?;
    file.write_all(&content)
        .map_err(|error| format!("failed to write workflows file: {error}"))?;
    file.commit()
        .map_err(|error| format!("failed to commit workflows file: {error}"))
}

/// 详情 → 状态记录：计数按详情里的组件构成（与站点卡片一致）。
fn installed_workflow_record(
    detail: &AgenticWorkflowDetail,
    installed_at: String,
) -> InstalledWorkflow {
    let count = |kind: &str| {
        detail
            .components
            .iter()
            .filter(|component| component.kind == kind)
            .count() as u32
    };
    InstalledWorkflow {
        slug: detail.slug.clone(),
        name: detail.name.clone(),
        category: detail.category.clone(),
        skill_count: count("skill"),
        mcp_count: count("mcp"),
        installed_at,
        components: detail.components.clone(),
    }
}

/// 一个按钮装齐：先取详情（组件清单），逐件安装（已存在跳过、失败不中断），
/// 最后写入本地状态文件（供「已安装」标记与输入框下方的选择器使用）。
fn install_workflow(
    paths: &AppPaths,
    request: &AgenticWorkflowInstall,
) -> Result<AgenticWorkflowInstallResult, String> {
    let slug = validate_slug(&request.slug)?.to_owned();
    let detail = fetch_workflow_detail(&slug)?;
    if detail.components.is_empty() {
        return Err(msg("market.agenticskills_detail_missing"));
    }
    // 目录只抓一次：技能 / MCP 组件都从同一份 chunk 里找条目，找不到再回退各自详情页。
    let cache = &WORKFLOW_CATALOG_CACHE;
    let _ = cached_skill_catalog(cache);
    let _ = cached_mcp_catalog(cache);
    let (skills, mcp, skipped, failures) =
        install_workflow_components(&detail.components, |component| {
            install_workflow_component(paths, cache, request, component)
        });
    store_installed_workflow(
        paths,
        request.scope,
        request.project_path.as_deref(),
        installed_workflow_record(&detail, installed_at_now()),
    )?;
    Ok(AgenticWorkflowInstallResult {
        workflow: slug,
        skills,
        mcp,
        skipped,
        failures,
    })
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
    app: tauri::AppHandle,
    paths: State<'_, AppPaths>,
    request: AgenticSkillInstall,
) -> Result<String, String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let cache = app.state::<AgenticCatalogCache>();
        install_skill(&paths, &cache, &request)
    })
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
    app: tauri::AppHandle,
    paths: State<'_, AppPaths>,
    request: AgenticMcpInstall,
) -> Result<String, String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let cache = app.state::<AgenticCatalogCache>();
        install_mcp(&paths, &cache, &request)
    })
    .await
    .map_err(|error| format!("agentic MCP install worker failed: {error}"))?
}

#[tauri::command]
pub async fn search_agentic_workflows(
    request: AgenticQuery,
) -> Result<Vec<AgenticWorkflowEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let entries = cached_workflow_catalog()?;
        Ok(filter_workflows(entries, &request.query))
    })
    .await
    .map_err(|error| format!("agentic workflow search worker failed: {error}"))?
}

#[tauri::command]
pub async fn agentic_workflow_detail(
    request: AgenticSlug,
) -> Result<AgenticWorkflowDetail, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_workflow_detail(&request.slug))
        .await
        .map_err(|error| format!("agentic workflow detail worker failed: {error}"))?
}

#[tauri::command]
pub async fn install_agentic_workflow(
    paths: State<'_, AppPaths>,
    request: AgenticWorkflowInstall,
) -> Result<AgenticWorkflowInstallResult, String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || install_workflow(&paths, &request))
        .await
        .map_err(|error| format!("agentic workflow install worker failed: {error}"))?
}

#[tauri::command]
pub async fn list_installed_workflows(
    paths: State<'_, AppPaths>,
    scope: ConfigScope,
    project_path: Option<String>,
) -> Result<Vec<InstalledWorkflow>, String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        read_installed_workflows(&paths, scope, project_path.as_deref())
    })
    .await
    .map_err(|error| format!("agentic workflow list worker failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::{
        agent_config, catalog_chunk_candidates, config_from_snippets, filter_mcp, filter_skills,
        filter_workflows, format_stars, install_workflow_components, installed_workflows_json,
        js_literals_to_json, kickoff_prompt_from_text, mcp_install_snippets, mcp_server_name,
        parse_catalog_chunk, parse_count_text, parse_mcp_catalog, parse_mcp_detail,
        parse_mcp_snippet, parse_skill_catalog, parse_skill_detail, parse_workflow_catalog,
        parse_workflow_detail, read_installed_workflows, skill_description_text,
        skill_install_source, skill_name_for, store_installed_workflow, truncate_bytes,
        validate_slug, workflow_component_installed, AgenticMcpInstall, AgenticMcpSnippet,
        AgenticSkillInstall, AgenticWorkflowComponent, ConfigScope, InstalledWorkflow,
        MAX_INSTALLED_WORKFLOWS,
    };
    use crate::app_paths::AppPaths;
    use crate::message::message_code;
    use std::collections::BTreeMap;
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
        assert_eq!(
            skill_description_text(detail.description.as_deref(), "fallback").len(),
            512
        );
        let empty = super::AgenticSkillDetail::default();
        assert_eq!(
            skill_description_text(empty.description.as_deref(), "fallback"),
            "fallback"
        );
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

        // chunk 路径带回分类，本地过滤也要能按分类命中。
        let chunk = parse_catalog_chunk(CATALOG_CHUNK_JS);
        assert_eq!(
            filter_skills(chunk.skills.clone(), "design & ui/ux").len(),
            1
        );
        assert_eq!(filter_mcp(chunk.mcp, "productivity & pm").len(), 1);
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

    /// 数据 chunk 的真实形态：裸键、`!0`/`!1`、三种引号、嵌套对象/数组、指数数字，
    /// 外加一段「仓库统计」映射，以及两个必须被忽略的对象（字符串里的假条目、导航项）。
    const CATALOG_CHUNK_JS: &str = r#"
const decoy = 'this string holds a fake entry: {slug:"decoy",platforms:["x"]}';
function render(){let e={slug:"nav-item",name:"Nav",icon:"Layers"};return e}
let s={"taste-skill":{lastCommitAt:"2026-05-26T19:31:36Z",repoStars:87002,repoForks:5930},"docs-writer":{repoStars:12}};
var i=[
 {slug:"taste-skill",name:"Taste Skill",description:"Anti-slop, frontend skill with a } brace and a , comma inside the string.",longDescription:"Ships as design-taste-frontend upstream.",author:{name:"Leonxlnx",username:"Leonxlnx",url:"https://github.com/Leonxlnx"},category:"Design & UI/UX",tags:["design","frontend"],platforms:["claude-code","codex","cursor","multi-platform"],githubUrl:"https://github.com/Leonxlnx/taste-skill/tree/main/skills/taste-skill",skillMdUrl:"https://raw.githubusercontent.com/Leonxlnx/taste-skill/main/skills/taste-skill/SKILL.md",quality:"S",isFeatured:!0,license:"MIT",lastUpdated:"2026-05-26",installCommand:"npx skills add Leonxlnx/taste-skill",price:"open-source"},
 {slug:"docs-writer",name:"Docs Writer",description:"Writes the docs you keep postponing.",author:{name:"Upstash",url:"https://upstash.com"},category:"Developer Tools",tags:["documentation"],platforms:["codex"],githubUrl:"https://github.com/upstash/docs-writer",skillMdUrl:"https://raw.githubusercontent.com/upstash/docs-writer/main/SKILL.md",quality:"A",isFeatured:!1,license:"Apache-2.0",lastUpdated:"2026-02-10",githubStars:46e3}
];
var t=[{slug:"notion",name:"Notion",description:"Pages, databases, search, and comments via Notion's hosted MCP.",longDescription:"The official Notion MCP server.",author:{name:"Notion",url:"https://notion.so"},category:"Productivity & PM",isOfficial:!0,trustLevel:"official",websiteUrl:"https://developers.notion.com",transport:["Streamable HTTP"],requiresApiKey:!0,isFeatured:!0,tags:["notes","databases"],popularity:"~36K visitors/wk",language:"TypeScript",configSource:"https://developers.notion.com/docs/get-started-with-mcp",config:[{label:"uvx",file:"claude_desktop_config.json",code:'"mcpServers": {\n  "notion": { "command": "uvx" }\n}'},{label:"Claude Code · remote",code:"claude mcp add --transport http notion https://mcp.notion.com/mcp"}]}];
"#;

    /// 单个 JS 对象字面量：裸键、`!0`/`!1`、`undefined`、指数数字、字符串里的 `,`/`}`、
    /// 转义引号、嵌套 `author`、单引号包裹的 `config[].code`。
    const JS_LITERAL_SAMPLE: &str = r#"{slug:"taste-skill",name:"Taste Skill",description:"Anti-slop, frontend skill with } and \"quoted\" text",author:{name:"Leonxlnx",url:"https://github.com/Leonxlnx"},isFeatured:!0,isOfficial:!1,missing:undefined,count:46e3,tags:["design","frontend"],config:[{label:"uvx",code:'{"mcpServers": {"notion": {"command": "uvx"}}}'}]}"#;

    /// 列表页里 chunk 的三种写法：相对路径、flight 数据里的 `\/` 转义、绝对 URL，
    /// 外加一个重复项、一个 CSS（要排除）、一个别的域名与一个非 immutable chunk。
    const LISTING_HTML: &str = r#"
<script src="/_next/static/immutable/chunks/aaa.js"></script>
<link rel="stylesheet" href="/_next/static/css/app.css">
<script>self.__next_f.push([1,"3:[\"$L1\",[\"https:\/\/agenticskills.io\/_next\/static\/immutable\/chunks\/bbb.js\"]]"])</script>
<script src="https://agenticskills.io/_next/static/immutable/chunks/aaa.js"></script>
<script src="/_next/static/immutable/chunks/ccc.js"></script>
<script src="https://cdn.example.com/_next/static/immutable/chunks/ddd.js"></script>
<script src="/_next/static/chunks/main-app.js"></script>
"#;

    #[test]
    fn converts_js_object_literals_to_json() {
        let json = js_literals_to_json(JS_LITERAL_SAMPLE);
        assert!(
            json.contains("\"slug\":"),
            "bare keys should be quoted: {json}"
        );
        assert!(!json.contains("slug:"));

        let value: serde_json::Value = serde_json::from_str(&json).expect("literal should parse");
        assert_eq!(value["slug"], "taste-skill");
        assert_eq!(value["name"], "Taste Skill");
        assert_eq!(value["author"]["name"], "Leonxlnx");
        assert_eq!(value["author"]["url"], "https://github.com/Leonxlnx");
        assert_eq!(
            value["description"],
            "Anti-slop, frontend skill with } and \"quoted\" text"
        );
        assert_eq!(value["tags"][1], "frontend");
        assert_eq!(value["isFeatured"].as_bool(), Some(true));
        assert_eq!(value["isOfficial"].as_bool(), Some(false));
        assert!(value["missing"].is_null());
        assert_eq!(value["count"].as_u64(), Some(46_000));

        let code = value["config"][0]["code"].as_str().expect("snippet code");
        assert!(code.contains("\"mcpServers\""));
        let snippet = parse_mcp_snippet(code).expect("snippet should parse");
        assert_eq!(snippet["command"], "uvx");

        // 残缺字面量不 panic，只是解析不出来。
        let broken = js_literals_to_json("{slug:");
        assert!(serde_json::from_str::<serde_json::Value>(&broken).is_err());
    }

    #[test]
    fn parses_catalog_chunk_with_categories() {
        let chunk = parse_catalog_chunk(CATALOG_CHUNK_JS);

        assert_eq!(chunk.skills.len(), 2);
        let skill = &chunk.skills[0];
        assert_eq!(skill.slug, "taste-skill");
        assert_eq!(skill.name, "Taste Skill");
        assert_eq!(skill.category.as_deref(), Some("Design & UI/UX"));
        assert_eq!(skill.author.as_deref(), Some("Leonxlnx"));
        assert_eq!(
            skill.skill_md_url.as_deref(),
            Some("https://raw.githubusercontent.com/Leonxlnx/taste-skill/main/skills/taste-skill/SKILL.md")
        );
        assert_eq!(
            skill.platforms,
            vec!["claude-code", "codex", "cursor", "multi-platform"]
        );
        assert_eq!(skill.tags, vec!["design", "frontend"]);
        assert_eq!(skill.license.as_deref(), Some("MIT"));
        assert_eq!(skill.last_updated.as_deref(), Some("2026-05-26"));
        assert_eq!(skill.quality.as_deref(), Some("S"));
        assert_eq!(
            skill.long_description.as_deref(),
            Some("Ships as design-taste-frontend upstream.")
        );
        assert_eq!(
            skill.github_url.as_deref(),
            Some("https://github.com/Leonxlnx/taste-skill/tree/main/skills/taste-skill")
        );
        assert!(skill
            .description
            .as_deref()
            .is_some_and(|text| text.contains("} brace and a , comma")));
        assert!(skill.featured);
        // 卡片上的安装量来自仓库统计（`repoStars`）。
        assert_eq!(skill.installs.as_deref(), Some("87.0K"));

        // `githubStars` 优先于仓库统计，指数写法 `46e3` 也要能读。
        assert_eq!(chunk.skills[1].installs.as_deref(), Some("46.0K"));
        assert!(!chunk.skills[1].featured);
        assert!(!chunk.skills.iter().any(|entry| entry.slug == "decoy"));
        assert!(!chunk.skills.iter().any(|entry| entry.slug == "nav-item"));

        assert_eq!(chunk.repo_stars.len(), 2);
        assert_eq!(chunk.repo_stars["taste-skill"], 87_002);
        assert_eq!(chunk.repo_stars["docs-writer"], 12);

        assert_eq!(chunk.mcp.len(), 1);
        let server = &chunk.mcp[0];
        assert_eq!(server.slug, "notion");
        assert_eq!(server.category.as_deref(), Some("Productivity & PM"));
        assert_eq!(server.transport, vec!["Streamable HTTP"]);
        assert!(server.official);
        assert!(server.requires_api_key);
        assert!(server.featured);
        assert_eq!(server.popularity.as_deref(), Some("~36K visitors/wk"));
        assert_eq!(
            server.website_url.as_deref(),
            Some("https://developers.notion.com")
        );
        assert_eq!(
            server.config_source.as_deref(),
            Some("https://developers.notion.com/docs/get-started-with-mcp")
        );
        assert_eq!(server.tags, vec!["notes", "databases"]);
        assert_eq!(server.snippets.len(), 2);
        assert_eq!(server.snippets[0].label, "uvx");
        assert_eq!(
            server.snippets[0].file.as_deref(),
            Some("claude_desktop_config.json")
        );
        assert!(server.snippets[0].code.contains("\"mcpServers\""));
        assert_eq!(
            parse_mcp_snippet(&server.snippets[0].code).expect("snippet")["command"],
            "uvx"
        );
        assert!(server.snippets[1].code.starts_with("claude mcp add"));

        // 普通 JS（函数体、没有目录字段的对象）不会产出条目，也不会 panic。
        let empty = parse_catalog_chunk("function f(){return {a:1}}");
        assert!(empty.skills.is_empty() && empty.mcp.is_empty());
        assert!(parse_catalog_chunk("").skills.is_empty());
    }

    #[test]
    fn install_paths_reuse_catalog_entries_without_extra_fetches() {
        // 目录缓存里有条目时，安装直接吃 chunk 数据（source / snippets），不再抓详情页。
        let cache = super::AgenticCatalogCache::default();
        let chunk = parse_catalog_chunk(CATALOG_CHUNK_JS);
        cache.store_skills(chunk.skills.clone(), false);
        cache.store_mcp(chunk.mcp.clone(), false);

        let (source, description) =
            skill_install_source(&cache, "taste-skill").expect("cached skill source");
        assert_eq!(
            source,
            "https://raw.githubusercontent.com/Leonxlnx/taste-skill/main/skills/taste-skill/SKILL.md"
        );
        assert!(description.is_some());

        let snippets = mcp_install_snippets(&cache, "notion").expect("cached snippets");
        assert_eq!(snippets.len(), 2);
        let config = config_from_snippets(&snippets).expect("snippet should parse");
        assert_eq!(config["command"], "uvx");
    }

    #[test]
    fn formats_star_counts_like_the_site() {
        assert_eq!(format_stars(87_002), "87.0K");
        assert_eq!(format_stars(1_234_567), "1.2M");
        assert_eq!(format_stars(1_000_000), "1.0M");
        assert_eq!(format_stars(1_000), "1.0K");
        assert_eq!(format_stars(923), "923");
        assert_eq!(format_stars(0), "");
    }

    /// stars / heat 的小夹具：技能覆盖仓库统计映射、条目自身 `githubStars`（指数写法）与
    /// 条目自身 `repoStars`；MCP 覆盖「有星」「只有 visitors/wk」「没有数字」几种形态。
    const STARS_HEAT_CHUNK_JS: &str = r#"
let s={"taste-skill":{repoStars:87002}};
var i=[
 {slug:"taste-skill",name:"Taste Skill",platforms:["codex"]},
 {slug:"docs-writer",name:"Docs Writer",platforms:["codex"],githubStars:46e3},
 {slug:"entry-stars",name:"Entry Stars",platforms:["codex"],repoStars:12000},
 {slug:"plain-skill",name:"Plain Skill",platforms:["codex"]}
];
var t=[
 {slug:"notion",name:"Notion",transport:["Streamable HTTP"],popularity:"~36K visitors/wk"},
 {slug:"glama",name:"Glama",transport:["stdio"],popularity:"A/A/A on Glama"},
 {slug:"official",name:"Official",transport:["stdio"],popularity:"Listed official"},
 {slug:"yearly",name:"Yearly",transport:["stdio"],popularity:"1.2M visitors/mo"},
 {slug:"starred",name:"Starred",transport:["stdio"],githubStars:46e3,popularity:"~36K visitors/wk"}
];
"#;

    #[test]
    fn exposes_raw_stars_and_heat_for_sorting() {
        let chunk = parse_catalog_chunk(STARS_HEAT_CHUNK_JS);
        let skill = |slug: &str| {
            chunk
                .skills
                .iter()
                .find(|entry| entry.slug == slug)
                .expect("skill entry")
        };

        // 仓库统计映射里的 `repoStars` 是技能的原始值，`installs` 只用于显示。
        let taste = skill("taste-skill");
        assert_eq!(taste.stars, Some(87_002));
        assert_eq!(taste.heat, Some(87_002));
        assert_eq!(taste.installs.as_deref(), Some("87.0K"));

        // 序列化契约：字段名保持 camelCase，缺省值就是 `null`（前端按可选字段处理）。
        let serialized = serde_json::to_value(taste).expect("entry should serialize");
        assert_eq!(serialized["stars"], 87_002);
        assert_eq!(serialized["heat"], 87_002);

        // 条目自身的 `githubStars:46e3` 优先于映射（指数写法在 JS→JSON 时折成 46000）。
        let docs = skill("docs-writer");
        assert_eq!(docs.stars, Some(46_000));
        assert_eq!(docs.heat, Some(46_000));
        assert_eq!(docs.installs.as_deref(), Some("46.0K"));

        // 条目自身的 `repoStars` 也算数。
        assert_eq!(skill("entry-stars").stars, Some(12_000));

        // 没有任何计数的条目保持 `None`，不用 0 冒充。
        let plain = skill("plain-skill");
        assert_eq!(plain.stars, None);
        assert_eq!(plain.heat, None);
        assert_eq!(plain.installs, None);
        let serialized = serde_json::to_value(plain).expect("entry should serialize");
        assert!(serialized["stars"].is_null() && serialized["heat"].is_null());

        let mcp = |slug: &str| {
            chunk
                .mcp
                .iter()
                .find(|entry| entry.slug == slug)
                .expect("mcp entry")
        };

        // MCP：没有 `githubStars` 时从 `popularity` 文本里取计数。
        let notion = mcp("notion");
        assert_eq!(notion.stars, None);
        assert_eq!(notion.heat, Some(36_000));
        assert_eq!(notion.popularity.as_deref(), Some("~36K visitors/wk"));
        assert_eq!(mcp("yearly").heat, Some(1_200_000));

        // 文本里没有数字 → `None`。
        assert_eq!(mcp("glama").heat, None);
        assert_eq!(mcp("official").heat, None);

        // MCP：有 `githubStars` 时热度用星数，不再看 `popularity`。
        let starred = mcp("starred");
        assert_eq!(starred.stars, Some(46_000));
        assert_eq!(starred.heat, Some(46_000));
    }

    #[test]
    fn parses_raw_count_boundaries() {
        // K / M 换算、逗号千分位、`~` 前缀、小数与尾部文本。
        assert_eq!(parse_count_text("~36K visitors/wk"), Some(36_000));
        assert_eq!(parse_count_text("1.2M"), Some(1_200_000));
        assert_eq!(parse_count_text("1.5K"), Some(1_500));
        assert_eq!(parse_count_text("0.5M"), Some(500_000));
        assert_eq!(parse_count_text("87,002"), Some(87_002));
        assert_eq!(parse_count_text("~ 12,345K"), Some(12_345_000));
        assert_eq!(parse_count_text("5000+ weekly"), Some(5_000));
        assert_eq!(parse_count_text("46"), Some(46));

        // 没有数字 / 负数 / 指数 / 越界 → `None`。
        assert_eq!(parse_count_text(""), None);
        assert_eq!(parse_count_text("   "), None);
        assert_eq!(parse_count_text("Listed official"), None);
        assert_eq!(parse_count_text("A/A/A on Glama"), None);
        assert_eq!(parse_count_text("-5K"), None);
        assert_eq!(parse_count_text("~-5K"), None);
        assert_eq!(parse_count_text("~1e6"), None);
        assert_eq!(parse_count_text("12M5"), None);
        assert_eq!(parse_count_text(&u64::MAX.to_string()), Some(u64::MAX));
        assert_eq!(parse_count_text(&(u64::MAX as u128 + 1).to_string()), None);
        assert_eq!(parse_count_text(&format!("{}K", u64::MAX)), None);
        assert_eq!(parse_count_text(&"9".repeat(60)), None);
    }

    #[test]
    fn extracts_chunk_candidates_from_listing_html() {
        assert_eq!(
            catalog_chunk_candidates(LISTING_HTML),
            vec![
                "https://agenticskills.io/_next/static/immutable/chunks/aaa.js",
                "https://agenticskills.io/_next/static/immutable/chunks/bbb.js",
                "https://agenticskills.io/_next/static/immutable/chunks/ccc.js",
                "https://cdn.example.com/_next/static/immutable/chunks/ddd.js",
                "https://agenticskills.io/_next/static/chunks/main-app.js",
            ]
        );
        assert!(catalog_chunk_candidates("<html><body>no chunks</body></html>").is_empty());
    }

    // ── 工作流（`/workflows`）────────────────────────────────────────────────

    /// 列表页小样本：JSON-LD `ItemList`（2 条）+ 一张卡片。卡片里有分类 / 难度 / 描述与
    /// 计数 span（真实页面会在数字与文字之间插 `<!-- -->` 注释节点）。
    const WORKFLOW_CATALOG_HTML: &str = r#"
      <script type="application/ld+json">[{"@context":"https://schema.org","@type":"CollectionPage",
        "url":"https://agenticskills.io/workflows","mainEntity":{"@type":"ItemList","numberOfItems":2,"itemListElement":[
          {"@type":"ListItem","position":1,"url":"https://agenticskills.io/workflows/fullstack-saas","name":"Full-Stack SaaS"},
          {"@type":"ListItem","position":2,"url":"https://agenticskills.io/workflows/disciplined-development","name":"Disciplined Development"}
        ]}},{"@context":"https://schema.org","@type":"BreadcrumbList"}]</script>
      <a class="group block bg-white border-r border-b border-[var(--line)] p-6" href="/workflows/fullstack-saas"><div class="flex items-center gap-2 mb-3"><span class="inline-block px-2.5 py-0.5 text-[10px] font-mono uppercase tracking-[0.12em] text-white bg-[var(--ink)] border border-[var(--ink)]">Development</span><span class="inline-block px-2 py-0.5 text-[10px] font-mono uppercase tracking-[0.12em] border text-[var(--ink)] bg-[var(--paper-3)] border-[var(--line-2)]">Intermediate</span></div><h2 class="text-lg font-semibold text-[var(--ink)] mb-2 line-clamp-2">Full-Stack SaaS</h2><p class="text-sm text-[var(--mute-1)] mb-4 line-clamp-3">The complete Next.js + Supabase + Vercel stack.</p><div class="flex items-center justify-between"><div class="flex items-center gap-3 text-xs text-[var(--mute-2)]"><span class="flex items-center gap-1"><svg class="lucide lucide-zap w-3.5 h-3.5"><path d="M4 14"></path></svg>5<!-- --> <!-- -->skills</span><span class="flex items-center gap-1"><svg class="lucide lucide-layers w-3.5 h-3.5"><path d="M12.83 2.18"></path></svg>4<!-- --> <!-- -->MCP servers</span></div><span class="flex items-center gap-1 as-micro">View<svg class="lucide lucide-arrow-right w-3.5 h-3.5"></svg></span></div></a>
    "#;

    /// 详情页小样本：`[HowTo, WebPage]`。HowTo 里有 2 个技能组件、1 个 MCP 组件、
    /// 1 个非组件工具（`/tools/`）与 1 条重复组件；step 2 里带起手提示词。
    const WORKFLOW_DETAIL_HTML: &str = r#"
      <script type="application/ld+json">[{"@context":"https://schema.org","@type":"HowTo","name":"How to Set Up the Disciplined Development Workflow","description":"Plan before coding, then verify before completion.","totalTime":"PT2M","tool":[{"@type":"HowToTool","name":"Brainstorming","url":"https://agenticskills.io/skills/brainstorming"},{"@type":"HowToTool","name":"Test-Driven Development","url":"https://agenticskills.io/skills/test-driven-development"},{"@type":"HowToTool","name":"GitHub MCP","url":"https://agenticskills.io/mcp/github"},{"@type":"HowToTool","name":"Submit a Skill","url":"https://agenticskills.io/tools/submit"},{"@type":"HowToTool","name":"Brainstorming again","url":"https://agenticskills.io/skills/brainstorming"}],"step":[{"@type":"HowToStep","position":1,"name":"Install the skills","text":"Install the whole collection. All 8 skills auto-activate."},{"@type":"HowToStep","position":2,"name":"Run the sprint","text":"Run the sprint：Prompt: \"Find the keyword gap for <topic>, draft an optimized article.\""}]},{"@context":"https://schema.org","@type":"WebPage","name":"Disciplined Development","url":"https://agenticskills.io/workflows/disciplined-development","isPartOf":{"@type":"WebSite","url":"https://agenticskills.io"}}]</script>
      <div class="mb-12"><div class="flex items-center gap-2 mb-4"><span class="inline-block px-3 py-1 text-[10px] font-mono uppercase tracking-[0.12em] text-white bg-[var(--ink)] border border-[var(--ink)]">Development</span><span class="inline-block px-3 py-1 text-[10px] font-mono uppercase tracking-[0.12em] border text-[var(--ink)] bg-white border-[var(--line)]">Beginner</span></div><h1 class="text-4xl">Disciplined Development</h1></div>
    "#;

    fn workflow_component(kind: &str, slug: &str) -> AgenticWorkflowComponent {
        AgenticWorkflowComponent {
            kind: kind.to_owned(),
            slug: slug.to_owned(),
            name: slug.to_owned(),
            url: format!(
                "https://agenticskills.io/{}/{slug}",
                if kind == "mcp" { "mcp" } else { "skills" }
            ),
        }
    }

    #[test]
    fn parses_workflow_catalog_with_card_metadata() {
        let entries = parse_workflow_catalog(WORKFLOW_CATALOG_HTML);

        assert_eq!(entries.len(), 2);
        let entry = &entries[0];
        assert_eq!(entry.slug, "fullstack-saas");
        assert_eq!(entry.name, "Full-Stack SaaS");
        assert_eq!(entry.category.as_deref(), Some("Development"));
        assert_eq!(entry.level.as_deref(), Some("Intermediate"));
        assert_eq!(
            entry.description.as_deref(),
            Some("The complete Next.js + Supabase + Vercel stack.")
        );
        assert_eq!(entry.skill_count, 5);
        assert_eq!(entry.mcp_count, 4);

        // JSON-LD 有、卡片没有的条目保持默认值（JSON-LD 的顺序在前）。
        assert_eq!(entries[1].slug, "disciplined-development");
        assert_eq!(entries[1].name, "Disciplined Development");
        assert_eq!(entries[1].category, None);
        assert_eq!(entries[1].level, None);
        assert_eq!(entries[1].skill_count, 0);
        assert_eq!(entries[1].mcp_count, 0);

        assert!(parse_workflow_catalog("<html><body>no data</body></html>").is_empty());
    }

    #[test]
    fn filters_workflows_locally_by_query() {
        let entries = parse_workflow_catalog(WORKFLOW_CATALOG_HTML);

        assert_eq!(filter_workflows(entries.clone(), "").len(), 2);
        assert_eq!(filter_workflows(entries.clone(), "SAAS").len(), 1);
        assert_eq!(filter_workflows(entries.clone(), "vercel").len(), 1);
        assert_eq!(filter_workflows(entries.clone(), "development").len(), 2);
        assert!(filter_workflows(entries, "missing").is_empty());
    }

    #[test]
    fn parses_workflow_detail_with_components_and_kickoff_prompt() {
        let detail = parse_workflow_detail(WORKFLOW_DETAIL_HTML, "disciplined-development")
            .expect("workflow detail should parse");

        assert_eq!(detail.slug, "disciplined-development");
        // 展示名取 WebPage 的（HowTo 的 name 是 SEO 标题「How to Set Up …」）。
        assert_eq!(detail.name, "Disciplined Development");
        assert_eq!(
            detail.description.as_deref(),
            Some("Plan before coding, then verify before completion.")
        );
        assert_eq!(detail.category.as_deref(), Some("Development"));
        assert_eq!(detail.level.as_deref(), Some("Beginner"));
        assert_eq!(detail.setup_time.as_deref(), Some("PT2M"));
        assert_eq!(
            detail.source_url,
            "https://agenticskills.io/workflows/disciplined-development"
        );

        // `/tools/` 路径被忽略、重复组件只留一份。
        assert_eq!(detail.components.len(), 3);
        assert_eq!(detail.components[0].kind, "skill");
        assert_eq!(detail.components[0].slug, "brainstorming");
        assert_eq!(detail.components[0].name, "Brainstorming");
        assert_eq!(detail.components[1].kind, "skill");
        assert_eq!(detail.components[1].slug, "test-driven-development");
        assert_eq!(detail.components[2].kind, "mcp");
        assert_eq!(detail.components[2].slug, "github");
        assert_eq!(
            detail.components[2].url,
            "https://agenticskills.io/mcp/github"
        );

        assert_eq!(detail.steps.len(), 2);
        assert_eq!(detail.steps[0].name, "Install the skills");
        assert!(detail.steps[1].text.starts_with("Run the sprint"));
        assert_eq!(
            detail.kickoff_prompt.as_deref(),
            Some("Find the keyword gap for <topic>, draft an optimized article.")
        );

        assert_eq!(
            parse_workflow_detail("<html><body>no data</body></html>", "demo-workflow"),
            None
        );
    }

    #[test]
    fn extracts_kickoff_prompt_forms() {
        assert_eq!(
            kickoff_prompt_from_text("Prompt: \"do the thing\""),
            Some("do the thing".to_owned())
        );
        assert_eq!(
            kickoff_prompt_from_text("Run the sprint：Prompt：\"全角冒号也认\""),
            Some("全角冒号也认".to_owned())
        );
        assert_eq!(
            kickoff_prompt_from_text("第一步：Prompt: \"  两侧空格  \" 之后"),
            Some("两侧空格".to_owned())
        );
        // 引号不闭合 / 没有 Prompt / 只有 Prompt 没有引号 → None。
        assert_eq!(kickoff_prompt_from_text("Prompt: \"unclosed"), None);
        assert_eq!(kickoff_prompt_from_text("no prompt here"), None);
        assert_eq!(kickoff_prompt_from_text("Prompt: no quotes"), None);
    }

    #[test]
    fn workflow_detail_without_kickoff_prompt_is_none() {
        let html = r#"<script type="application/ld+json">{"@context":"https://schema.org","@type":"HowTo","name":"Plain Workflow","tool":[{"@type":"HowToTool","name":"SEO Audit","url":"https://agenticskills.io/skills/seo-audit"}],"step":[{"@type":"HowToStep","position":1,"name":"Do it","text":"No prompt in this step."}]}</script>"#;

        let detail = parse_workflow_detail(html, "plain-workflow").expect("detail should parse");

        assert_eq!(detail.slug, "plain-workflow");
        // 没有 WebPage 时回落到 HowTo 的 name。
        assert_eq!(detail.name, "Plain Workflow");
        assert_eq!(detail.category, None);
        assert_eq!(detail.kickoff_prompt, None);
        assert_eq!(detail.components.len(), 1);
        assert_eq!(detail.steps.len(), 1);
    }

    #[test]
    fn install_skips_existing_components_and_isolates_failures() {
        let (paths, root) = temp_paths("workflow-install");
        // 预置：一个技能目录 + mcp.json 里的一个服务 → 两个组件都应当被跳过。
        fs::create_dir_all(paths.pi_home.join("skills/demo-skill")).expect("skill dir");
        fs::write(
            paths.pi_home.join("skills/demo-skill/SKILL.md"),
            "---\nname: demo-skill\ndescription: demo\n---\n",
        )
        .expect("SKILL.md");
        fs::write(
            paths.pi_home.join("mcp.json"),
            "{\"mcpServers\":{\"notion\":{\"command\":\"uvx\"}}}",
        )
        .expect("mcp.json");

        let components = vec![
            workflow_component("skill", "demo-skill"),
            workflow_component("skill", "fresh-skill"),
            workflow_component("mcp", "notion"),
            workflow_component("mcp", "firecrawl"),
            workflow_component("skill", "after-failure"),
        ];
        let (skills, mcp, skipped, failures) =
            install_workflow_components(&components, |component| {
                if workflow_component_installed(&paths, ConfigScope::Global, None, component)? {
                    return Ok(false);
                }
                if component.slug == "firecrawl" {
                    return Err(super::msg("market.agentic_mcp_no_config"));
                }
                Ok(true)
            });

        assert_eq!(skills, vec!["fresh-skill", "after-failure"]);
        assert!(mcp.is_empty());
        assert_eq!(skipped, vec!["demo-skill", "notion"]);
        // 失败只记进 failures，不影响后面的组件。
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].kind, "mcp");
        assert_eq!(failures[0].slug, "firecrawl");
        assert_eq!(
            message_code(&failures[0].error).as_deref(),
            Some("market.agentic_mcp_no_config")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_and_writes_installed_workflow_state() {
        let (paths, root) = temp_paths("workflow-state");

        // 文件不存在 → 空数组。
        assert!(read_installed_workflows(&paths, ConfigScope::Global, None)
            .expect("missing file reads as empty")
            .is_empty());

        let record = InstalledWorkflow {
            slug: "fullstack-saas".to_owned(),
            name: "Full-Stack SaaS".to_owned(),
            category: Some("Development".to_owned()),
            skill_count: 5,
            mcp_count: 4,
            installed_at: "1735689600000".to_owned(),
            components: vec![workflow_component("skill", "better-auth")],
        };
        store_installed_workflow(&paths, ConfigScope::Global, None, record.clone())
            .expect("state should be written");
        assert!(paths.pi_home.join("workflows.json").is_file());

        let mut other = record.clone();
        other.slug = "disciplined-development".to_owned();
        other.name = "Disciplined Development".to_owned();
        other.category = None;
        other.mcp_count = 0;
        store_installed_workflow(&paths, ConfigScope::Global, None, other.clone())
            .expect("second state should be written");

        // 按 slug 排序返回，组件一并读回。
        let read = read_installed_workflows(&paths, ConfigScope::Global, None)
            .expect("state should be read");
        assert_eq!(read, vec![other.clone(), record.clone()]);
        assert_eq!(read[1].components[0].kind, "skill");
        assert_eq!(read[1].components[0].slug, "better-auth");

        // 同 slug 再写一次是覆盖，不是追加。
        store_installed_workflow(&paths, ConfigScope::Global, None, other)
            .expect("state should be rewritten");
        assert_eq!(
            read_installed_workflows(&paths, ConfigScope::Global, None)
                .expect("read")
                .len(),
            2
        );

        // 项目范围缺 projectPath → 作用域路径本身非法，报既有错误码。
        assert_eq!(
            message_code(
                &read_installed_workflows(&paths, ConfigScope::Project, None)
                    .expect_err("missing project path")
            )
            .as_deref(),
            Some("agent_config.project_path_required")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installed_workflow_state_tolerates_broken_files_and_caps_entries() {
        let (paths, root) = temp_paths("workflow-state-broken");
        let file = paths.pi_home.join("workflows.json");

        fs::write(&file, "{ this is not json").expect("broken file");
        assert!(read_installed_workflows(&paths, ConfigScope::Global, None)
            .expect("broken json reads as empty")
            .is_empty());

        fs::write(&file, "{\"workflows\":{\"../escape\":{\"name\":\"Bad\"}}}").expect("bad slug");
        assert!(read_installed_workflows(&paths, ConfigScope::Global, None)
            .expect("bad slug is skipped")
            .is_empty());

        // 手写的残缺记录按默认值读回（name 回落 slug）。
        fs::write(&file, "{\"workflows\":{\"plain\":{\"installedAt\":\"1\"}}}")
            .expect("partial record");
        let read = read_installed_workflows(&paths, ConfigScope::Global, None).expect("read");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].name, "plain");
        assert_eq!(read[0].skill_count, 0);

        // 70 条 → 读取截断到 64 条。
        let records: Vec<String> = (0..70)
            .map(|index| format!("wf-{index:02}"))
            .map(|slug| format!("\"{slug}\":{{\"name\":\"{slug}\",\"installedAt\":\"{slug}\"}}"))
            .collect();
        fs::write(
            &file,
            format!("{{\"workflows\":{{{}}}}}", records.join(",")),
        )
        .expect("many records");
        assert_eq!(
            read_installed_workflows(&paths, ConfigScope::Global, None)
                .expect("read many")
                .len(),
            MAX_INSTALLED_WORKFLOWS
        );

        // 写入同样截断：超出的按 installedAt 从旧到新丢弃，保留最新 64 条。
        let mut workflows: BTreeMap<String, InstalledWorkflow> = BTreeMap::new();
        for index in 0..70u64 {
            let slug = format!("wf-{index:02}");
            workflows.insert(
                slug.clone(),
                InstalledWorkflow {
                    slug,
                    name: format!("Workflow {index}"),
                    installed_at: format!("{}", 1_700_000_000_000 + index),
                    ..InstalledWorkflow::default()
                },
            );
        }
        let bytes = installed_workflows_json(&workflows).expect("capped json");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let kept = value["workflows"].as_object().expect("object");
        assert_eq!(kept.len(), MAX_INSTALLED_WORKFLOWS);
        assert!(!kept.contains_key("wf-00"));
        assert!(kept.contains_key("wf-69"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore = "requires network access to agenticskills.io"]
    fn fetches_live_agenticskills_catalogs_and_details() {
        let cache = super::AgenticCatalogCache::default();
        let skills =
            super::cached_skill_catalog(&cache).expect("skill catalog should be reachable");
        assert!(skills.len() > 100, "expected a full skill catalog");
        let skills_with_category = skills
            .iter()
            .filter(|entry| entry.category.is_some())
            .count();
        let skills_with_source = skills
            .iter()
            .filter(|entry| entry.skill_md_url.is_some())
            .count();
        let skills_with_installs = skills
            .iter()
            .filter(|entry| entry.installs.is_some())
            .count();
        let skills_with_stars = skills.iter().filter(|entry| entry.stars.is_some()).count();
        let skills_heat_matches_stars = skills
            .iter()
            .filter(|entry| entry.heat == entry.stars)
            .count();
        println!(
            "skills={} withCategory={skills_with_category} withSkillMdUrl={skills_with_source} withInstalls={skills_with_installs} withStars={skills_with_stars} heatMatchesStars={skills_heat_matches_stars}",
            skills.len()
        );
        assert!(
            skills_with_category * 10 > skills.len() * 9,
            "more than 90% of skills should carry a category"
        );
        assert!(
            skills_with_source * 2 > skills.len(),
            "most skills should carry a SKILL.md url"
        );
        // 原始 star 数（排序用）：>90% 的技能应带星；技能热度与星数完全一致。
        assert!(
            skills_with_stars * 10 > skills.len() * 9,
            "more than 90% of skills should carry raw star counts"
        );
        assert_eq!(
            skills_heat_matches_stars,
            skills.len(),
            "skill heat should always mirror stars"
        );

        let mcp = super::cached_mcp_catalog(&cache).expect("MCP catalog should be reachable");
        assert!(mcp.len() > 100, "expected a full MCP catalog");
        let mcp_with_category = mcp.iter().filter(|entry| entry.category.is_some()).count();
        let mcp_with_transport = mcp
            .iter()
            .filter(|entry| !entry.transport.is_empty())
            .count();
        let mcp_with_snippets = mcp
            .iter()
            .filter(|entry| !entry.snippets.is_empty())
            .count();
        let mcp_with_stars = mcp.iter().filter(|entry| entry.stars.is_some()).count();
        let mcp_with_heat = mcp.iter().filter(|entry| entry.heat.is_some()).count();
        let weekly_visitors = mcp
            .iter()
            .filter(|entry| {
                entry
                    .popularity
                    .as_deref()
                    .is_some_and(|text| text.contains("visitors/wk"))
            })
            .collect::<Vec<_>>();
        let weekly_heat_above_1k = weekly_visitors
            .iter()
            .filter(|entry| entry.heat.is_some_and(|heat| heat > 1_000))
            .count();
        println!(
            "mcp={} withCategory={mcp_with_category} withTransport={mcp_with_transport} withSnippets={mcp_with_snippets} withStars={mcp_with_stars} withHeat={mcp_with_heat} weeklyVisitors={} weeklyHeatAbove1k={weekly_heat_above_1k}",
            mcp.len(),
            weekly_visitors.len()
        );
        assert!(
            mcp_with_category * 10 > mcp.len() * 9,
            "more than 90% of MCP entries should carry a category"
        );
        assert!(
            mcp_with_transport * 10 > mcp.len() * 9,
            "more than 90% of MCP entries should carry a transport"
        );
        assert!(
            mcp_with_snippets >= 50,
            "expected at least 50 MCP entries with config snippets"
        );
        // 热度的统一口径：站点只给 17 条 MCP 的 `githubStars`、20 条带数字的 `popularity`
        // （2026-06 实测 37/193），因此门槛按真实数据留出抖动空间。
        assert!(
            mcp_with_heat >= 30,
            "expected at least 30 MCP entries with a heat count"
        );
        assert!(
            mcp_with_stars > 0,
            "expected githubStars to reach at least one MCP entry"
        );
        assert!(
            !weekly_visitors.is_empty(),
            "expected visitors/wk entries on the MCP page"
        );
        assert_eq!(
            weekly_heat_above_1k,
            weekly_visitors.len(),
            "entries with visitors/wk popularity should carry heat above 1000"
        );
        // 第二次目录请求应当直接吃缓存里的 chunk URL（不用再拉列表页）。
        assert!(cache.chunk_url().is_some());

        let detail = super::fetch_skill_detail("taste-skill").expect("skill detail");
        assert!(detail.skill_md_url.is_some());
        let detail = super::fetch_mcp_detail("playwright").expect("MCP detail");
        assert!(!detail.snippets.is_empty());

        // 工作流：列表 ≥ 15 条且卡片元信息齐全；详情同时含技能与 MCP 组件；
        // seo-content-sprint 的 step 里带起手提示词。
        let workflows =
            super::cached_workflow_catalog().expect("workflow list should be reachable");
        let workflows_with_category = workflows
            .iter()
            .filter(|entry| entry.category.is_some())
            .count();
        let workflows_with_counts = workflows
            .iter()
            .filter(|entry| entry.skill_count > 0 || entry.mcp_count > 0)
            .count();
        println!(
            "workflows={} withCategory={workflows_with_category} withCounts={workflows_with_counts}",
            workflows.len()
        );
        assert!(
            workflows.len() >= 15,
            "expected at least 15 workflows on the list page"
        );
        assert_eq!(
            workflows_with_category,
            workflows.len(),
            "every workflow card should carry a category"
        );
        assert!(
            workflows_with_counts * 10 > workflows.len() * 9,
            "almost every workflow card should carry component counts"
        );

        let fullstack = super::fetch_workflow_detail("fullstack-saas").expect("workflow detail");
        let skill_components = fullstack
            .components
            .iter()
            .filter(|component| component.kind == "skill")
            .count();
        let mcp_components = fullstack
            .components
            .iter()
            .filter(|component| component.kind == "mcp")
            .count();
        println!(
            "fullstack-saas name={} components={} skills={skill_components} mcp={mcp_components} setupTime={:?} level={:?}",
            fullstack.name,
            fullstack.components.len(),
            fullstack.setup_time,
            fullstack.level
        );
        assert!(skill_components > 0, "fullstack-saas should list skills");
        assert!(mcp_components > 0, "fullstack-saas should list MCP servers");
        assert!(fullstack.setup_time.is_some());

        let sprint = super::fetch_workflow_detail("seo-content-sprint").expect("SEO workflow");
        println!("seo-content-sprint kickoff={:?}", sprint.kickoff_prompt);
        assert!(
            sprint.kickoff_prompt.is_some(),
            "seo-content-sprint should carry a kickoff prompt"
        );
    }
}
