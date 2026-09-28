//! 站点英文内容翻译：用已配置的 Pi Provider 把站点文案批量翻译成中文。
//!
//! 契约（前端按此调用）：
//! - 命令 `translate_agentic_texts`，参数 `{ texts: string[], target: "zh-CN" | "zh-TW" }`；
//! - 返回与输入等长、同序的译文数组。
//!
//! 只有「提示词 + 缓存」这一层是本模块独有的：Provider 选择、凭据、鉴权头、
//! URL 与请求体的协议差异、响应解析全部复用
//! [`crate::provider::call_configured_model`]（与提示词增强同一条通路，
//! 不复制第二份 HTTP/凭据逻辑）。

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    io::Write,
    path::PathBuf,
    time::Duration,
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app_paths::AppPaths;
use crate::message::{msg, msg_with};
use crate::provider::{call_configured_model, ChatCallError};

/// 单次请求最多翻译多少条。
const MAX_TEXTS: usize = 6;
/// 单条原文（trim 后）最多多少字符。
const MAX_TEXT_CHARS: usize = 4_000;
/// 单次请求原文总字符上限。
const MAX_TOTAL_CHARS: usize = 12_000;
/// 与提示词增强一致：一次模型调用 60 秒。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// 翻译输出长度接近输入（12000 字符 ≈ 数千 token），给比提示词增强更大的输出预算。
const MAX_OUTPUT_TOKENS: u64 = 8_192;
/// 缓存上限：最多 2000 条 / 约 2 MiB，超限按插入顺序淘汰最旧（FIFO）。
const MAX_CACHE_ENTRIES: usize = 2_000;
const MAX_CACHE_BYTES: usize = 2 * 1024 * 1024;
/// 缓存位于 `<AppPaths.cache>/agenticskills/`。
const CACHE_DIR: &str = "agenticskills";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateRequest {
    pub texts: Vec<String>,
    pub target: String,
}

/// 目标语言；契约只允许 `zh-CN` / `zh-TW`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetLanguage {
    Simplified,
    Traditional,
}

impl TargetLanguage {
    fn from_tag(tag: &str) -> Option<Self> {
        match tag.trim() {
            "zh-CN" => Some(Self::Simplified),
            "zh-TW" => Some(Self::Traditional),
            _ => None,
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Simplified => "zh-CN",
            Self::Traditional => "zh-TW",
        }
    }

    /// 提示词里给模型看的语言描述（台湾用语单独说明）。
    fn label(self) -> &'static str {
        match self {
            Self::Simplified => "simplified Chinese (zh-CN)",
            Self::Traditional => "traditional Chinese as written in Taiwan (zh-TW)",
        }
    }
}

#[tauri::command]
pub async fn translate_agentic_texts(
    paths: tauri::State<'_, AppPaths>,
    request: TranslateRequest,
) -> Result<Vec<String>, String> {
    // State 只在命令签名里借用；worker 需要 'static，所以克隆出 AppPaths。
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || translate_inner(&paths, &request))
        .await
        .map_err(|error| {
            msg_with(
                "market.translate_failed",
                &[("error", &format!("translation worker failed: {error}"))],
            )
        })?
}

fn translate_inner(paths: &AppPaths, request: &TranslateRequest) -> Result<Vec<String>, String> {
    translate_with(paths, request, &|system, user| {
        match call_configured_model(paths, system, user, REQUEST_TIMEOUT, MAX_OUTPUT_TOKENS) {
            Ok(text) => Ok(text),
            // 没有可用的 Provider 时复用提示词增强已有的消息码。
            Err(ChatCallError::ProviderUnconfigured) => {
                Err(msg("prompt_enhance.provider_unconfigured"))
            }
            Err(error) => Err(msg_with(
                "market.translate_failed",
                &[("error", &error.detail())],
            )),
        }
    })
}

/// 翻译主流程；`call` 是「调一次模型」的注入点（测试注入假模型，生产走 Provider）。
fn translate_with(
    paths: &AppPaths,
    request: &TranslateRequest,
    call: &dyn Fn(&str, &str) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    let Some((target, texts)) = normalize_request(request)? else {
        // 空数组或全部空白：原样返回，不见模型。
        return Ok(request.texts.clone());
    };

    let mut cache = TranslationCache::load(cache_path(paths, target));
    let mut pending: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for text in texts.iter().filter(|text| !text.is_empty()) {
        if cache.get(text).is_none() && seen.insert(text.clone()) {
            pending.push(text.clone());
        }
    }

    if !pending.is_empty() {
        let raw = call(&system_prompt(target), &user_prompt(target, &pending))?;
        let translated = parse_translations(&raw, pending.len())
            .map_err(|error| msg_with("market.translate_failed", &[("error", &error)]))?;
        for (source, translated) in pending.into_iter().zip(translated) {
            cache.insert(&source, translated);
        }
        // 缓存写失败不影响翻译结果（下次只是再调一次模型）。
        if let Err(error) = cache.save() {
            log::warn!("event=agentic_translation_cache_write_failed error={error}");
        }
    }

    let mut output = Vec::with_capacity(request.texts.len());
    for (original, text) in request.texts.iter().zip(texts.iter()) {
        if text.is_empty() {
            // 空白项没有可翻译的内容，原样返回。
            output.push(original.clone());
            continue;
        }
        let Some(translated) = cache.get(text) else {
            return Err(msg_with(
                "market.translate_failed",
                &[("error", "translation is missing after the model call")],
            ));
        };
        output.push(translated.to_owned());
    }
    Ok(output)
}

/// 校验并归一化入参。
///
/// - `Err(msg("market.translate_request_invalid"))`：超条数、单条超长、总长超限、
///   目标语言非法；
/// - `Ok(None)`：`texts` 为空或全部是空白（调用方原样返回）；
/// - `Ok(Some(..))`：目标语言 + trim 后的原文（逐条与输入对齐，空白项为空串）。
fn normalize_request(
    request: &TranslateRequest,
) -> Result<Option<(TargetLanguage, Vec<String>)>, String> {
    if request.texts.len() > MAX_TEXTS {
        return Err(msg("market.translate_request_invalid"));
    }
    let Some(target) = TargetLanguage::from_tag(&request.target) else {
        return Err(msg("market.translate_request_invalid"));
    };
    let mut texts = Vec::with_capacity(request.texts.len());
    let mut total = 0usize;
    for text in &request.texts {
        let trimmed = text.trim();
        let length = trimmed.chars().count();
        if length > MAX_TEXT_CHARS {
            return Err(msg("market.translate_request_invalid"));
        }
        total += length;
        texts.push(trimmed.to_owned());
    }
    if total > MAX_TOTAL_CHARS {
        return Err(msg("market.translate_request_invalid"));
    }
    if texts.iter().all(String::is_empty) {
        return Ok(None);
    }
    Ok(Some((target, texts)))
}

/// 系统提示：只输出译文 JSON 数组，保留结构、代码与占位符。
fn system_prompt(target: TargetLanguage) -> String {
    format!(
        "You are a professional technical translator. Translate the user's texts into {label}.\n\
         Rules:\n\
         1. Preserve Markdown structure (headings, lists, tables, links), code blocks, inline code, shell commands, file paths, JSON, URLs and placeholders such as {{name}} exactly as they are.\n\
         2. Never translate or rewrite identifiers, command names, JSON keys, URLs or proper nouns; translate only natural-language prose.\n\
         3. Keep the meaning, numbers and formatting precise; never add, drop or explain anything.\n\
         4. Output ONLY a JSON array of translated strings with the same length and order as the input array.\n\
         Return no prose, no code fences and no commentary.",
        label = target.label(),
    )
}

/// 用户提示：给出待翻译的 JSON 数组，并复述目标语言与保留要求。
fn user_prompt(target: TargetLanguage, texts: &[String]) -> String {
    let payload = serde_json::to_string(texts).unwrap_or_else(|_| "[]".to_owned());
    format!(
        "Translate each string of this JSON array into {label}. Preserve Markdown structure, \
         code blocks, commands, JSON, URLs, placeholders such as {{name}} and proper nouns. \
         Return a JSON array of translated strings with the same length and order.\n\
         Input: {payload}",
        label = target.label(),
    )
}

/// 解析模型返回的 JSON 数组；容忍 ``` 包裹与前后说明文字。
fn parse_translations(raw: &str, expected: usize) -> Result<Vec<String>, String> {
    let json =
        extract_json_array(raw).ok_or_else(|| "model response has no JSON array".to_owned())?;
    let value: Value = serde_json::from_str(json)
        .map_err(|error| format!("model response is not valid JSON: {error}"))?;
    let items = value
        .as_array()
        .ok_or_else(|| "model response is not a JSON array".to_owned())?;
    let mut translated = Vec::with_capacity(items.len());
    for item in items {
        let text = item
            .as_str()
            .ok_or_else(|| "model response array contains a non-string item".to_owned())?;
        translated.push(text.to_owned());
    }
    if translated.len() != expected {
        return Err(format!(
            "model response has {} items, expected {expected}",
            translated.len()
        ));
    }
    Ok(translated)
}

/// 从模型输出里截出 JSON 数组（去掉 ``` 包裹与前后说明文字）。
fn extract_json_array(raw: &str) -> Option<&str> {
    let mut text = raw.trim();
    if let Some(rest) = text.strip_prefix("```") {
        // 跳过 fence 语言标记（如 ```json）。
        let rest = match rest.find('\n') {
            Some(index)
                if rest[..index]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-') =>
            {
                &rest[index + 1..]
            }
            _ => rest,
        };
        text = rest.trim().trim_end_matches("```").trim();
    }
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    (end > start).then(|| &text[start..=end])
}

/// 落盘的缓存结构：`entries` 是原文→译文的映射，`order` 记录插入顺序用于 FIFO 淘汰。
#[derive(Debug, Default, Deserialize, Serialize)]
struct CacheFile {
    #[serde(default)]
    entries: BTreeMap<String, String>,
    #[serde(default)]
    order: Vec<String>,
}

impl CacheFile {
    /// 丢弃 `order` 里的无效引用，并把缺失的键按排序补到末尾。
    fn normalize_order(&mut self) {
        let mut seen = BTreeSet::new();
        self.order
            .retain(|key| self.entries.contains_key(key) && seen.insert(key.clone()));
        for key in self.entries.keys() {
            if !seen.contains(key) {
                self.order.push(key.clone());
            }
        }
    }

    /// 估算序列化后的字节数（键/值长度 + JSON 结构开销，略偏保守）。
    fn estimated_bytes(&self) -> usize {
        let entries: usize = self
            .entries
            .iter()
            .map(|(key, value)| key.len() + value.len() + 8)
            .sum();
        let order: usize = self.order.iter().map(|key| key.len() + 5).sum();
        entries + order + 32
    }
}

/// `<cache>/agenticskills/translations-<target>.json` 的内存视图。
///
/// 坏文件（不存在、不是 JSON、结构不对）一律当作空缓存：翻译不能因为缓存坏掉而失败。
#[derive(Debug)]
struct TranslationCache {
    path: PathBuf,
    file: CacheFile,
}

impl TranslationCache {
    fn load(path: PathBuf) -> Self {
        let mut file = fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<CacheFile>(&content).ok())
            .unwrap_or_default();
        file.normalize_order();
        Self { path, file }
    }

    fn get(&self, source: &str) -> Option<&str> {
        self.file.entries.get(source).map(String::as_str)
    }

    fn insert(&mut self, source: &str, translated: String) {
        if self
            .file
            .entries
            .insert(source.to_owned(), translated)
            .is_none()
        {
            // 已存在的键不刷新淘汰顺序：同一个键重复写入算「早就插入过」。
            self.file.order.push(source.to_owned());
        }
        self.evict();
    }

    /// 超过条数或体积上限时，按插入顺序淘汰最旧（FIFO）。
    fn evict(&mut self) {
        while self.over_limit() {
            let Some(oldest) = self.file.order.first().cloned() else {
                break;
            };
            self.file.order.remove(0);
            self.file.entries.remove(&oldest);
        }
    }

    fn over_limit(&self) -> bool {
        self.file.entries.len() > MAX_CACHE_ENTRIES || self.file.estimated_bytes() > MAX_CACHE_BYTES
    }

    fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("failed to create translation cache directory: {error}")
            })?;
        }
        let bytes = serde_json::to_vec(&self.file)
            .map_err(|error| format!("failed to serialize translation cache: {error}"))?;
        let mut file = AtomicWriteFile::open(&self.path)
            .map_err(|error| format!("failed to open translation cache: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("failed to write translation cache: {error}"))?;
        file.commit()
            .map_err(|error| format!("failed to commit translation cache: {error}"))
    }
}

fn cache_path(paths: &AppPaths, target: TargetLanguage) -> PathBuf {
    paths
        .cache
        .join(CACHE_DIR)
        .join(format!("translations-{}.json", target.tag()))
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, fs, path::PathBuf};

    use super::{
        cache_path, parse_translations, system_prompt, translate_inner, translate_with,
        user_prompt, TargetLanguage, TranslateRequest, TranslationCache, MAX_CACHE_BYTES,
        MAX_CACHE_ENTRIES, MAX_TEXTS, MAX_TEXT_CHARS, MAX_TOTAL_CHARS,
    };
    use crate::app_paths::AppPaths;
    use crate::message::msg;

    fn test_paths(name: &str) -> (PathBuf, AppPaths) {
        let root =
            std::env::temp_dir().join(format!("deeppi-translate-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let paths = AppPaths::from_roots(root.join("roaming"), root.join("local"), root.clone())
            .expect("application paths should initialize");
        (root, paths)
    }

    fn request(texts: &[&str], target: &str) -> TranslateRequest {
        TranslateRequest {
            texts: texts.iter().map(|text| (*text).to_owned()).collect(),
            target: target.to_owned(),
        }
    }

    #[test]
    fn rejects_invalid_requests_without_calling_the_model() {
        let (root, paths) = test_paths("invalid");
        let calls = Cell::new(0usize);
        let call = |_: &str, _: &str| -> Result<String, String> {
            calls.set(calls.get() + 1);
            Ok("[]".to_owned())
        };

        // 超过 6 条。
        let too_many: Vec<String> = (0..MAX_TEXTS + 1)
            .map(|index| format!("text {index}"))
            .collect();
        let error = translate_with(
            &paths,
            &TranslateRequest {
                texts: too_many,
                target: "zh-CN".to_owned(),
            },
            &call,
        )
        .expect_err("too many texts should fail");
        assert_eq!(error, msg("market.translate_request_invalid"));

        // 单条超长。
        let long = "a".repeat(MAX_TEXT_CHARS + 1);
        let error = translate_with(&paths, &request(&[long.as_str()], "zh-CN"), &call)
            .expect_err("too long text should fail");
        assert_eq!(error, msg("market.translate_request_invalid"));

        // 总长超限（每条都不超过单条上限）。
        let chunk = "b".repeat(3_500);
        let texts: Vec<&str> = (0..4).map(|_| chunk.as_str()).collect();
        const {
            assert!(4 * 3_500 > MAX_TOTAL_CHARS);
        }
        let error = translate_with(&paths, &request(&texts, "zh-CN"), &call)
            .expect_err("over total length should fail");
        assert_eq!(error, msg("market.translate_request_invalid"));

        // 目标语言非法。
        let error = translate_with(&paths, &request(&["hello"], "en-US"), &call)
            .expect_err("unsupported target should fail");
        assert_eq!(error, msg("market.translate_request_invalid"));

        assert_eq!(calls.get(), 0, "invalid input must not reach the model");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn returns_blank_input_unchanged_without_calling_the_model() {
        let (root, paths) = test_paths("blank");
        let calls = Cell::new(0usize);
        let call = |_: &str, _: &str| -> Result<String, String> {
            calls.set(calls.get() + 1);
            Ok("[]".to_owned())
        };

        let empty = request(&[], "zh-CN");
        assert_eq!(
            translate_with(&paths, &empty, &call).expect("empty input should pass"),
            Vec::<String>::new()
        );

        let blank = request(&["   ", "\n\t", ""], "zh-TW");
        assert_eq!(
            translate_with(&paths, &blank, &call).expect("blank input should pass"),
            vec!["   ".to_owned(), "\n\t".to_owned(), String::new()]
        );
        assert_eq!(calls.get(), 0, "blank input must not reach the model");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn caches_translations_and_skips_the_model_on_hits() {
        let (root, paths) = test_paths("cache");
        let calls = Cell::new(0usize);
        let call = |_: &str, _: &str| -> Result<String, String> {
            calls.set(calls.get() + 1);
            Ok("```json\n[\"译文一\", \"译文二\"]\n```".to_owned())
        };

        let first = request(&["Hello world", "Second text"], "zh-CN");
        assert_eq!(
            translate_with(&paths, &first, &call).expect("translation should succeed"),
            vec!["译文一".to_owned(), "译文二".to_owned()]
        );
        assert_eq!(calls.get(), 1);

        // 第二次同样文本：全部命中缓存，模型即使失败也不能被调用。
        let fail = |_: &str, _: &str| -> Result<String, String> {
            Err("@msg:test.model_must_not_be_called".to_owned())
        };
        assert_eq!(
            translate_with(&paths, &first, &fail).expect("cache hit should succeed"),
            vec!["译文一".to_owned(), "译文二".to_owned()]
        );
        assert_eq!(calls.get(), 1);

        // 读回一致：重新从磁盘加载缓存也能拿到同样的映射。
        let cache = TranslationCache::load(cache_path(&paths, TargetLanguage::Simplified));
        assert_eq!(cache.get("Hello world"), Some("译文一"));
        assert_eq!(cache.get("Second text"), Some("译文二"));

        // 落盘结构：entries 以原文为键，order 保留插入顺序。
        let value: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(cache_path(&paths, TargetLanguage::Simplified))
                .expect("cache file should exist"),
        )
        .expect("cache file should be valid JSON");
        assert_eq!(value["entries"]["Hello world"], "译文一");
        assert_eq!(value["order"][0], "Hello world");
        assert_eq!(value["order"][1], "Second text");

        // zh-TW 使用独立缓存文件。
        assert_ne!(
            cache_path(&paths, TargetLanguage::Traditional),
            cache_path(&paths, TargetLanguage::Simplified)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_cache_file_is_treated_as_empty() {
        let (root, paths) = test_paths("corrupt");
        let path = cache_path(&paths, TargetLanguage::Simplified);
        fs::create_dir_all(path.parent().expect("cache path has a parent"))
            .expect("cache directory should be created");
        fs::write(&path, b"{ this is not json").expect("fixture should be written");

        let calls = Cell::new(0usize);
        let call = |_: &str, _: &str| -> Result<String, String> {
            calls.set(calls.get() + 1);
            Ok(r#"["译文"]"#.to_owned())
        };
        assert_eq!(
            translate_with(&paths, &request(&["Hello"], "zh-CN"), &call)
                .expect("corrupt cache must not fail translation"),
            vec!["译文".to_owned()]
        );
        assert_eq!(calls.get(), 1);

        // 一次成功的写入把坏文件替换成合法结构。
        let cache = TranslationCache::load(path.clone());
        assert_eq!(cache.get("Hello"), Some("译文"));
        assert!(
            serde_json::from_str::<serde_json::Value>(
                &fs::read_to_string(&path).expect("cache file should exist")
            )
            .is_ok(),
            "cache file should be rewritten as valid JSON"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn evicts_oldest_entries_when_the_entry_limit_is_exceeded() {
        let (root, paths) = test_paths("evict-entries");
        let mut cache = TranslationCache::load(cache_path(&paths, TargetLanguage::Simplified));
        for index in 0..MAX_CACHE_ENTRIES + 5 {
            cache.insert(&format!("source {index:04}"), format!("译文 {index}"));
        }
        assert_eq!(cache.file.entries.len(), MAX_CACHE_ENTRIES);
        assert_eq!(cache.file.order.len(), MAX_CACHE_ENTRIES);
        assert!(
            cache.get("source 0000").is_none(),
            "oldest entries should be evicted first"
        );
        let newest = format!("source {:04}", MAX_CACHE_ENTRIES + 4);
        let expected = format!("译文 {}", MAX_CACHE_ENTRIES + 4);
        assert_eq!(cache.get(&newest), Some(expected.as_str()));

        // 更新已有键不会产生重复的 order 记录。
        let mut small = TranslationCache::load(cache_path(&paths, TargetLanguage::Traditional));
        small.insert("a", "1".to_owned());
        small.insert("b", "2".to_owned());
        small.insert("a", "3".to_owned());
        assert_eq!(small.get("a"), Some("3"));
        assert_eq!(small.file.order, vec!["a".to_owned(), "b".to_owned()]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn evicts_oldest_entries_when_the_size_limit_is_exceeded() {
        let (root, paths) = test_paths("evict-size");
        let mut cache = TranslationCache::load(cache_path(&paths, TargetLanguage::Simplified));
        let value = "x".repeat(200 * 1024);
        for index in 0..32 {
            cache.insert(&format!("source {index}"), value.clone());
        }
        assert!(cache.file.entries.len() < 32, "size limit should evict");
        assert!(cache.file.estimated_bytes() <= MAX_CACHE_BYTES);
        assert!(cache.get("source 0").is_none());
        assert_eq!(cache.get("source 31"), Some(value.as_str()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prompts_state_the_target_language_and_preservation_rules() {
        let simplified = system_prompt(TargetLanguage::Simplified);
        assert!(simplified.contains("zh-CN"), "system prompt: {simplified}");
        assert!(simplified.contains("Markdown"));
        assert!(simplified.contains("code blocks"));
        assert!(simplified.contains("{name}"));
        assert!(simplified.contains("JSON array"));

        let traditional = system_prompt(TargetLanguage::Traditional);
        assert!(traditional.contains("zh-TW"));
        assert!(traditional.contains("Taiwan"));

        let user = user_prompt(
            TargetLanguage::Traditional,
            &["Hello".to_owned(), "World".to_owned()],
        );
        assert!(user.contains("zh-TW"));
        assert!(user.contains("Markdown"));
        assert!(user.contains("{name}"));
        assert!(user.contains(r#"["Hello","World"]"#));
    }

    #[test]
    fn parses_json_arrays_from_plain_fenced_and_prose_responses() {
        assert_eq!(
            parse_translations(r#"["甲","乙"]"#, 2).expect("plain array should parse"),
            vec!["甲".to_owned(), "乙".to_owned()]
        );
        assert_eq!(
            parse_translations("```json\n[\"甲\"]\n```", 1).expect("fenced array should parse"),
            vec!["甲".to_owned()]
        );
        assert_eq!(
            parse_translations("Here you go: [\"甲\"]", 1).expect("array with prose should parse"),
            vec!["甲".to_owned()]
        );
        assert!(parse_translations("no array here", 1).is_err());
        assert!(
            parse_translations(r#"["甲"]"#, 2).is_err(),
            "length mismatch"
        );
        assert!(parse_translations("[1, 2]", 2).is_err(), "non-string items");
    }

    #[test]
    fn model_receives_the_translation_prompt_and_sources() {
        let (root, paths) = test_paths("prompt-pass");
        let seen = std::cell::RefCell::new(Vec::<(String, String)>::new());
        let call = |system: &str, user: &str| -> Result<String, String> {
            seen.borrow_mut().push((system.to_owned(), user.to_owned()));
            Ok(r#"["译文"]"#.to_owned())
        };

        assert_eq!(
            translate_with(&paths, &request(&["Hello"], "zh-CN"), &call)
                .expect("translation should succeed"),
            vec!["译文".to_owned()]
        );
        let seen = seen.borrow();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].0.contains("zh-CN"));
        assert!(seen[0].0.contains("code blocks"));
        assert!(seen[0].1.contains("Hello"));
        drop(seen);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_texts_are_sent_to_the_model_once() {
        let (root, paths) = test_paths("duplicates");
        let calls = Cell::new(0usize);
        let prompts = std::cell::RefCell::new(Vec::<String>::new());
        let call = |_: &str, user: &str| -> Result<String, String> {
            calls.set(calls.get() + 1);
            prompts.borrow_mut().push(user.to_owned());
            Ok(r#"["相同译文"]"#.to_owned())
        };

        let result = translate_with(&paths, &request(&["Hello", "Hello"], "zh-CN"), &call)
            .expect("duplicate texts should translate");
        assert_eq!(result, vec!["相同译文".to_owned(), "相同译文".to_owned()]);
        assert_eq!(calls.get(), 1);
        assert_eq!(prompts.borrow()[0].matches("\"Hello\"").count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reports_unconfigured_provider_with_the_existing_message_code() {
        let (root, paths) = test_paths("no-provider");
        let error = translate_inner(&paths, &request(&["Hello"], "zh-CN"))
            .expect_err("a fresh temporary home has no configured provider");
        assert_eq!(error, msg("prompt_enhance.provider_unconfigured"));
        let _ = fs::remove_dir_all(root);
    }
}
