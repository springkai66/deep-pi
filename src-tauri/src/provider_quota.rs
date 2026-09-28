//! Verified provider quota and balance queries. Never return credentials or upstream text.
use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::{app_paths::AppPaths, credentials::provider_api_key};

const MAX_BODY: u64 = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaRequest {
    provider_id: String,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuotaWindow {
    remaining_percent: f64,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuotaWindows {
    #[serde(skip_serializing_if = "Option::is_none")]
    five_hour: Option<QuotaWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weekly: Option<QuotaWindow>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaResult {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    currency: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    windows: Option<QuotaWindows>,
}

impl QuotaResult {
    fn state(status: &'static str) -> Self {
        Self {
            status,
            amount: None,
            currency: None,
            windows: None,
        }
    }

    fn balance(amount: f64, currency: &'static str) -> Self {
        Self {
            status: if amount <= 0.0 {
                "exhausted"
            } else {
                "available"
            },
            amount: Some(amount),
            currency: Some(currency),
            windows: None,
        }
    }

    fn windows(five_hour: Option<f64>, weekly: Option<f64>) -> Self {
        if five_hour.is_none() && weekly.is_none() {
            return Self::state("error");
        }
        Self {
            status: if five_hour == Some(0.0) || weekly == Some(0.0) {
                "exhausted"
            } else {
                "available"
            },
            amount: None,
            currency: None,
            windows: Some(QuotaWindows {
                five_hour: five_hour.map(|remaining_percent| QuotaWindow { remaining_percent }),
                weekly: weekly.map(|remaining_percent| QuotaWindow { remaining_percent }),
            }),
        }
    }
}

fn parse_amount(value: &Value) -> Option<f64> {
    let amount = value.as_f64().or_else(|| value.as_str()?.parse().ok())?;
    (amount.is_finite() && amount >= 0.0).then_some(amount)
}

fn remaining_percent(value: &Value) -> Option<f64> {
    if !matches!(value.get("status")?.as_str()?, "ok" | "rate-limited") {
        return None;
    }
    let used = value.get("percent")?.as_f64()?;
    (used.is_finite() && (0.0..=100.0).contains(&used)).then_some(100.0 - used)
}

fn parse_codex_window(window: &Value) -> Option<(u64, f64)> {
    let seconds = window.get("limit_window_seconds")?.as_u64()?;
    let used = window.get("used_percent")?.as_f64()?;
    (used.is_finite() && (0.0..=100.0).contains(&used)).then_some((seconds, 100.0 - used))
}

fn parse_codex_usage(value: &Value) -> QuotaResult {
    let Some(rate_limit) = value.get("rate_limit").and_then(Value::as_object) else {
        return QuotaResult::state("error");
    };
    let mut five_hour = None;
    let mut weekly = None;
    for name in ["primary_window", "secondary_window"] {
        let Some(window) = rate_limit.get(name).filter(|value| !value.is_null()) else {
            continue;
        };
        let Some((seconds, remaining)) = parse_codex_window(window) else {
            return QuotaResult::state("error");
        };
        let slot = match seconds {
            18_000 => &mut five_hour,
            604_800 => &mut weekly,
            _ => continue,
        };
        if slot.replace(remaining).is_some() {
            return QuotaResult::state("error");
        }
    }
    QuotaResult::windows(five_hour, weekly)
}

fn parse_codex_balance(body: &str, account: &str) -> QuotaResult {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return QuotaResult::state("error");
    };
    if value
        .get("account_id")
        .is_some_and(|id| id.as_str() != Some(account))
    {
        return QuotaResult::state("error");
    }
    parse_codex_usage(&value)
}

fn parse_balance(provider: &str, body: &str) -> QuotaResult {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return QuotaResult::state("error");
    };
    if provider == "openai-codex" {
        return parse_codex_usage(&value);
    }
    if provider == "opencode-go" {
        let (Some(five_hour), Some(weekly)) = (
            value.pointer("/usage/rolling").and_then(remaining_percent),
            value.pointer("/usage/weekly").and_then(remaining_percent),
        ) else {
            return QuotaResult::state("error");
        };
        return QuotaResult::windows(Some(five_hour), Some(weekly));
    }
    if provider == "deepseek" {
        let Some(infos) = value.get("balance_infos").and_then(Value::as_array) else {
            return QuotaResult::state("error");
        };
        let Some(available) = value.get("is_available").and_then(Value::as_bool) else {
            return QuotaResult::state("error");
        };
        if infos.is_empty() {
            return QuotaResult::state("error");
        }
        let balances: Option<Vec<(f64, &'static str)>> = infos
            .iter()
            .map(|info| {
                let currency = match info.get("currency")?.as_str()? {
                    "CNY" => "CNY",
                    "USD" => "USD",
                    _ => return None,
                };
                Some((parse_amount(info.get("total_balance")?)?, currency))
            })
            .collect();
        let Some(balances) = balances else {
            return QuotaResult::state("error");
        };
        let Some((amount, currency)) = balances
            .iter()
            .copied()
            .find(|(amount, _)| *amount > 0.0)
            .or_else(|| balances.first().copied())
        else {
            return QuotaResult::state("error");
        };
        if !available && amount > 0.0 {
            return QuotaResult::state("unavailable");
        }
        return QuotaResult::balance(amount, currency);
    }
    if value.get("code").and_then(Value::as_i64) != Some(0)
        || value.get("status").and_then(Value::as_bool) != Some(true)
    {
        return QuotaResult::state("error");
    }
    let Some(amount) = value
        .pointer("/data/available_balance")
        .and_then(parse_amount)
    else {
        return QuotaResult::state("error");
    };
    QuotaResult::balance(
        amount,
        if provider == "moonshot-cn" {
            "CNY"
        } else {
            "USD"
        },
    )
}

fn managed_credential(paths: &AppPaths, provider: &str) -> Result<Option<(String, bool)>, ()> {
    let text = match fs::read_to_string(paths.pi_auth_file()) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    let root: Value = serde_json::from_str(&text).map_err(|_| ())?;
    let Some(entry) = root.get(provider) else {
        return Ok(None);
    };
    match entry.get("type").and_then(Value::as_str) {
        Some("api_key") => entry
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty())
            .map(|key| Some((key.to_owned(), true)))
            .ok_or(()),
        Some("oauth") => Ok(Some((String::new(), false))),
        _ => Err(()),
    }
}

fn managed_codex_token(paths: &AppPaths) -> Result<Option<(String, String)>, ()> {
    let text = match fs::read_to_string(paths.pi_auth_file()) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    let root: Value = serde_json::from_str(&text).map_err(|_| ())?;
    let Some(entry) = root.get("openai-codex") else {
        return Ok(None);
    };
    if entry.get("type").and_then(Value::as_str) != Some("oauth") {
        return Ok(None);
    }
    let access = entry.get("access").and_then(Value::as_str).ok_or(())?;
    let account = entry.get("accountId").and_then(Value::as_str).ok_or(())?;
    let expires = entry.get("expires").and_then(Value::as_u64).ok_or(())?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ())?
        .as_millis();
    if expires as u128 <= now + 30_000
        || access.is_empty()
        || access.len() > 16_384
        || !access
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_'))
        || account.is_empty()
        || account.len() > 256
        || !account
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err(());
    }
    Ok(Some((access.to_owned(), account.to_owned())))
}

fn official_url(provider: &str, paths: &AppPaths) -> Option<(&'static str, &'static str)> {
    if provider == "deepseek" {
        return Some(("https://api.deepseek.com/user/balance", "deepseek"));
    }
    if provider == "opencode-go" {
        return Some(("https://opencode.ai/zen/go/v1/usage", "opencode-go"));
    }
    if !matches!(provider, "moonshot" | "moonshotai") {
        return None;
    }
    let models: Value =
        serde_json::from_str(&fs::read_to_string(paths.pi_models_file()).ok()?).ok()?;
    let url = models
        .pointer(&format!("/providers/{provider}/baseUrl"))
        .and_then(Value::as_str)?;
    let parsed = tauri::Url::parse(url).ok()?;
    if parsed.scheme() != "https"
        || parsed.port().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    match parsed.host_str()? {
        "api.moonshot.ai" => Some(("https://api.moonshot.ai/v1/users/me/balance", "moonshot")),
        "api.moonshot.cn" => Some(("https://api.moonshot.cn/v1/users/me/balance", "moonshot-cn")),
        _ => None,
    }
}

fn query_codex(app: &AppHandle) -> QuotaResult {
    let paths = app.state::<AppPaths>();
    match managed_credential(&paths, "openai-codex") {
        Ok(None) => return QuotaResult::state("missing"),
        Ok(Some((_, true))) => return QuotaResult::state("unavailable"),
        Err(_) => return QuotaResult::state("error"),
        _ => {}
    }
    match crate::pi_auth::refresh_codex_oauth(app, &paths) {
        Ok(true) => {}
        Ok(false) => return QuotaResult::state("missing"),
        Err(()) => return QuotaResult::state("error"),
    }
    let (access, account) = match managed_codex_token(&paths) {
        Ok(Some(token)) => token,
        Ok(None) => return QuotaResult::state("missing"),
        Err(()) => return QuotaResult::state("error"),
    };
    fetch_balance(
        "https://chatgpt.com/backend-api/wham/usage",
        &access,
        Some(&account),
        "openai-codex",
    )
}

fn query(paths: &AppPaths, provider: &str) -> QuotaResult {
    // Vertex may use ADC outside DeepPi; do not inspect unrelated cloud credentials.
    if provider == "google-vertex" {
        return QuotaResult::state("unavailable");
    }
    if !matches!(
        provider,
        "anthropic"
            | "openai-codex"
            | "google"
            | "deepseek"
            | "z-ai"
            | "zai"
            | "kimi-coding"
            | "moonshot"
            | "moonshotai"
            | "qwen"
            | "opencode-go"
    ) {
        return QuotaResult::state("unavailable");
    }
    let stored = match managed_credential(paths, provider) {
        Ok(value) => value,
        Err(_) => return QuotaResult::state("error"),
    };
    // Pi's managed login takes precedence: an unrelated saved API key must not
    // masquerade as the balance of an active OAuth subscription.
    let key = match stored.as_ref() {
        Some((value, true)) => Some(value.clone()),
        Some(_) => None,
        None => match provider_api_key(provider) {
            Ok(value) => value,
            Err(_) => return QuotaResult::state("error"),
        },
    };
    if key.is_none() && stored.is_none() {
        return QuotaResult::state("missing");
    }
    let Some((url, parser)) = official_url(provider, paths) else {
        return QuotaResult::state("unavailable");
    };
    let Some(key) = key else {
        return QuotaResult::state("unavailable");
    };
    fetch_balance(url, &key, None, parser)
}

fn fetch_balance(url: &str, key: &str, account: Option<&str>, parser: &str) -> QuotaResult {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .proxy(crate::proxy::ureq_proxy())
        .build()
        .new_agent();
    let request = agent
        .get(url)
        .header("Authorization", format!("Bearer {key}"))
        .header("Accept", "application/json");
    let request = if let Some(account) = account {
        request.header("ChatGPT-Account-Id", account)
    } else {
        request
    };
    let Ok(mut response) = request.call() else {
        return QuotaResult::state("error");
    };
    if response.status().as_u16() != 200 {
        return QuotaResult::state("error");
    }
    let Ok(body) = response
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_string()
    else {
        return QuotaResult::state("error");
    };
    if let Some(account) = account {
        parse_codex_balance(&body, account)
    } else {
        parse_balance(parser, &body)
    }
}

#[tauri::command]
pub async fn provider_quota(app: AppHandle, request: QuotaRequest) -> QuotaResult {
    match tauri::async_runtime::spawn_blocking(move || {
        if request.provider_id == "openai-codex" {
            query_codex(&app)
        } else {
            query(&app.state::<AppPaths>(), &request.provider_id)
        }
    })
    .await
    {
        Ok(result) => result,
        Err(_) => QuotaResult::state("error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_mismatched_codex_account() {
        let body = r#"{"account_id":"other","rate_limit":{"primary_window":{"used_percent":10,"limit_window_seconds":18000}}}"#;
        assert_eq!(
            parse_codex_balance(body, "expected"),
            QuotaResult::state("error")
        );
        assert_eq!(
            parse_codex_balance(body, "other"),
            QuotaResult::windows(Some(90.0), None)
        );
        assert_eq!(
            parse_codex_balance(
                r#"{"account_id":null,"rate_limit":{"primary_window":{"used_percent":10,"limit_window_seconds":18000}}}"#,
                "other"
            ),
            QuotaResult::state("error")
        );
    }

    #[test]
    fn reads_only_managed_codex_oauth() {
        let root = std::env::temp_dir().join(format!("deeppi-codex-auth-{}", std::process::id()));
        let paths =
            AppPaths::from_roots(root.join("roaming"), root.join("local"), root.clone()).unwrap();
        assert_eq!(managed_codex_token(&paths), Ok(None));
        let valid = serde_json::json!({"type":"oauth","access":"header.payload.signature","accountId":"acct-123","expires":u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap() + 3_600_000});
        fs::write(
            paths.pi_auth_file(),
            serde_json::json!({"openai-codex":valid}).to_string(),
        )
        .unwrap();
        assert_eq!(
            managed_codex_token(&paths),
            Ok(Some(("header.payload.signature".into(), "acct-123".into())))
        );
        fs::write(
            paths.pi_auth_file(),
            serde_json::json!({"openai-codex":{"type":"api_key","key":"key"}}).to_string(),
        )
        .unwrap();
        assert_eq!(managed_codex_token(&paths), Ok(None));
        for bad in [
            serde_json::json!({"type":"oauth","access":"header.payload.signature","accountId":"acct-123","expires":1}),
            serde_json::json!({"type":"oauth","access":"header.payload.signature","accountId":"acct-123\r\nInjected: yes","expires":u64::MAX}),
            serde_json::json!({"type":"oauth","access":"invalid token","accountId":"acct-123","expires":u64::MAX}),
            serde_json::json!({"type":"oauth","access":"header.payload.signature","expires":u64::MAX}),
        ] {
            fs::write(
                paths.pi_auth_file(),
                serde_json::json!({"openai-codex":bad}).to_string(),
            )
            .unwrap();
            assert_eq!(managed_codex_token(&paths), Err(()));
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_codex_verified_windows_only() {
        let result = parse_balance(
            "openai-codex",
            r#"{"plan_type":"plus","rate_limit":{"allowed":true,"primary_window":{"used_percent":25,"limit_window_seconds":18000},"secondary_window":{"used_percent":80,"limit_window_seconds":604800}}}"#,
        );
        assert_eq!(result, QuotaResult::windows(Some(75.0), Some(20.0)));
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({"status":"available","windows":{"fiveHour":{"remainingPercent":75.0},"weekly":{"remainingPercent":20.0}}})
        );
        assert_eq!(
            parse_balance(
                "openai-codex",
                r#"{"rate_limit":{"primary_window":{"used_percent":100,"limit_window_seconds":604800}}}"#
            ),
            QuotaResult::windows(None, Some(0.0))
        );
        for body in [
            r#"{"rate_limit":{"primary_window":{"used_percent":33,"limit_window_seconds":3600}}}"#,
            r#"{"rate_limit":{"primary_window":{"used_percent":101,"limit_window_seconds":18000}}}"#,
            r#"{"rate_limit":{"primary_window":{"used_percent":-1,"limit_window_seconds":18000}}}"#,
            r#"{"rate_limit":null}"#,
            r#"{"error":"SECRET_ACCOUNT_DATA"}"#,
        ] {
            assert_eq!(
                parse_balance("openai-codex", body),
                QuotaResult::state("error")
            );
        }
    }

    #[test]
    fn parses_opencode_go_usage_windows() {
        let result = parse_balance(
            "opencode-go",
            r#"{"usage":{"rolling":{"status":"ok","percent":37.5,"resetsAt":"2026-09-28T00:00:00Z"},"weekly":{"status":"ok","percent":80,"resetsAt":"2026-10-01T00:00:00Z"},"monthly":{"status":"ok","percent":12}}}"#,
        );
        assert_eq!(result, QuotaResult::windows(Some(62.5), Some(20.0)));
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({
                "status":"available",
                "windows":{"fiveHour":{"remainingPercent":62.5},"weekly":{"remainingPercent":20.0}}
            })
        );
        assert_eq!(
            parse_balance(
                "opencode-go",
                r#"{"usage":{"rolling":{"status":"rate-limited","percent":100},"weekly":{"status":"ok","percent":0}}}"#
            ),
            QuotaResult::windows(Some(0.0), Some(100.0))
        );
    }

    #[test]
    fn rejects_unverified_opencode_go_usage() {
        for body in [
            r#"{"usage":{"rolling":{"percent":30},"weekly":{"percent":50}}}"#,
            r#"{"usage":{"rolling":{"status":"ok","percent":-1},"weekly":{"status":"ok","percent":50}}}"#,
            r#"{"usage":{"rolling":{"status":"ok","percent":101},"weekly":{"status":"ok","percent":50}}}"#,
            r#"{"usage":{"monthly":{"status":"ok","percent":50}}}"#,
            r#"{"type":"error","error":{"message":"SECRET_ACCOUNT_DATA"}}"#,
        ] {
            assert_eq!(
                parse_balance("opencode-go", body),
                QuotaResult::state("error")
            );
        }
    }

    #[test]
    fn parses_documented_balances_and_zero() {
        assert_eq!(
            parse_balance(
                "deepseek",
                r#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"13.25"}]}"#
            ),
            QuotaResult::balance(13.25, "CNY")
        );
        assert_eq!(
            parse_balance(
                "deepseek",
                r#"{"is_available":false,"balance_infos":[{"currency":"USD","total_balance":"0.00"}]}"#
            ),
            QuotaResult::balance(0.0, "USD")
        );
        assert_eq!(
            parse_balance(
                "moonshot",
                r#"{"code":0,"status":true,"data":{"available_balance":3.5}}"#
            ),
            QuotaResult::balance(3.5, "USD")
        );
        assert_eq!(
            parse_balance(
                "moonshot-cn",
                r#"{"code":0,"status":true,"data":{"available_balance":0}}"#
            ),
            QuotaResult::balance(0.0, "CNY")
        );
    }

    #[test]
    fn rejects_invalid_values_without_exposing_upstream_text() {
        for body in [
            r#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"NaN"}]}"#,
            r#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"-1"}]}"#,
            r#"{"is_available":true,"balance_infos":[]}"#,
        ] {
            assert_eq!(parse_balance("deepseek", body), QuotaResult::state("error"));
        }
        assert_eq!(
            parse_balance("moonshot", r#"{"error":"SECRET_ACCOUNT_DATA"}"#),
            QuotaResult::state("error")
        );
        assert!(!serde_json::to_string(&parse_balance(
            "moonshot",
            r#"{"error":"SECRET_ACCOUNT_DATA"}"#
        ))
        .unwrap()
        .contains("SECRET_ACCOUNT_DATA"));
    }

    #[test]
    fn only_documented_hosts_can_receive_moonshot_keys() {
        let root = std::env::temp_dir().join(format!("deeppi-quota-url-{}", std::process::id()));
        let paths =
            AppPaths::from_roots(root.join("roaming"), root.join("local"), root.clone()).unwrap();
        assert_eq!(
            official_url("deepseek", &paths).unwrap().0,
            "https://api.deepseek.com/user/balance"
        );
        assert_eq!(official_url("kimi-coding", &paths), None);
        assert_eq!(
            official_url("opencode-go", &paths).unwrap().0,
            "https://opencode.ai/zen/go/v1/usage"
        );
        for (base, accepted) in [
            ("https://api.moonshot.cn/v1", true),
            ("https://api.moonshot.ai/v1", true),
            ("https://api.moonshot.cn.evil.invalid/v1", false),
            ("http://api.moonshot.cn/v1", false),
            ("https://api.moonshot.cn:444/v1", false),
        ] {
            fs::write(
                paths.pi_models_file(),
                serde_json::json!({"providers":{"moonshot":{"baseUrl":base}}}).to_string(),
            )
            .unwrap();
            assert_eq!(
                official_url("moonshot", &paths).is_some(),
                accepted,
                "{base}"
            );
        }
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn reads_only_managed_api_keys() {
        let root = std::env::temp_dir().join(format!("deeppi-quota-auth-{}", std::process::id()));
        let paths =
            AppPaths::from_roots(root.join("roaming"), root.join("local"), root.clone()).unwrap();
        fs::write(paths.pi_auth_file(), r#"{"deepseek":{"type":"api_key","key":"test-key"},"opencode-go":{"type":"api_key","key":"go-key"},"anthropic":{"type":"oauth","access":"secret"}}"#).unwrap();
        assert_eq!(
            managed_credential(&paths, "deepseek").unwrap(),
            Some(("test-key".into(), true))
        );
        assert_eq!(
            managed_credential(&paths, "opencode-go").unwrap(),
            Some(("go-key".into(), true))
        );
        assert_eq!(
            managed_credential(&paths, "anthropic").unwrap(),
            Some((String::new(), false))
        );
        assert_eq!(managed_credential(&paths, "qwen").unwrap(), None);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn gemini_accounts_never_report_inferred_quota() {
        let root = std::env::temp_dir().join(format!("deeppi-quota-gemini-{}", std::process::id()));
        let paths =
            AppPaths::from_roots(root.join("roaming"), root.join("local"), root.clone()).unwrap();
        assert_eq!(managed_credential(&paths, "google").unwrap(), None);
        fs::write(
            paths.pi_auth_file(),
            r#"{"google":{"type":"api_key","key":"test-key"}}"#,
        )
        .unwrap();
        assert_eq!(official_url("google", &paths), None);
        assert_eq!(official_url("google-vertex", &paths), None);
        assert_eq!(query(&paths, "google"), QuotaResult::state("unavailable"));
        assert_eq!(
            query(&paths, "google-vertex"),
            QuotaResult::state("unavailable")
        );
        let _ = fs::remove_dir_all(root);
    }
}
