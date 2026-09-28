//! 原生对话框文案（由前端传入）。
//!
//! 与 [`crate::theme`]、[`crate::diagnostics`] 的 `dialog_title` 参数遵循同一约定：
//! 后端**不持有任何面向用户的自然语言**。前端按当前语言把标题、按钮与正文模板
//! （`{name}` 占位符）传进来，后端只在确认时刻用运行时算出的实际值渲染。
//!
//! 这样原生对话框与界面其余部分共享同一套文案（`src/lib/app-messages-dialogs.ts`），
//! 新增语言不需要改后端；而确认文案里的路径、提交 ID、引用列表等仍然全部来自
//! 后端在执行时刻的实测值，不依赖前端可能已经过期的预览。

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::message::msg_with;

/// 标题字节上限。
const MAX_TITLE_BYTES: usize = 256;
/// 正文模板字节上限（正文含路径与提交摘要，比标题宽松）。
const MAX_MESSAGE_BYTES: usize = 8 * 1024;
/// 按钮文案字节上限。
const MAX_LABEL_BYTES: usize = 64;
/// 词汇表条目数上限。
const MAX_TERMS: usize = 32;
/// 词汇表总字节上限（键与值合计）。
const MAX_TERM_BYTES: usize = 8 * 1024;
/// 占位符名的字节上限。
const MAX_PLACEHOLDER_BYTES: usize = 32;

/// 前端传入的确认对话框文案。
///
/// 字段由前端按当前语言填好；`message` 里的 `{name}` 占位符由后端在确认时刻用
/// 实测值替换，`terms` 是后端可能要用到的词汇（例如「启用 / 关闭」），后端按
/// 运行时状态挑选，因此后端不需要自己拼自然语言。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmDialog {
    /// 对话框标题。
    pub title: String,
    /// 正文模板，`{name}` 为占位符。
    pub message: String,
    /// 确认按钮文案。
    pub confirm_label: String,
    /// 取消按钮文案。
    pub cancel_label: String,
    /// 附加词汇，键名稳定（后端按状态挑选），值已按当前语言翻译。
    #[serde(default)]
    pub terms: BTreeMap<String, String>,
}

impl ConfirmDialog {
    /// 校验前端文案，拒绝空串、超长正文与畸形词汇表。
    pub fn validate(&self) -> Result<(), String> {
        ensure_text("title", &self.title, MAX_TITLE_BYTES)?;
        ensure_text("message", &self.message, MAX_MESSAGE_BYTES)?;
        ensure_text("confirmLabel", &self.confirm_label, MAX_LABEL_BYTES)?;
        ensure_text("cancelLabel", &self.cancel_label, MAX_LABEL_BYTES)?;
        if self.terms.len() > MAX_TERMS {
            return Err(invalid("terms"));
        }
        let mut total = 0usize;
        for (key, value) in &self.terms {
            if !is_placeholder(key) {
                return Err(invalid("terms"));
            }
            total += key.len() + value.len();
        }
        if total > MAX_TERM_BYTES {
            return Err(invalid("terms"));
        }
        Ok(())
    }

    /// 取一条词汇；缺失时返回键名本身，漏配会直接显示在对话框里而不是静默变成空串。
    pub fn term<'a>(&'a self, key: &'a str) -> &'a str {
        self.terms.get(key).map(String::as_str).unwrap_or(key)
    }

    /// 渲染正文模板：`{name}` 用 `values` 或 `terms` 里的值替换。
    ///
    /// 模板里出现但没有对应值的占位符会**原样保留**，漏传参数可以直接在界面上看到。
    pub fn render(&self, values: &[(&str, String)]) -> String {
        render_template(&self.message, &self.terms, values)
    }

    /// 渲染一条词汇模板（例如「…共 {count} 个路径，完整范围已在应用中列出」）。
    pub fn render_term(&self, key: &str, values: &[(&str, String)]) -> String {
        match self.terms.get(key) {
            Some(template) => render_template(template, &self.terms, values),
            None => key.to_owned(),
        }
    }
}

/// 文案必须非空白且不超上限。
fn ensure_text(field: &str, value: &str, limit: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > limit {
        return Err(invalid(field));
    }
    Ok(())
}

fn invalid(field: &str) -> String {
    msg_with("app.dialog_text_invalid", &[("field", field)])
}

/// 占位符名：小写字母、数字与下划线，长度有界。
fn is_placeholder(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_PLACEHOLDER_BYTES
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn render_template(
    template: &str,
    terms: &BTreeMap<String, String>,
    values: &[(&str, String)],
) -> String {
    let mut out = String::with_capacity(template.len() + 64);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let Some(offset) = rest[start..].find('}') else {
            break;
        };
        let name = &rest[start + 1..start + offset];
        let replacement = if is_placeholder(name) {
            values
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.as_str())
                .or_else(|| terms.get(name).map(String::as_str))
        } else {
            None
        };
        match replacement {
            Some(value) => {
                out.push_str(&rest[..start]);
                out.push_str(value);
            }
            // 未知或非占位符的 `{...}` 原样保留。
            None => out.push_str(&rest[..start + offset + 1]),
        }
        rest = &rest[start + offset + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog(message: &str) -> ConfirmDialog {
        ConfirmDialog {
            title: "确认提交".into(),
            message: message.into(),
            confirm_label: "提交".into(),
            cancel_label: "取消".into(),
            terms: BTreeMap::from([("signature".to_owned(), "启用".to_owned())]),
        }
    }

    #[test]
    fn renders_placeholders_from_values_and_terms() {
        let text = dialog("签名：{signature}\n工作树：{worktree}")
            .render(&[("signature", "关闭".into()), ("worktree", "F:/项目".into())]);
        assert_eq!(text, "签名：关闭\n工作树：F:/项目");
    }

    #[test]
    fn keeps_unknown_placeholders_visible() {
        let text = dialog("{missing} 与 {Not_A_Placeholder} 与裸 {").render(&[]);
        assert_eq!(text, "{missing} 与 {Not_A_Placeholder} 与裸 {");
    }

    #[test]
    fn terms_fall_back_to_the_key() {
        assert_eq!(dialog("x").term("signature"), "启用");
        assert_eq!(dialog("x").term("sign_off"), "sign_off");
    }

    #[test]
    fn renders_a_term_template_with_a_count() {
        let mut dialog = dialog("{paths}");
        dialog
            .terms
            .insert("more_paths".into(), "…共 {count} 个路径".into());
        assert_eq!(
            dialog.render_term("more_paths", &[("count", "13".into())]),
            "…共 13 个路径"
        );
        assert_eq!(dialog.render_term("absent", &[]), "absent");
    }

    #[test]
    fn rejects_empty_or_oversized_text() {
        let mut broken = dialog("正文");
        broken.title = "  ".into();
        assert!(broken.validate().is_err());
        broken.title = "标题".into();
        broken.confirm_label = "x".repeat(MAX_LABEL_BYTES + 1);
        assert!(broken.validate().is_err());
        broken.confirm_label = "提交".into();
        broken.terms.insert("Bad-Key".into(), "值".into());
        assert!(broken.validate().is_err());
        broken.terms.clear();
        assert!(broken.validate().is_ok());
    }

    #[test]
    fn deserializes_the_frontend_payload_shape() {
        let payload = r#"{"title":"确认推送","message":"目标：{target}","confirmLabel":"推送","cancelLabel":"取消","terms":{"new_ref":"新建"}}"#;
        let dialog: ConfirmDialog = serde_json::from_str(payload).unwrap();
        assert_eq!(
            dialog.render(&[("target", "refs/heads/main".into())]),
            "目标：refs/heads/main"
        );
        assert_eq!(dialog.term("new_ref"), "新建");
        // 缺字段与多字段都视为前端契约错误。
        assert!(serde_json::from_str::<ConfirmDialog>(r#"{"title":"a"}"#).is_err());
        assert!(serde_json::from_str::<ConfirmDialog>(
            r#"{"title":"a","message":"b","confirmLabel":"c","cancelLabel":"d","extra":1}"#
        )
        .is_err());
    }
}
