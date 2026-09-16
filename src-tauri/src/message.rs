//! 结构化消息（Structured messages）。
//!
//! 后端**不返回最终文案**，只返回「消息码 + 参数」，由前端按当前语言渲染。
//! 这样 Rust 侧不再硬编码任何面向用户的自然语言，新增语言也不需要改后端。
//!
//! ## 线格式
//!
//! ```text
//! @msg:<code>
//! @msg:<code>?<key>=<value>&<key>=<value>
//! ```
//!
//! - `code`：`[a-z0-9._-]` 组成的稳定标识，例如 `runtime.dsh.pinned`。
//! - `key`：`[a-z0-9_]`，与前端文案里的 `{key}` 占位符对应。
//! - `value`：百分号转义的 UTF-8，避免 `?`、`&`、`=`、`%` 与非 ASCII 造成歧义。
//!
//! 前端 `src/lib/app-message.ts` 的 `resolveAppMessage()` 负责解析与渲染；
//! 遇到不认识的消息码时会原样显示 code，便于排查而不是丢信息。

/// 消息前缀。前端据此判断一个字符串是「消息码」还是普通文本。
pub const PREFIX: &str = "@msg:";

/// 需要转义的字符集合：分隔符 + 百分号本身。
fn needs_escape(byte: u8) -> bool {
    !matches!(byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
        | b'-' | b'_' | b'.' | b'~' | b'/' | b':' | b' '
    ) || byte == b'%'
}

/// 对参数值做百分号转义（UTF-8 逐字节）。
pub fn escape_param(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if needs_escape(*byte) {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        } else {
            out.push(*byte as char);
        }
    }
    out
}

/// 构造无参数消息码。
pub fn msg(code: &str) -> String {
    format!("{PREFIX}{code}")
}

/// 构造带参数消息码。
///
/// ```
/// # use deeppi::message::msg_with;
/// let text = msg_with("runtime.dsh.pinned", &[("latest", "0.2.0"), ("pinned", "0.1.5")]);
/// assert_eq!(text, "@msg:runtime.dsh.pinned?latest=0.2.0&pinned=0.1.5");
/// ```
pub fn msg_with(code: &str, params: &[(&str, &str)]) -> String {
    if params.is_empty() {
        return msg(code);
    }
    let mut out = String::with_capacity(code.len() + params.len() * 24 + PREFIX.len() + 1);
    out.push_str(PREFIX);
    out.push_str(code);
    out.push('?');
    for (index, (key, value)) in params.iter().enumerate() {
        if index > 0 {
            out.push('&');
        }
        out.push_str(key);
        out.push('=');
        out.push_str(&escape_param(value));
    }
    out
}

/// 把可能带参数的 code 归一化：只保留 code 部分（供日志/诊断使用）。
pub fn message_code(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(PREFIX)?;
    Some(rest.split('?').next().unwrap_or(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_messages_without_params() {
        assert_eq!(msg("app.startup.not_ready"), "@msg:app.startup.not_ready");
        assert_eq!(
            message_code(&msg("app.startup.not_ready")),
            Some("app.startup.not_ready")
        );
    }

    #[test]
    fn builds_a_message_with_params_in_order() {
        let text = msg_with(
            "runtime.dsh.pinned",
            &[("latest", "0.2.0"), ("pinned", "0.1.5")],
        );
        assert_eq!(text, "@msg:runtime.dsh.pinned?latest=0.2.0&pinned=0.1.5");
        assert_eq!(message_code(&text), Some("runtime.dsh.pinned"));
    }

    #[test]
    fn escapes_delimiters_and_non_ascii_values() {
        let text = msg_with("test.escape.delimiters", &[("detail", "a&b=c?d")]);
        assert_eq!(text, "@msg:test.escape.delimiters?detail=a%26b%3Dc%3Fd");
        // 中文按 UTF-8 逐字节转义，链路两端不依赖任何本地编码
        let text = msg_with("test.escape", &[("name", "工作区")]);
        assert!(text.starts_with("@msg:test.escape?name=%E5%B7%A5"));
    }
    #[test]
    fn an_empty_param_list_falls_back_to_the_bare_code() {
        assert_eq!(msg_with("test.empty", &[]), "@msg:test.empty");
    }

    #[test]
    fn plain_text_is_not_a_message() {
        assert_eq!(message_code("普通文本"), None);
        assert_eq!(message_code(""), None);
    }
}
