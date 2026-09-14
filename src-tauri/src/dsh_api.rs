use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 0.1.5 Host API：POST /api/<namespace>/<method>，信封 payload 为 `{args:{…}}`。
const API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

fn dsh_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(API_TIMEOUT))
        // cookie 交换依赖 303 响应的 Set-Cookie 头，禁止跟随重定向。
        .max_redirects(0)
        .max_redirects_will_error(false)
        .build()
        .new_agent()
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DshSessionSummary {
    pub session_id: String,
    pub updated_at: i64,
    pub running: bool,
    pub blank: bool,
    pub cwd: Option<String>,
    #[serde(rename = "projections", default, deserialize_with = "projection_title")]
    pub title: Option<String>,
}

fn projection_title<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let projections = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(projections
        .as_ref()
        .and_then(|value| value.get("values"))
        .and_then(|value| value.get("title"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionListEnvelope {
    #[serde(rename = "type")]
    message_type: String,
    rpc_id: String,
    result: SessionListResult,
}

#[derive(Deserialize)]
struct SessionListResult {
    ok: bool,
    value: Option<SessionListValue>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct SessionListValue {
    items: Vec<DshSessionSummary>,
}

#[derive(Deserialize)]
struct RpcError {
    code: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionListRequest<'a> {
    #[serde(rename = "type")]
    message_type: &'static str,
    rpc_id: &'a str,
    method: &'static str,
    payload: SessionListPayload,
}

/// 0.1.5 Typert Remote 信封：payload 必须是单个 `args` 对象字段。
#[derive(Serialize)]
struct SessionListPayload {
    args: SessionListArgs,
}

/// `session/list` 的唯一参数名是 `_request`（空对象即可）。
#[derive(Serialize)]
struct SessionListArgs {
    #[serde(rename = "_request")]
    request: SessionListRequestArgs,
}

#[derive(Serialize)]
struct SessionListRequestArgs {}

pub fn list_sessions(base_url: &str, cookie: Option<&str>) -> Result<Vec<DshSessionSummary>, String> {
    if !base_url.starts_with("http://127.0.0.1:") {
        return Err("DSH API URL is not loopback".into());
    }
    let rpc_id = Uuid::new_v4().to_string();
    let envelope = SessionListRequest {
        message_type: "client-request",
        rpc_id: &rpc_id,
        method: "session/list",
        payload: SessionListPayload {
            args: SessionListArgs {
                request: SessionListRequestArgs {},
            },
        },
    };
    let agent = dsh_agent();
    let mut request = agent
        .post(format!("{base_url}/api/session/list"));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    let mut response = request
        .send_json(&envelope)
        .map_err(|error| format!("DSH session.list failed: {error}"))?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("failed to read DSH session.list: {error}"))?;
    parse_session_list(&body, &rpc_id)
}

/// 用启动 URL 里的 launch token 交换浏览器会话 cookie。
/// 0.1.5 起 Host API 请求必须携带 `dsh-auth-…` 签名 cookie，否则 401。
/// 返回 `name=value` 形式（可直接作为 Cookie 头），失败返回 `None`。
pub fn exchange_browser_cookie(api_base: &str, launch_url: &str) -> Result<Option<String>, String> {
    if !api_base.starts_with("http://127.0.0.1:") {
        return Err("DSH API URL is not loopback".into());
    }
    let response = dsh_agent()
        .get(launch_url)
        .call()
        .map_err(|error| format!("DSH cookie exchange failed: {error}"))?;
    // 有效 token 换取 303 重定向 + Set-Cookie；直接跟随重定向前的头即可。
    let status = response.status().as_u16();
    if !(300..400).contains(&status) {
        return Ok(None);
    }
    let Some(set_cookie) = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
    else {
        return Ok(None);
    };
    // 只取 name=value；属性（HttpOnly/SameSite 等）不需要回传。
    let pair = set_cookie.split(';').next().unwrap_or("").trim();
    if pair.is_empty() || !pair.contains('=') {
        return Ok(None);
    }
    Ok(Some(pair.to_owned()))
}

fn parse_session_list(body: &str, expected_rpc_id: &str) -> Result<Vec<DshSessionSummary>, String> {
    let envelope: SessionListEnvelope = serde_json::from_str(body)
        .map_err(|error| format!("invalid DSH session.list response: {error}"))?;
    if envelope.message_type != "server-response" {
        return Err("unexpected DSH response type".into());
    }
    if envelope.rpc_id != expected_rpc_id {
        return Err("DSH session.list rpcId mismatch".into());
    }
    if !envelope.result.ok {
        let error = envelope
            .result
            .error
            .map(|error| format!("{}: {}", error.code, error.message))
            .unwrap_or_else(|| "unknown DSH RPC error".into());
        return Err(error);
    }
    envelope
        .result
        .value
        .map(|value| value.items)
        .ok_or_else(|| "DSH session.list response has no value".into())
}

#[cfg(test)]
mod tests {
    use super::parse_session_list;

    #[test]
    fn parses_valid_session_list_response() {
        let body = r#"{
          "type":"server-response",
          "rpcId":"rpc-1",
          "result":{"ok":true,"value":{"items":[{
            "sessionId":"session-1",
            "updatedAt":123,
            "running":false,
            "blank":true,
            "cwd":"F:/project",
            "projections":{"asOfSeq":2,"values":{"title":"DSH task"}}
          }]}}
        }"#;

        let sessions = parse_session_list(body, "rpc-1").expect("response should parse");

        assert_eq!(sessions[0].session_id, "session-1");
        assert_eq!(sessions[0].title.as_deref(), Some("DSH task"));
        assert!(!sessions[0].running);
    }

    #[test]
    fn rejects_mismatched_rpc_id() {
        let body = r#"{"type":"server-response","rpcId":"other","result":{"ok":true,"value":{"items":[]}}}"#;

        assert!(parse_session_list(body, "rpc-1").is_err());
    }
}
