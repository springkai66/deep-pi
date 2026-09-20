//! 全局网络代理：作用于 DeepPi 自身的 HTTP 客户端与所有 Node 子进程
//!（Pi RPC/TUI 会话、DSH、npm 包管理、pi 登录）。
//!
//! 子进程里的模型请求由 pi 的 `fetch` 发出，Node 24 只有在
//! `NODE_USE_ENV_PROXY=1` 且设置了 `HTTP(S)_PROXY` 时才会走代理，
//! 因此手动模式必须同时注入该变量；ureq/reqwest（更新器、市场、运行时下载）
//! 则从进程环境变量读取。

use std::{
    process::Command,
    sync::RwLock,
    time::{Duration, Instant},
};

use crate::settings::AppSettings;

/// 手动模式允许的代理地址最长长度。
const MAX_PROXY_LENGTH: usize = 512;
/// 连通性测试端点（与更新检查同源，代理不通时能明确失败）。
const TEST_URL: &str = "https://api.github.com/";
/// 需要与子进程同步的代理环境变量（大小写两套：Node/ureq 用大写，
/// git/curl 约定小写）。
const PROXY_VARIABLES: [&str; 8] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "http_proxy",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ProxyState {
    mode: String,
    url: String,
    no_proxy: String,
}

static STATE: RwLock<ProxyState> = RwLock::new(ProxyState {
    mode: String::new(),
    url: String::new(),
    no_proxy: String::new(),
});

/// 规范化代理模式；未知值回落 `system`（跟随进程/系统环境变量）。
pub fn normalize_mode(mode: &str) -> &'static str {
    match mode {
        "direct" => "direct",
        "manual" => "manual",
        _ => "system",
    }
}

/// 校验手动代理地址：`scheme://host[:port]`，允许 http/https，不允许凭据与查询串。
pub fn validate_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("proxy address is required in manual mode".into());
    }
    if value.len() > MAX_PROXY_LENGTH {
        return Err("proxy address is too long".into());
    }
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err("proxy address must start with http:// or https://".into());
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err("proxy scheme must be http or https".into());
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty()
        || authority.contains('@')
        || !authority
            .chars()
            .all(|character| character.is_ascii_graphic())
    {
        return Err("proxy address host is invalid".into());
    }
    Ok(format!("{scheme}://{authority}"))
}

/// 校验整组代理设置（保存设置时由 settings::validate 调用）。
pub fn validate(settings: &AppSettings) -> Result<(), String> {
    if normalize_mode(&settings.proxy_mode) == "manual" {
        validate_url(&settings.proxy_url)?;
    }
    if settings.proxy_no_proxy.len() > MAX_PROXY_LENGTH {
        return Err("no-proxy list is too long".into());
    }
    Ok(())
}

fn snapshot() -> ProxyState {
    STATE.read().map(|state| state.clone()).unwrap_or_default()
}

fn set_no_proxy(value: &str) {
    for key in ["NO_PROXY", "no_proxy"] {
        if value.is_empty() {
            std::env::remove_var(key);
        } else {
            std::env::set_var(key, value);
        }
    }
}

/// 应用启动与设置保存时调用：更新全局状态并同步进程环境变量
///（ureq/reqwest/更新器从这里读取；子进程另有 apply_to_command 注入）。
pub fn configure(settings: &AppSettings) {
    let state = ProxyState {
        mode: normalize_mode(&settings.proxy_mode).to_owned(),
        url: settings.proxy_url.trim().to_owned(),
        no_proxy: settings.proxy_no_proxy.trim().to_owned(),
    };
    if let Ok(mut guard) = STATE.write() {
        *guard = state.clone();
    }
    match state.mode.as_str() {
        "manual" => {
            for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
                std::env::set_var(key, &state.url);
            }
            set_no_proxy(&state.no_proxy);
        }
        "direct" => {
            for key in PROXY_VARIABLES {
                std::env::remove_var(key);
            }
        }
        // system：保持进程环境原样，交给系统/用户环境变量。
        _ => {}
    }
}

/// 给 Node 子进程注入/清理代理环境变量。
/// 手动模式额外设置 `NODE_USE_ENV_PROXY=1`，否则 pi/DSH 的 fetch 不会走代理。
pub fn apply_to_command(command: &mut Command) {
    apply_state_to_command(command, &snapshot());
}

fn apply_state_to_command(command: &mut Command, state: &ProxyState) {
    match state.mode.as_str() {
        "manual" => {
            for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
                command.env(key, &state.url);
            }
            command.env("NODE_USE_ENV_PROXY", "1");
            if !state.no_proxy.is_empty() {
                command.env("NO_PROXY", &state.no_proxy);
                command.env("no_proxy", &state.no_proxy);
            }
        }
        "direct" => {
            for key in PROXY_VARIABLES {
                command.env_remove(key);
            }
            command.env_remove("NODE_USE_ENV_PROXY");
        }
        _ => {}
    }
}

/// `portable-pty` 的命令构建器版本（走终端会话的 pi TUI）。
pub fn apply_to_pty(command: &mut portable_pty::CommandBuilder) {
    let state = snapshot();
    match state.mode.as_str() {
        "manual" => {
            for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
                command.env(key, &state.url);
            }
            command.env("NODE_USE_ENV_PROXY", "1");
            if !state.no_proxy.is_empty() {
                command.env("NO_PROXY", &state.no_proxy);
                command.env("no_proxy", &state.no_proxy);
            }
        }
        "direct" => {
            for key in PROXY_VARIABLES {
                command.env_remove(key);
            }
            command.env_remove("NODE_USE_ENV_PROXY");
        }
        _ => {}
    }
}

/// 给 ureq 客户端用的代理：手动模式用显式地址，直连模式为 None，
/// 跟随系统则读取环境变量（HTTP(S)_PROXY / NO_PROXY）。
pub fn ureq_proxy() -> Option<ureq::Proxy> {
    let state = snapshot();
    match state.mode.as_str() {
        "manual" => ureq::Proxy::new(&state.url).ok(),
        "direct" => None,
        _ => ureq::Proxy::try_from_env(),
    }
}

/// 当前应使用的代理地址（手动模式用显式地址；跟随系统时读环境变量；直连为 None）。
pub fn current_url() -> Option<String> {
    let state = snapshot();
    match state.mode.as_str() {
        "manual" => Some(state.url.clone()).filter(|url| !url.is_empty()),
        "direct" => None,
        _ => ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"]
            .iter()
            .find_map(|key| std::env::var(key).ok())
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
    }
}

fn probe(proxy: Option<ureq::Proxy>) -> Result<String, String> {
    let mut config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(8)))
        .timeout_global(Some(Duration::from_secs(12)))
        .https_only(true);
    config = config.proxy(proxy);
    let agent = config.build().new_agent();
    let started = Instant::now();
    let response = agent
        .get(TEST_URL)
        .header("User-Agent", "DeepPi")
        .call()
        .map_err(|error| format!("{error}"))?;
    let status = response.status().as_u16();
    Ok(format!(
        "HTTP {status} · {} ms",
        started.elapsed().as_millis()
    ))
}

/// 按给定配置测试代理连通性（不依赖全局状态，避免与防抖保存竞态）。
#[tauri::command]
pub async fn proxy_test(mode: String, url: String) -> Result<String, String> {
    let proxy = match normalize_mode(&mode) {
        "direct" => None,
        "manual" => {
            let url = validate_url(&url)?;
            Some(
                ureq::Proxy::new(&url)
                    .map_err(|error| format!("proxy address is invalid: {error}"))?,
            )
        }
        _ => ureq::Proxy::try_from_env(),
    };
    tauri::async_runtime::spawn_blocking(move || probe(proxy))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_unknown_modes_to_system() {
        assert_eq!(normalize_mode("manual"), "manual");
        assert_eq!(normalize_mode("direct"), "direct");
        assert_eq!(normalize_mode("system"), "system");
        assert_eq!(normalize_mode(""), "system");
        assert_eq!(normalize_mode("nonsense"), "system");
    }

    #[test]
    fn validates_manual_proxy_addresses() {
        assert_eq!(
            validate_url("http://127.0.0.1:7890").unwrap(),
            "http://127.0.0.1:7890"
        );
        assert!(validate_url("https://proxy.local:8443").is_ok());
        assert!(validate_url("  http://127.0.0.1:7890/  ").is_ok());
        assert!(validate_url("").is_err());
        assert!(validate_url("127.0.0.1:7890").is_err());
        assert!(validate_url("ftp://127.0.0.1:7890").is_err());
        assert!(validate_url("socks5://127.0.0.1:7890").is_err());
        assert!(validate_url("http://user:pass@127.0.0.1:7890").is_err());
        assert!(validate_url("http://").is_err());
    }

    #[test]
    fn settings_validation_requires_url_only_in_manual_mode() {
        let mut settings = AppSettings {
            proxy_mode: "system".into(),
            proxy_url: String::new(),
            ..AppSettings::default()
        };
        assert!(validate(&settings).is_ok());
        settings.proxy_mode = "manual".into();
        assert!(validate(&settings).is_err());
        settings.proxy_url = "http://127.0.0.1:7890".into();
        assert!(validate(&settings).is_ok());
    }

    #[test]
    fn command_injection_sets_node_proxy_flag_and_clears_in_direct_mode() {
        let envs = |state: &ProxyState| {
            let mut command = Command::new("node");
            apply_state_to_command(&mut command, state);
            command
                .get_envs()
                .map(|(key, value)| {
                    (
                        key.to_string_lossy().into_owned(),
                        value.map(|value| value.to_string_lossy().into_owned()),
                    )
                })
                .collect::<Vec<_>>()
        };
        let manual = envs(&ProxyState {
            mode: "manual".into(),
            url: "http://127.0.0.1:7890".into(),
            no_proxy: "localhost".into(),
        });
        assert!(manual.contains(&("HTTPS_PROXY".into(), Some("http://127.0.0.1:7890".into()))));
        assert!(manual.contains(&("NODE_USE_ENV_PROXY".into(), Some("1".into()))));
        assert!(manual.contains(&("NO_PROXY".into(), Some("localhost".into()))));

        let direct = envs(&ProxyState {
            mode: "direct".into(),
            url: String::new(),
            no_proxy: String::new(),
        });
        assert!(direct.contains(&("HTTP_PROXY".into(), None)));
        assert!(direct.contains(&("NODE_USE_ENV_PROXY".into(), None)));

        // system：不动子进程环境（继承父进程）。
        assert!(envs(&ProxyState {
            mode: "system".into(),
            url: String::new(),
            no_proxy: String::new(),
        })
        .is_empty());
    }
}
