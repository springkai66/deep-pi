use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

#[derive(Serialize)]
struct SessionListPayload {}

pub fn list_sessions(base_url: &str) -> Result<Vec<DshSessionSummary>, String> {
    if !base_url.starts_with("http://127.0.0.1:") {
        return Err("DSH API URL is not loopback".into());
    }
    let rpc_id = Uuid::new_v4().to_string();
    let request = SessionListRequest {
        message_type: "client-request",
        rpc_id: &rpc_id,
        method: "session.list",
        payload: SessionListPayload {},
    };
    let mut response = ureq::post(format!("{base_url}/api/session.list"))
        .send_json(&request)
        .map_err(|error| format!("DSH session.list failed: {error}"))?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("failed to read DSH session.list: {error}"))?;
    parse_session_list(&body, &rpc_id)
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
