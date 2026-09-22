//! 全局网络代理：作用于 DeepPi 自身的 HTTP 客户端与所有 Node 子进程
//! （Pi RPC/TUI 会话、DSH、npm 包管理、pi 登录）。
//!
//! 子进程里的模型请求由 pi 的 `fetch` 发出，Node 24 只有在
//! `NODE_USE_ENV_PROXY=1` 且设置了 `HTTP(S)_PROXY` 时才会走代理，
//! 因此手动模式必须同时注入该变量；ureq/reqwest（更新器、市场、运行时下载）
//! 则从进程环境变量读取。
//!
//! 三种模式的目标行为：
//! - 跟随系统：优先读取代理环境变量；未设置时检测 Windows 系统代理
//!   （注册表 `Internet Settings`，要求 `ProxyEnable=1`，避免残留配置劫持
//!   全部流量），把结果注入子进程并交给各 HTTP 客户端。TUN 模式在网络层
//!   透明接管，无需（也无法）在应用层干预。
//! - 直连：清除全部代理变量；宿主进程额外写入 `NO_PROXY=*`，让不走
//!   `apply_to_command` 的 in-process 客户端（tauri 更新器的 reqwest）也直连。
//! - 手动：显式地址 + `NODE_USE_ENV_PROXY=1` + NO_PROXY。
//!
//! 例外地址始终合并本地默认（localhost/127.0.0.1/::1/[::1]），否则开启代理后
//! 本地目标（DSH 本地 API、Ollama 等本地 Provider）会被错误送进代理。

use std::{
    process::Command,
    sync::RwLock,
    time::{Duration, Instant},
};

use tauri::Url;

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
/// 手动/跟随系统注入时写入代理地址的键。
const PROXY_ADDRESS_KEYS: [&str; 4] = ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"];
/// 判断「环境已有代理」时检查的键（与 pi 桥接注入的判定保持一致）。
const PROXY_ENV_LOOKUP_KEYS: [&str; 4] = ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"];
/// 例外地址本地默认：本地目标永不经代理，本地 Provider/DSH 不因代理而失联。
const LOCAL_NO_PROXY_DEFAULTS: [&str; 4] = ["localhost", "127.0.0.1", "::1", "[::1]"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ProxyState {
    mode: String,
    url: String,
    no_proxy: String,
    /// 首次改写进程环境前保存的原始值（键 → 原值；None 表示原本未设置），
    /// 切回「跟随系统」时恢复，避免模式切换永久丢失用户自设的环境变量。
    saved_env: Vec<(String, Option<String>)>,
}

/// configure 写者互斥：捕获/恢复/发布状态与环境写入作为一个串行单元，
/// 避免并发设置保存交错（调用路径：应用 setup 与设置保存，均经本函数）。
static CONFIGURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

static STATE: RwLock<ProxyState> = RwLock::new(ProxyState {
    mode: String::new(),
    url: String::new(),
    no_proxy: String::new(),
    saved_env: Vec::new(),
});

/// 规范化代理模式；未知值回落 `system`（跟随进程/系统环境变量）。
pub fn normalize_mode(mode: &str) -> &'static str {
    match mode {
        "direct" => "direct",
        "manual" => "manual",
        _ => "system",
    }
}

/// 当前生效的代理模式（规范化后）。
pub(crate) fn current_mode() -> &'static str {
    normalize_mode(&snapshot().mode)
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
        std::env::set_var(key, value);
    }
}

fn remove_var(key: &str) {
    std::env::remove_var(key);
}

/// 保存 configure 可能改写的环境键的原始值。
fn capture_env() -> Vec<(String, Option<String>)> {
    PROXY_VARIABLES
        .iter()
        .map(|key| (key.to_string(), std::env::var(key).ok()))
        .collect()
}

/// 恢复保存的环境原始值（含「原本未设置 → 移除」）。
fn restore_env(saved: &[(String, Option<String>)]) {
    for (key, value) in saved {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

/// 恢复保存的、新模式不再管理的键（手动模式不管 ALL_PROXY）。
fn restore_unmanaged(saved: &[(String, Option<String>)], managed: &[&str]) {
    for (key, value) in saved {
        if !managed.contains(&key.as_str()) {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

/// 合并多份例外地址列表与本地默认，去重保序。
fn merged_no_proxy(parts: &[&str]) -> String {
    let mut entries: Vec<&str> = Vec::new();
    for part in parts {
        for entry in part.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
    }
    for entry in LOCAL_NO_PROXY_DEFAULTS {
        if !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    entries.join(",")
}

fn combined_no_proxy(user: &str) -> String {
    merged_no_proxy(&[user])
}

/// 应用启动与设置保存时调用：更新全局状态并同步进程环境变量
/// （ureq/reqwest/更新器从这里读取；子进程另有 apply_to_command 注入）。
/// 写者已串行化；进程内读者（如保存设置瞬间的请求）可能读到单变量粒度的
/// 新旧混合状态，但不完整状态仅存在于写序列进行中的极短窗口。
pub fn configure(settings: &AppSettings) {
    // 写者串行化：避免并发 configure 交错捕获/恢复环境与发布状态。
    let _writer = CONFIGURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let state = ProxyState {
        mode: normalize_mode(&settings.proxy_mode).to_owned(),
        url: settings.proxy_url.trim().to_owned(),
        no_proxy: settings.proxy_no_proxy.trim().to_owned(),
        saved_env: Vec::new(),
    };
    // 读取旧状态（毒锁时按未初始化处理）。
    let old_saved = STATE
        .read()
        .ok()
        .map(|g| g.saved_env.clone())
        .unwrap_or_default();
    let mut saved = old_saved;
    if state.mode == "system" {
        // 切回「跟随系统」：恢复用户原始环境（同时清掉我们写入的键），
        // 避免模式切换永久丢失用户自设的代理环境变量。
        if !saved.is_empty() {
            restore_env(&saved);
            saved = Vec::new();
        }
    } else if saved.is_empty() {
        // 即将改写进程环境且尚未保存原始值：先保存（仅首次，去抖保存
        // 引起的重复 configure 不会把覆写后的值当原始值）。
        saved = capture_env();
    }
    if let Ok(mut guard) = STATE.write() {
        *guard = ProxyState {
            saved_env: saved.clone(),
            ..state.clone()
        };
    }
    match state.mode.as_str() {
        "manual" => {
            for key in PROXY_ADDRESS_KEYS {
                std::env::set_var(key, &state.url);
            }
            set_no_proxy(&combined_no_proxy(&state.no_proxy));
            // ALL_PROXY 不由手动模式管理：恢复用户原始值。
            restore_unmanaged(
                &saved,
                &[
                    "HTTP_PROXY",
                    "HTTPS_PROXY",
                    "http_proxy",
                    "https_proxy",
                    "NO_PROXY",
                    "no_proxy",
                ],
            );
        }
        "direct" => {
            for key in PROXY_VARIABLES {
                remove_var(key);
            }
            // in-process 的 reqwest（tauri 更新器）每次请求都重建客户端并读取
            // 环境变量；`*` 匹配所有主机，直连语义下让它无视系统代理。
            set_no_proxy("*");
        }
        // system：进程环境已恢复原样（含用户自设变量），交给 env/注册表检测。
        _ => {}
    }
}

/// 跟随系统模式下子进程的注入决策（纯函数，便于测试）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SystemInjection {
    /// 环境已有代理变量：尊重现有配置，补 `NODE_USE_ENV_PROXY=1`（否则
    /// Node 24 的 fetch 会无视代理环境变量），并合并例外地址（环境已有的
    /// NO_PROXY + 用户设置 + 本地默认），保证本地目标仍绕过代理。
    RespectEnv { no_proxy: String },
    /// 环境无代理变量但系统代理可用：注入完整代理环境（含合并后的 NO_PROXY）。
    UseSystemProxy { url: String, no_proxy: String },
    /// 环境与系统都没有代理：不注入。
    None,
}

/// 决策「跟随系统」模式给子进程注入的代理环境。
/// `lookup` 读取进程环境变量；`system_proxy` 为严格检测（ProxyEnable=1）结果。
pub(crate) fn decide_system_injection(
    lookup: &dyn Fn(&str) -> Option<String>,
    system_proxy: Option<String>,
    user_no_proxy: &str,
) -> SystemInjection {
    for key in PROXY_ENV_LOOKUP_KEYS {
        if lookup(key).is_some_and(|value| !value.trim().is_empty()) {
            let env_no_proxy = ["NO_PROXY", "no_proxy"]
                .iter()
                .find_map(|key| lookup(key).filter(|value| !value.trim().is_empty()))
                .unwrap_or_default();
            return SystemInjection::RespectEnv {
                no_proxy: merged_no_proxy(&[&env_no_proxy, user_no_proxy]),
            };
        }
    }
    match system_proxy {
        Some(url) => SystemInjection::UseSystemProxy {
            url,
            no_proxy: combined_no_proxy(user_no_proxy),
        },
        None => SystemInjection::None,
    }
}

fn system_injection_for(state: &ProxyState) -> SystemInjection {
    decide_system_injection(
        &|key| std::env::var(key).ok(),
        detect_system_proxy(),
        &state.no_proxy,
    )
}

/// 子进程环境操作：Command 与 portable-pty 的 CommandBuilder 共用。
enum EnvOp {
    Set(&'static str, String),
    Remove(&'static str),
}

/// 计算子进程代理环境操作（纯函数，便于测试）。
fn child_env_ops(state: &ProxyState, system: &SystemInjection) -> Vec<EnvOp> {
    match state.mode.as_str() {
        "manual" => {
            let mut ops: Vec<EnvOp> = PROXY_ADDRESS_KEYS
                .into_iter()
                .map(|key| EnvOp::Set(key, state.url.clone()))
                .collect();
            ops.push(EnvOp::Set("NODE_USE_ENV_PROXY", "1".into()));
            let no_proxy = combined_no_proxy(&state.no_proxy);
            ops.push(EnvOp::Set("NO_PROXY", no_proxy.clone()));
            ops.push(EnvOp::Set("no_proxy", no_proxy));
            ops
        }
        "direct" => {
            let mut ops: Vec<EnvOp> = PROXY_VARIABLES.into_iter().map(EnvOp::Remove).collect();
            ops.push(EnvOp::Remove("NODE_USE_ENV_PROXY"));
            ops
        }
        _ => match system {
            SystemInjection::RespectEnv { no_proxy } => {
                let mut ops: Vec<EnvOp> = vec![EnvOp::Set("NODE_USE_ENV_PROXY", "1".into())];
                ops.push(EnvOp::Set("NO_PROXY", no_proxy.clone()));
                ops.push(EnvOp::Set("no_proxy", no_proxy.clone()));
                ops
            }
            SystemInjection::UseSystemProxy { url, no_proxy } => {
                let mut ops: Vec<EnvOp> = PROXY_ADDRESS_KEYS
                    .into_iter()
                    .map(|key| EnvOp::Set(key, url.clone()))
                    .collect();
                ops.push(EnvOp::Set("NODE_USE_ENV_PROXY", "1".into()));
                ops.push(EnvOp::Set("NO_PROXY", no_proxy.clone()));
                ops.push(EnvOp::Set("no_proxy", no_proxy.clone()));
                ops
            }
            SystemInjection::None => Vec::new(),
        },
    }
}

impl EnvOp {
    fn apply_command(&self, command: &mut Command) {
        match self {
            EnvOp::Set(key, value) => {
                command.env(key, value);
            }
            EnvOp::Remove(key) => {
                command.env_remove(key);
            }
        }
    }

    fn apply_pty(&self, command: &mut portable_pty::CommandBuilder) {
        match self {
            EnvOp::Set(key, value) => {
                command.env(key, value);
            }
            EnvOp::Remove(key) => {
                command.env_remove(key);
            }
        }
    }
}

/// 给 Node 子进程注入/清理代理环境变量。
/// 手动模式额外设置 `NODE_USE_ENV_PROXY=1`，否则 pi/DSH 的 fetch 不会走代理。
pub fn apply_to_command(command: &mut Command) {
    let state = snapshot();
    let system = system_injection_for(&state);
    for op in child_env_ops(&state, &system) {
        op.apply_command(command);
    }
}

/// `portable-pty` 的命令构建器版本（走终端会话的 pi TUI）。
pub fn apply_to_pty(command: &mut portable_pty::CommandBuilder) {
    let state = snapshot();
    let system = system_injection_for(&state);
    for op in child_env_ops(&state, &system) {
        op.apply_pty(command);
    }
}

/// 把 `scheme://host[:port]` 形态的代理构造成带例外地址的 ureq 代理。
/// 用 Url 解析以正确处理 IPv6 字面量与默认端口；不支持凭据（手动模式校验
/// 阶段已拒绝，注册表解析产物也不含凭据）。
fn build_ureq_proxy(url: &str, no_proxy: &str) -> Option<ureq::Proxy> {
    let parsed = Url::parse(url.trim()).ok()?;
    let protocol = ureq::ProxyProtocol::try_from(parsed.scheme()).ok()?;
    let host = parsed.host_str()?;
    let mut builder = ureq::Proxy::builder(protocol).host(host);
    if let Some(port) = parsed.port_or_known_default() {
        builder = builder.port(port);
    }
    for entry in no_proxy
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        builder = builder.no_proxy(entry);
    }
    builder.build().ok()
}

/// 给 ureq 客户端用的代理：手动模式用显式地址，直连模式为 None，
/// 跟随系统则先读环境变量（HTTP(S)_PROXY / ALL_PROXY / NO_PROXY），
/// 未设置时回落 Windows 系统代理（严格语义）。
pub fn ureq_proxy() -> Option<ureq::Proxy> {
    let state = snapshot();
    match state.mode.as_str() {
        "manual" => build_ureq_proxy(&state.url, &combined_no_proxy(&state.no_proxy)),
        "direct" => None,
        _ => ureq::Proxy::try_from_env().or_else(|| {
            detect_system_proxy()
                .and_then(|url| build_ureq_proxy(&url, &combined_no_proxy(&state.no_proxy)))
        }),
    }
}

/// 当前应使用的代理地址（手动模式用显式地址；跟随系统时先读环境变量、
/// 未设置时回落 Windows 系统代理；直连为 None）。
pub fn current_url() -> Option<String> {
    let state = snapshot();
    match state.mode.as_str() {
        "manual" => Some(state.url.clone()).filter(|url| !url.is_empty()),
        "direct" => None,
        _ => ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"]
            .iter()
            .find_map(|key| std::env::var(key).ok())
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .or_else(detect_system_proxy),
    }
}

/// 解析 Windows `ProxyServer` 注册表值，返回形如 `http://host:port` 的代理。
/// 支持三种形态：`127.0.0.1:7890`（单值同时用于 http/https）、
/// `http=…;https=…`（按协议分组，优先 https）、`socks=…`（仅 SOCKS 无法映射
/// 为 HTTP 代理，返回 None）。其余或非法输入返回 None。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_windows_proxy_server(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.contains('=') {
        let mut https = None;
        let mut http = None;
        for part in raw.split(';') {
            let Some((scheme, value)) = part.trim().split_once('=') else {
                continue;
            };
            match scheme.trim().to_ascii_lowercase().as_str() {
                "https" => https = Some(value.trim()),
                "http" => http = Some(value.trim()),
                // ftp= / socks= 等其它协议条目与 HTTP(S) 代理环境变量无关。
                _ => {}
            }
        }
        return host_port_to_http_proxy(https.or(http)?);
    }
    host_port_to_http_proxy(raw)
}

/// 把 `host:port` 或 URL 形态的代理统一成 `http://host:port`。
#[cfg_attr(not(windows), allow(dead_code))]
fn host_port_to_http_proxy(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.contains("//") {
        let url = Url::parse(value).ok()?;
        if url.scheme() != "http" {
            return None;
        }
        let host = url.host_str()?;
        let port = url.port_or_known_default()?;
        return Some(format!("http://{host}:{port}"));
    }
    let (host, port_text) = value.rsplit_once(':')?;
    let host = host.trim();
    let port_text = port_text.trim();
    if host.is_empty()
        || port_text.is_empty()
        || !port_text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let port: u16 = port_text.parse().ok()?;
    if port == 0 {
        return None;
    }
    Some(format!("http://{host}:{port}"))
}

/// Windows 系统代理解析（注册表 Internet Settings），严格语义：
/// `ProxyEnable=1` 才视为开启，避免残留配置劫持全部流量。
#[cfg(windows)]
pub(crate) fn detect_system_proxy() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;

    let settings = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    // ProxyEnable 是 REG_DWORD；缺失或非 DWORD 视为未开启。
    let enabled: u32 = settings.get_value("ProxyEnable").unwrap_or(0);
    if enabled != 1 {
        return None;
    }
    let raw: String = settings.get_value("ProxyServer").ok()?;
    parse_windows_proxy_server(&raw)
}

/// 非 Windows 平台暂不解析系统代理。
#[cfg(not(windows))]
pub(crate) fn detect_system_proxy() -> Option<String> {
    None
}

/// Windows 系统代理解析（容忍语义，仅限 pi OAuth 桥接特例）：某些代理客户端
/// 会把 ProxyEnable 保持为 0，但仍写入 ProxyServer 并由浏览器实际使用；只要
/// 地址通过安全校验，就应交给桥接子进程，否则 OAuth 会绕过浏览器代理直连超时。
#[cfg(windows)]
pub(crate) fn detect_system_proxy_tolerant() -> Option<String> {
    use crate::provider::validate_proxy;
    use winreg::enums::HKEY_CURRENT_USER;

    let settings = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    let raw: String = settings.get_value("ProxyServer").ok()?;
    let proxy = parse_windows_proxy_server(&raw)?;
    validate_proxy(Some(&proxy)).ok()?;
    Some(proxy)
}

/// 非 Windows 平台暂不解析系统代理。
#[cfg(not(windows))]
pub(crate) fn detect_system_proxy_tolerant() -> Option<String> {
    None
}

/// 决定桥接子进程要注入的代理环境变量：已有任何代理环境变量时尊重现有配置
/// （返回 None），否则使用系统代理（含 `NODE_USE_ENV_PROXY=1` 与合并后的
/// NO_PROXY：环境已有列表 + 本地默认）。`lookup` 由调用方注入以便测试。
pub(crate) fn decide_proxy_injection(
    lookup: &dyn Fn(&str) -> Option<String>,
    system_proxy: Option<String>,
) -> Option<Vec<(&'static str, String)>> {
    for key in PROXY_ENV_LOOKUP_KEYS {
        if lookup(key).is_some_and(|value| !value.trim().is_empty()) {
            return None;
        }
    }
    let proxy = system_proxy?;
    let env_no_proxy = ["NO_PROXY", "no_proxy"]
        .iter()
        .find_map(|key| lookup(key).filter(|value| !value.trim().is_empty()))
        .unwrap_or_default();
    let no_proxy = merged_no_proxy(&[&env_no_proxy]);
    Some(vec![
        ("HTTP_PROXY", proxy.clone()),
        ("HTTPS_PROXY", proxy.clone()),
        ("http_proxy", proxy.clone()),
        ("https_proxy", proxy.clone()),
        ("NODE_USE_ENV_PROXY", "1".to_string()),
        ("NO_PROXY", no_proxy.clone()),
        ("no_proxy", no_proxy),
    ])
}

fn probe(proxy: Option<ureq::Proxy>) -> Result<String, String> {
    let mut config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(8)))
        .timeout_global(Some(Duration::from_secs(12)))
        // 非 2xx 不作为 Err 抱出：需要读响应体区分「GitHub 匿名限流」与真实故障。
        .http_status_as_error(false)
        .https_only(true);
    config = config.proxy(proxy);
    let agent = config.build().new_agent();
    let started = Instant::now();
    let mut response = agent
        .get(TEST_URL)
        .header("User-Agent", "DeepPi")
        .call()
        .map_err(|error| format!("{error}"))?;
    let status = response.status().as_u16();
    let elapsed = started.elapsed().as_millis();
    if (200..300).contains(&status) {
        return Ok(format!("HTTP {status} · {elapsed} ms"));
    }
    // GitHub 匿名额度按出口 IP 计（60 次/小时）：共享代理节点常被其他人耗尽，
    // 403 + rate limit 响应体只说明测试端点受限，代理隧道本身是通的。
    let body = response.body_mut().read_to_string().unwrap_or_default();
    if status == 403 && body.contains("rate limit exceeded") {
        return Ok(format!(
            "连通 · {elapsed} ms · 测试端点触发 GitHub 匿名限流（共享出口节点常见），不影响 Pi/Advisor"
        ));
    }
    Err(format!("目标返回 HTTP {status} · {elapsed} ms"))
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
        // 与 ureq_proxy() 的跟随系统逻辑一致：env 优先，未设置时回落系统代理，
        // 让「测试连接」反映实际将被使用的代理。
        _ => ureq::Proxy::try_from_env()
            .or_else(|| detect_system_proxy().and_then(|url| build_ureq_proxy(&url, ""))),
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
    fn merges_user_no_proxy_with_local_defaults() {
        assert_eq!(
            combined_no_proxy(""),
            "localhost,127.0.0.1,::1,[::1]".to_string()
        );
        assert_eq!(
            combined_no_proxy(" internal.local, localhost "),
            "internal.local,localhost,127.0.0.1,::1,[::1]".to_string()
        );
    }

    #[test]
    fn manual_mode_sets_node_proxy_flag_and_merged_no_proxy() {
        let state = ProxyState {
            mode: "manual".into(),
            url: "http://127.0.0.1:7890".into(),
            no_proxy: "localhost".into(),
            saved_env: Vec::new(),
        };
        let ops = child_env_ops(&state, &SystemInjection::None);
        let sets = collect_sets(&ops);
        assert_eq!(
            sets.get("HTTPS_PROXY").map(String::as_str),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(
            sets.get("NODE_USE_ENV_PROXY").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            sets.get("NO_PROXY").map(String::as_str),
            Some("localhost,127.0.0.1,::1,[::1]")
        );
    }

    #[test]
    fn direct_mode_clears_proxy_env_including_node_flag() {
        let state = ProxyState {
            mode: "direct".into(),
            url: String::new(),
            no_proxy: String::new(),
            saved_env: Vec::new(),
        };
        let ops = child_env_ops(&state, &SystemInjection::None);
        let removes = collect_removes(&ops);
        for key in ["HTTP_PROXY", "NO_PROXY", "no_proxy", "NODE_USE_ENV_PROXY"] {
            assert!(removes.contains(&key), "{key} should be removed");
        }
    }

    #[test]
    fn system_mode_none_injects_nothing() {
        let state = ProxyState {
            mode: "system".into(),
            url: String::new(),
            no_proxy: String::new(),
            saved_env: Vec::new(),
        };
        assert!(child_env_ops(&state, &SystemInjection::None).is_empty());
    }

    #[test]
    fn system_mode_respect_env_sets_flag_and_merged_no_proxy() {
        let state = ProxyState {
            mode: "system".into(),
            url: String::new(),
            no_proxy: String::new(),
            saved_env: Vec::new(),
        };
        let ops = child_env_ops(
            &state,
            &SystemInjection::RespectEnv {
                no_proxy: "localhost,127.0.0.1,::1,[::1]".into(),
            },
        );
        let sets = collect_sets(&ops);
        let removes = collect_removes(&ops);
        assert_eq!(
            sets.get("NODE_USE_ENV_PROXY").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            sets.get("NO_PROXY").map(String::as_str),
            Some("localhost,127.0.0.1,::1,[::1]")
        );
        assert!(removes.is_empty());
    }

    #[test]
    fn system_mode_use_system_proxy_injects_full_env() {
        let state = ProxyState {
            mode: "system".into(),
            url: String::new(),
            no_proxy: String::new(),
            saved_env: Vec::new(),
        };
        let system = SystemInjection::UseSystemProxy {
            url: "http://127.0.0.1:7890".into(),
            no_proxy: "localhost,127.0.0.1,::1,[::1]".into(),
        };
        let ops = child_env_ops(&state, &system);
        let sets = collect_sets(&ops);
        for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
            assert_eq!(
                sets.get(key).map(String::as_str),
                Some("http://127.0.0.1:7890")
            );
        }
        assert_eq!(
            sets.get("NODE_USE_ENV_PROXY").map(String::as_str),
            Some("1")
        );
        assert!(sets.contains_key("NO_PROXY"));
        assert!(sets.contains_key("no_proxy"));
    }

    #[test]
    fn system_injection_respects_existing_env() {
        // 用户已显式配置代理环境变量：只补 NODE_USE_ENV_PROXY，不覆盖。
        let existing = |key: &str| (key == "HTTPS_PROXY").then(|| "http://env-proxy:1".to_string());
        assert_eq!(
            decide_system_injection(
                &existing,
                Some("http://127.0.0.1:7890".into()),
                "mine.local"
            ),
            SystemInjection::RespectEnv {
                no_proxy: "mine.local,localhost,127.0.0.1,::1,[::1]".into(),
            }
        );
        // 环境已有 NO_PROXY：保留并合并。
        let with_no_proxy = |key: &str| match key {
            "HTTPS_PROXY" => Some("http://env-proxy:1".to_string()),
            "NO_PROXY" => Some("corp.internal".to_string()),
            _ => None,
        };
        assert_eq!(
            decide_system_injection(&with_no_proxy, Some("http://127.0.0.1:7890".into()), ""),
            SystemInjection::RespectEnv {
                no_proxy: "corp.internal,localhost,127.0.0.1,::1,[::1]".into(),
            }
        );
        // 空字符串的环境变量视为未设置。
        let empty = |key: &str| (key == "HTTP_PROXY").then(|| "  ".to_string());
        assert_eq!(
            decide_system_injection(&empty, Some("http://127.0.0.1:7890".into()), ""),
            SystemInjection::UseSystemProxy {
                url: "http://127.0.0.1:7890".into(),
                no_proxy: "localhost,127.0.0.1,::1,[::1]".into(),
            }
        );
        // 没有环境变量也没有系统代理：不注入。
        let none = |_: &str| None::<String>;
        assert_eq!(
            decide_system_injection(&none, None, ""),
            SystemInjection::None
        );
        // 用户例外地址合并进注入结果。
        assert_eq!(
            decide_system_injection(
                &none,
                Some("http://127.0.0.1:7890".into()),
                "internal.local"
            ),
            SystemInjection::UseSystemProxy {
                url: "http://127.0.0.1:7890".into(),
                no_proxy: "internal.local,localhost,127.0.0.1,::1,[::1]".into(),
            }
        );
    }

    #[test]
    fn parses_windows_proxy_server_forms() {
        // 单值形态：host:port 同时用于 http/https。
        assert_eq!(
            parse_windows_proxy_server("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(
            parse_windows_proxy_server(" localhost:8080 ").as_deref(),
            Some("http://localhost:8080")
        );
        // 按协议分组：优先 https 条目，大小写不敏感。
        assert_eq!(
            parse_windows_proxy_server("http=10.0.0.2:80;https=10.0.0.2:443").as_deref(),
            Some("http://10.0.0.2:443")
        );
        assert_eq!(
            parse_windows_proxy_server("HTTP=10.0.0.2:8080").as_deref(),
            Some("http://10.0.0.2:8080")
        );
        // 仅 SOCKS 无法映射为 HTTP 代理环境变量；空值与非法端口返回 None。
        assert_eq!(parse_windows_proxy_server("socks=127.0.0.1:1080"), None);
        assert_eq!(parse_windows_proxy_server(""), None);
        assert_eq!(parse_windows_proxy_server("host:not-a-port"), None);
    }

    #[test]
    fn builds_ureq_proxy_with_no_proxy() {
        let proxy = build_ureq_proxy("http://127.0.0.1:7890", "localhost,127.0.0.1").unwrap();
        assert!(proxy.is_no_proxy(&"http://127.0.0.1:11434/".parse().unwrap()));
        assert!(!proxy.is_no_proxy(&"https://api.github.com/".parse().unwrap()));

        // IPv6 字面量与默认端口（例外地址用合并后的本地默认）。
        let ipv6 = build_ureq_proxy("http://[::1]:7890", &combined_no_proxy("")).unwrap();
        assert!(ipv6.is_no_proxy(&"http://[::1]:7890/".parse().unwrap()));
        assert!(!ipv6.is_no_proxy(&"https://api.github.com/".parse().unwrap()));

        let default_port = build_ureq_proxy("http://proxy.local", "").unwrap();
        // 构造成功即可；默认端口由 Url::port_or_known_default 补齐。
        drop(default_port);

        // 非法输入返回 None。
        assert!(build_ureq_proxy("not-a-url", "").is_none());
        assert!(build_ureq_proxy("ftp://proxy.local:21", "").is_none());
    }

    #[test]
    fn bridge_injection_respects_existing_env() {
        // 用户已显式配置代理环境变量：不注入、不覆盖。
        let existing = |key: &str| (key == "HTTPS_PROXY").then(|| "http://env-proxy:1".to_string());
        assert!(decide_proxy_injection(&existing, Some("http://127.0.0.1:7890".into())).is_none());
        let none = |_: &str| None::<String>;
        let pairs = decide_proxy_injection(&none, Some("http://127.0.0.1:7890".into()))
            .expect("system proxy should be injected");
        assert!(pairs
            .iter()
            .any(|(key, value)| *key == "HTTPS_PROXY" && value == "http://127.0.0.1:7890"));
        // 环境代理变量注入时附带 Node 开关，否则 pi 的 fetch 仍会直连。
        assert!(pairs
            .iter()
            .any(|(key, value)| *key == "NODE_USE_ENV_PROXY" && value == "1"));
        // 环境已有 NO_PROXY：保留并合并。
        let with_no_proxy = |key: &str| (key == "NO_PROXY").then(|| "corp.internal".to_string());
        let pairs = decide_proxy_injection(&with_no_proxy, Some("http://127.0.0.1:7890".into()))
            .expect("injected");
        let no_proxy = pairs
            .iter()
            .find(|(key, _)| *key == "NO_PROXY")
            .map(|(_, value)| value.clone())
            .unwrap();
        assert!(no_proxy.starts_with("corp.internal,"));
        // 没有可用系统代理时不注入任何东西。
        assert!(decide_proxy_injection(&none, None).is_none());
    }

    /// 本地端到端验证：开启代理时请求经 CONNECT 隧道由代理应答，
    /// 命中例外地址的同主机请求绕过代理直连目标。
    #[test]
    fn local_proxy_routes_requests_and_bypasses_no_proxy_hosts() {
        use std::io::{BufRead, BufReader, Write};

        let respond = |mut stream: std::net::TcpStream, body: &str| {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        };

        let proxy_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_port = proxy_listener.local_addr().unwrap().port();
        let target_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let target_port = target_listener.local_addr().unwrap().port();

        // 目标服务器：直连请求由它应答。
        std::thread::spawn(move || {
            let Ok((stream, _)) = target_listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                    break;
                }
            }
            respond(stream, "direct");
        });
        // 假代理：应答 CONNECT 建立隧道，再对隧道内的内层请求应答 "proxied"
        //（绕行请求不应到达代理）。
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = proxy_listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut connect_line = String::new();
            reader.read_line(&mut connect_line).unwrap_or(0);
            assert!(
                connect_line.starts_with("CONNECT "),
                "proxy should receive CONNECT: {connect_line:?}"
            );
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                    break;
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .unwrap();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                    break;
                }
            }
            respond(stream, "proxied");
        });

        let url_for = |port: u16| format!("http://127.0.0.1:{port}/probe");

        // 无例外地址：请求应经代理（绝对 URI 到达假代理）。
        let proxy = build_ureq_proxy(&format!("http://127.0.0.1:{proxy_port}"), "").unwrap();
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .proxy(Some(proxy))
            .build()
            .new_agent();
        let body = agent
            .get(&url_for(target_port))
            .call()
            .expect("proxied request should succeed")
            .into_body()
            .read_to_string()
            .unwrap();
        assert_eq!(body, "proxied");

        // 例外地址覆盖目标主机：请求应绕过代理，由目标直接应答。
        let proxy = build_ureq_proxy(
            &format!("http://127.0.0.1:{proxy_port}"),
            "localhost,127.0.0.1",
        )
        .unwrap();
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .proxy(Some(proxy))
            .build()
            .new_agent();
        let body = agent
            .get(&url_for(target_port))
            .call()
            .expect("bypassed request should succeed")
            .into_body()
            .read_to_string()
            .unwrap();
        assert_eq!(body, "direct");
    }

    fn collect_sets(ops: &[EnvOp]) -> std::collections::HashMap<&'static str, String> {
        ops.iter()
            .filter_map(|op| match op {
                EnvOp::Set(key, value) => Some((*key, value.clone())),
                EnvOp::Remove(_) => None,
            })
            .collect()
    }

    fn collect_removes(ops: &[EnvOp]) -> Vec<&'static str> {
        ops.iter()
            .filter_map(|op| match op {
                EnvOp::Set(_, _) => None,
                EnvOp::Remove(key) => Some(*key),
            })
            .collect()
    }
}
