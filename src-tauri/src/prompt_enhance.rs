//! 提示词增强：用已配置的 Pi Provider 模型把用户输入改写成更清晰的提示词。
//!
//! 实现参考业界通行的 prompt 增强方案（角色化元提示 + 保持意图与语言 +
//! 仅输出改写结果，不额外发明需求），例如 OpenAI/Anthropic 官方的
//! prompt-improvement 指南所采用的结构化改写策略。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::message::{msg, msg_with};
use crate::{app_paths::AppPaths, provider::ProviderRecord};

const MAX_INPUT_CHARS: usize = 8_000;
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_OUTPUT_TOKENS: u64 = 2_048;

/// 元提示词：把用户输入改写成适合编码代理的提示词。
/// 关键约束：保持原意与语言、只做保守澄清、禁止额外发明需求、仅输出改写结果。
const META_PROMPT: &str = "You are an expert prompt engineer helping a user talk to a coding agent. Rewrite the user's message into a clear, actionable prompt.\nRules:\n1. Preserve the user's intent, facts, constraints, code references and language exactly (never translate).\n2. Add helpful structure (goal, context, constraints, expected output) only where it improves clarity.\n3. Resolve obvious ambiguities conservatively; never invent requirements or tasks the user did not ask for.\n4. Do not add explanations, comments or alternatives.\nReturn ONLY the rewritten prompt as plain text.";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancePromptRequest {
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancedPrompt {
    pub text: String,
}

#[tauri::command]
pub async fn enhance_prompt(
    app: AppHandle,
    request: EnhancePromptRequest,
) -> Result<EnhancedPrompt, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = app.state::<AppPaths>().inner().clone();
        enhance_inner(&paths, &request.text)
    })
    .await
    .map_err(|error| format!("prompt enhancement worker failed: {error}"))?
}

fn enhance_inner(paths: &AppPaths, text: &str) -> Result<EnhancedPrompt, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(msg("prompt_enhance.input_empty"));
    }
    if trimmed.chars().count() > MAX_INPUT_CHARS {
        return Err(msg_with(
            "prompt_enhance.input_too_long",
            &[("max", &MAX_INPUT_CHARS.to_string())],
        ));
    }
    let provider = crate::provider::list_provider_file(&paths.pi_models_file())?
        .into_iter()
        .find(|provider| !provider.models.is_empty())
        .ok_or_else(|| msg("prompt_enhance.provider_unconfigured"))?;
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
        .ok_or_else(|| msg("prompt_enhance.base_url_missing"))?;
    let url = chat_url(base_url, &api)?;
    let api_key = crate::credentials::provider_api_key(&provider.id)?;
    let agent = request_agent(&provider)?;
    let body = chat_request_body(&api, &model_id, trimmed);

    let request = agent
        .post(url.as_str())
        .header("Accept", "application/json");
    let request = crate::provider::apply_provider_auth(request, &provider, api_key.as_deref());
    let mut response = request.send_json(&body).map_err(|error| {
        msg_with(
            "prompt_enhance.request_failed",
            &[("error", &error.to_string())],
        )
    })?;
    let status = response.status().as_u16();
    let body_text = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(|error| format!("failed to read enhancement response: {error}"))?;
    if !(200..300).contains(&status) {
        return Err(msg_with(
            "prompt_enhance.http_failed",
            &[("status", &status.to_string())],
        ));
    }
    let text = parse_chat_response(&body_text, &api)?;
    let text = strip_wrapping(&text);
    if text.is_empty() {
        return Err(msg("prompt_enhance.response_empty"));
    }
    if text.chars().count() > MAX_INPUT_CHARS * 4 {
        return Err(msg("prompt_enhance.response_too_long"));
    }
    Ok(EnhancedPrompt { text })
}

/// 按 Provider API 类型构造聊天补全 URL。
fn chat_url(base_url: &str, api: &str) -> Result<String, String> {
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

fn chat_request_body(api: &str, model_id: &str, text: &str) -> Value {
    if api == "anthropic-messages" {
        json!({
            "model": model_id,
            "system": META_PROMPT,
            "messages": [{"role": "user", "content": text}],
            "max_tokens": MAX_OUTPUT_TOKENS,
        })
    } else if api == "openai-responses" {
        json!({
            "model": model_id,
            "instructions": META_PROMPT,
            "input": text,
            "max_output_tokens": MAX_OUTPUT_TOKENS,
        })
    } else {
        json!({
            "model": model_id,
            "messages": [
                {"role": "system", "content": META_PROMPT},
                {"role": "user", "content": text},
            ],
            "max_tokens": MAX_OUTPUT_TOKENS,
            "temperature": 0.4,
        })
    }
}

fn parse_chat_response(body: &str, _api: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("enhancement response is not valid JSON: {error}"))?;
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
    Err("enhancement response has no message text".into())
}

/// 去掉模型可能包裹的外层引号或 ```fence```。
fn strip_wrapping(text: &str) -> String {
    let trimmed = text.trim();
    let trimmed = if trimmed.len() >= 8 && trimmed.starts_with("```") {
        let inner = &trimmed[3..];
        // 跳过 fence 语言标记（如 ```text）。
        let inner = match inner.find('\n') {
            Some(idx)
                if inner[..idx]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-') =>
            {
                &inner[idx + 1..]
            }
            _ => inner,
        };
        inner.trim().trim_end_matches("```").trim()
    } else {
        trimmed
    };
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('“') && trimmed.ends_with('”')))
    {
        trimmed[1..trimmed.len() - 1].trim().to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// 与 provider::provider_agent 相同的超时策略，但给增强留 60 秒。
fn request_agent(provider: &ProviderRecord) -> Result<ureq::Agent, String> {
    let mut config = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false);
    if let Some(proxy) = provider.proxy.as_deref() {
        let proxy =
            ureq::Proxy::new(proxy).map_err(|error| format!("proxy is invalid: {error}"))?;
        config = config.proxy(Some(proxy));
    }
    Ok(config.build().new_agent())
}

#[cfg(test)]
mod tests {
    use super::{chat_url, parse_chat_response, strip_wrapping};

    #[test]
    fn builds_chat_urls_per_api() {
        assert_eq!(
            chat_url("https://api.example.com/v1", "openai-completions")
                .expect("url should build")
                .to_string(),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("https://api.example.com/v1", "openai-responses")
                .expect("url should build")
                .to_string(),
            "https://api.example.com/v1/responses"
        );
        assert_eq!(
            chat_url("https://api.anthropic.com", "anthropic-messages")
                .expect("url should build")
                .to_string(),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            chat_url("https://api.anthropic.com/v1", "anthropic-messages")
                .expect("url should build")
                .to_string(),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn parses_openai_completions_response() {
        let body = r#"{"choices":[{"message":{"content":"优化后的提示词"}}]}"#;
        assert_eq!(
            parse_chat_response(body, "openai-completions").expect("should parse"),
            "优化后的提示词"
        );
    }

    #[test]
    fn parses_responses_and_anthropic_payloads() {
        let responses =
            r#"{"output":[{"content":[{"type":"output_text","text":"better prompt"}]}]}"#;
        assert_eq!(
            parse_chat_response(responses, "openai-responses").expect("should parse"),
            "better prompt"
        );
        let anthropic = r#"{"content":[{"type":"text","text":"better prompt"}]}"#;
        assert_eq!(
            parse_chat_response(anthropic, "anthropic-messages").expect("should parse"),
            "better prompt"
        );
        assert!(parse_chat_response("not json", "openai-completions").is_err());
        assert!(parse_chat_response(r#"{"choices":[]}"#, "openai-completions").is_err());
    }

    #[test]
    fn strips_wrapping_fences_and_quotes() {
        assert_eq!(strip_wrapping("  \"干净提示\"  "), "干净提示");
        assert_eq!(strip_wrapping("```\n干净提示\n```"), "干净提示");
        assert_eq!(strip_wrapping("```text\n干净提示\n```"), "干净提示");
        assert_eq!(strip_wrapping("保持原样"), "保持原样");
    }
}
