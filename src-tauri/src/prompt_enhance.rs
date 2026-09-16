//! 提示词增强：用已配置的 Pi Provider 模型把用户输入改写成更清晰的提示词。
//!
//! 实现参考业界通行的 prompt 增强方案（角色化元提示 + 保持意图与语言 +
//! 仅输出改写结果，不额外发明需求），例如 OpenAI/Anthropic 官方的
//! prompt-improvement 指南所采用的结构化改写策略。
//!
//! Provider 调用链路（读 models.json、凭据、鉴权头、URL 与请求体协议、
//! 响应解析）已抽取到 [`crate::provider::call_configured_model`]：站点翻译
//! 与本模块共用同一条通路，这里只保留增强自己的输入校验、元提示词、
//! 输出清理与消息码。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::app_paths::AppPaths;
use crate::message::{msg, msg_with};
use crate::provider::ChatCallError;

const MAX_INPUT_CHARS: usize = 8_000;
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
    let text = crate::provider::call_configured_model(
        paths,
        META_PROMPT,
        trimmed,
        REQUEST_TIMEOUT,
        MAX_OUTPUT_TOKENS,
    )
    .map_err(enhance_error)?;
    let text = strip_wrapping(&text);
    if text.is_empty() {
        return Err(msg("prompt_enhance.response_empty"));
    }
    if text.chars().count() > MAX_INPUT_CHARS * 4 {
        return Err(msg("prompt_enhance.response_too_long"));
    }
    Ok(EnhancedPrompt { text })
}

/// 把共用的调用错误映射回提示词增强既有的消息码与原始文案（行为保持不变）。
fn enhance_error(error: ChatCallError) -> String {
    match error {
        ChatCallError::ProviderUnconfigured => msg("prompt_enhance.provider_unconfigured"),
        ChatCallError::BaseUrlMissing => msg("prompt_enhance.base_url_missing"),
        ChatCallError::Request(error) => {
            msg_with("prompt_enhance.request_failed", &[("error", &error)])
        }
        ChatCallError::Http(status) => msg_with(
            "prompt_enhance.http_failed",
            &[("status", &status.to_string())],
        ),
        // 下面两种在既有实现里就是原始英文文案（不是消息码），保持原样。
        ChatCallError::ResponseRead(error) => {
            format!("failed to read enhancement response: {error}")
        }
        ChatCallError::ResponseInvalid(detail) => format!("enhancement response {detail}"),
        ChatCallError::Local(error) => error,
    }
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

#[cfg(test)]
mod tests {
    use super::{enhance_error, strip_wrapping};
    use crate::message::{msg, msg_with};
    use crate::provider::{chat_url, parse_chat_response, ChatCallError};

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
            parse_chat_response(body).expect("should parse"),
            "优化后的提示词"
        );
    }

    #[test]
    fn parses_responses_and_anthropic_payloads() {
        let responses =
            r#"{"output":[{"content":[{"type":"output_text","text":"better prompt"}]}]}"#;
        assert_eq!(
            parse_chat_response(responses).expect("should parse"),
            "better prompt"
        );
        let anthropic = r#"{"content":[{"type":"text","text":"better prompt"}]}"#;
        assert_eq!(
            parse_chat_response(anthropic).expect("should parse"),
            "better prompt"
        );
        assert!(parse_chat_response("not json").is_err());
        assert!(parse_chat_response(r#"{"choices":[]}"#).is_err());
    }

    #[test]
    fn strips_wrapping_fences_and_quotes() {
        assert_eq!(strip_wrapping("  \"干净提示\"  "), "干净提示");
        assert_eq!(strip_wrapping("```\n干净提示\n```"), "干净提示");
        assert_eq!(strip_wrapping("```text\n干净提示\n```"), "干净提示");
        assert_eq!(strip_wrapping("保持原样"), "保持原样");
    }

    /// 抽取 helper 后，原有错误码与原始文案必须逐字节一致。
    #[test]
    fn maps_shared_chat_errors_to_the_existing_codes() {
        assert_eq!(
            enhance_error(ChatCallError::ProviderUnconfigured),
            msg("prompt_enhance.provider_unconfigured")
        );
        assert_eq!(
            enhance_error(ChatCallError::BaseUrlMissing),
            msg("prompt_enhance.base_url_missing")
        );
        assert_eq!(
            enhance_error(ChatCallError::Request("boom".into())),
            msg_with("prompt_enhance.request_failed", &[("error", "boom")])
        );
        assert_eq!(
            enhance_error(ChatCallError::Http(500)),
            "@msg:prompt_enhance.http_failed?status=500"
        );
        assert_eq!(
            enhance_error(ChatCallError::ResponseRead("broken pipe".into())),
            "failed to read enhancement response: broken pipe"
        );
        assert_eq!(
            enhance_error(ChatCallError::ResponseInvalid("has no message text".into())),
            "enhancement response has no message text"
        );
        assert_eq!(
            enhance_error(ChatCallError::Local("bad url".into())),
            "bad url"
        );
    }
}
