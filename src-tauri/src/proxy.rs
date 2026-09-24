//! 全局网络代理：作用于 DeepPi 自身的 HTTP 客户端与所有 Node 子进程
//! （Pi RPC/TUI 会话、DSH、npm 包管理、pi 登录）。
//!
//! 子进程里的模型请求由 pi 的 `fetch` 发出，Node 24 只有在
//! `NODE_USE_ENV_PROXY=1` 且设置了 `HTTP(S)_PROXY` 时才会走代理，
//! 因此手动模式必须同时注入该变量；ureq/reqwest（更新器、市场、运行时下载）
//! 则从进程环境变量读取。
//!
//! 两种模式的目标行为：
//! - 跟随系统（默认）：以 Windows 当前已启用的系统代理设置为准，不受继承的旧
//!   HTTP_PROXY 或 `ProxyEnable=0` 的残留 ProxyServer 劫持；系统未启用显式
//!   代理时走 OS 网络栈，透明 TUN 仍可接管流量。
//! - 手动：显式地址 + `NODE_USE_ENV_PROXY=1` + NO_PROXY。
//! 旧版 direct 设置在读取时迁移到跟随系统，不再作为单独的选项。
//!
//! 例外地址始终合并本地默认（localhost/127.0.0.1/::1/[::1]），否则开启代理后
//! 本地目标（DSH 本地 API、Ollama 等本地 Provider）会被错误送进代理。
//!
//! 中继（`crate::proxy_relay`）：子进程的代理环境在 spawn 那一刻冻结，DSH 侧的
//! 代理策略在进程启动时也只解析一次，所以「切换模式」本身无法触达已运行的子
//! 进程。应用运行中会把两种模式的子进程统一指向一个本机回环中继，地址恒定；
//! 切换模式只改中继的上游，已运行的 DSH/Pi 会话立即改走新路径，无需重启。

use std::{
    process::Command,
    sync::RwLock,
    time::{Duration, Instant},
};

use tauri::Url;

use crate::proxy_relay::Upstream;
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

/// 仅保留「跟随系统」与「手动」；旧版 direct 设置迁移到 system。
pub fn normalize_mode(mode: &str) -> &'static str {
    match mode {
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
        // 离开手动代理时先恢复原环境，再按**当前系统设置**决定出网；
        // 恢复的旧 HTTP_PROXY 不能被误当作系统仍启用的代理。
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
            // ALL_PROXY 若继承旧代理会抢走手动请求；清除两套大小写环境变量。
            for key in ["ALL_PROXY", "all_proxy"] {
                remove_var(key);
            }
        }
        // 跟随系统不继承启动器的旧代理环境：明确映射已启用的系统配置，
        // 若没有显式系统代理就让进程直接走 OS 网络栈（含透明 TUN）。
        _ => match detect_system_proxy() {
            Some(url) => {
                for key in PROXY_ADDRESS_KEYS {
                    std::env::set_var(key, &url);
                }
                for key in ["ALL_PROXY", "all_proxy"] {
                    remove_var(key);
                }
                set_no_proxy(&combined_no_proxy(&state.no_proxy));
            }
            None => {
                for key in PROXY_VARIABLES {
                    remove_var(key);
                }
                // reqwest 可自行读取 WinINET/环境代理；`*` 确保系统代理关闭
                // 时不被进程残值重定向，底层 TUN 仍会照常接管真实出网。
                set_no_proxy("*");
                // PAC/WPAD 生效时注册表里没有静态 ProxyServer，上面的分支会走到
                // 这里。子进程仍然正确（它们指向按目标求值的中继），但宿主进程内
                // 的 ureq/reqwest 无法求值 PAC，此时按 OS 网络栈直连。
                if crate::system_proxy::has_dynamic_proxy_config() {
                    log::info!(
                        "event=proxy_mode status=system dynamic_proxy_config kind=pac_or_autodetect \
                         note=in_process_clients_use_os_route"
                    );
                }
            }
        },
    }
    // 状态与环境都已就位，再把中继上游切到新模式的等效值。已经建立的中继监听
    // socket 不变，新连接立即走新上游——这就是「切模式不重启子进程」的实现点。
    let current = snapshot();
    let system = system_injection_for(&current);
    crate::proxy_relay::set_global_upstream(effective_upstream(&current, &system));
}

/// 跟随系统模式下子进程的注入决策（纯函数，便于测试）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SystemInjection {
    /// 环境无代理变量但系统代理可用：注入完整代理环境（含合并后的 NO_PROXY）。
    UseSystemProxy { url: String, no_proxy: String },
    /// 环境与系统都没有代理：不注入。
    None,
}

/// 决策「跟随系统」模式的代理：以 Windows 已启用的系统设置为准，绝不让
/// 继承的 HTTP_PROXY 或 ProxyEnable=0 的注册表残值覆盖系统当前网络路径。
/// `lookup` 保留为注入式测试入口；进程环境在此模式下不是系统设置的来源。
pub(crate) fn decide_system_injection(
    _lookup: &dyn Fn(&str) -> Option<String>,
    system_proxy: Option<String>,
    user_no_proxy: &str,
) -> SystemInjection {
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

/// 跟随系统/手动模式的中继上游。系统设置已关闭时经 OS 网络栈出网（TUN
/// 仍可透明接管），绝不因为环境里还留着 HTTP_PROXY 就使用旧代理端口。
fn upstream_for(state: &ProxyState, system: &SystemInjection) -> Upstream {
    if state.mode == "manual" {
        return match state.url.trim() {
            "" => Upstream::Direct,
            url => Upstream::ViaProxy(url.to_owned()),
        };
    }
    match system {
        SystemInjection::UseSystemProxy { url, .. } => Upstream::ViaProxy(url.clone()),
        SystemInjection::None => Upstream::Direct,
    }
}

fn effective_upstream(state: &ProxyState, system: &SystemInjection) -> Upstream {
    upstream_for(state, system)
}

/// 每条新中继连接都重新读取当前系统设置，并**按目标**解析路由：用户在 Windows
/// 中开/关代理、改 PAC、或不同协议走不同代理时，都不必重启 DeepPi 或 DSH。
/// 初始化前返回 None，让中继保留自己的默认上游。
pub(crate) fn live_upstream_for(target: &str) -> Option<Upstream> {
    let state = snapshot();
    if state.mode.is_empty() {
        return None;
    }
    if state.mode == "manual" {
        return Some(upstream_for(&state, &SystemInjection::None));
    }
    // 系统模式以 Windows 当前启用的设置为准：含按协议分流、PAC/WPAD 与例外
    // 列表。`Unsupported` 原样交给中继（它返回 502），不能退化成直连——那会在
    // 用户以为在走代理时把流量泄漏出去。
    Some(match crate::system_proxy::resolve(target) {
        crate::system_proxy::Route::Direct => Upstream::Direct,
        crate::system_proxy::Route::HttpProxy(url) => Upstream::ViaProxy(url),
        crate::system_proxy::Route::Unsupported(reason) => Upstream::Unsupported(reason),
    })
}

/// 子进程应指向的中继地址。中继未启动、或当前上游不被中继支持（例如
/// `https://` 上游代理）时返回 `None`，调用方回落到既有注入行为。
///
/// 上游不被支持时「切换模式无需重启子进程」这个性质**不成立**（子进程拿到的
/// 仍是真实地址），所以中继在运行的情况下必须留痕，否则这个折扣是隐形的。
fn child_relay_url(state: &ProxyState, system: &SystemInjection) -> Option<String> {
    if !effective_upstream(state, system).is_relay_supported() {
        if crate::proxy_relay::global_port().is_some() {
            log::warn!("event=proxy_relay status=unsupported_upstream kind=https_or_invalid");
        }
        return None;
    }
    crate::proxy_relay::global_url()
}

/// 子进程环境操作：Command 与 portable-pty 的 CommandBuilder 共用。
enum EnvOp {
    Set(&'static str, String),
    Remove(&'static str),
}

/// 中继运行时子进程的代理环境：两种模式一律指向同一个回环中继，地址恒定。
fn relay_env_ops(relay_url: &str, no_proxy: &str) -> Vec<EnvOp> {
    let mut ops: Vec<EnvOp> = PROXY_ADDRESS_KEYS
        .into_iter()
        .map(|key| EnvOp::Set(key, relay_url.to_owned()))
        .collect();
    // ALL_PROXY 不由模式管理，但部分客户端会优先使用它；中继存在时留着它会让
    // 子进程绕过中继，模式切换就重新变得不可控，因此显式清除。
    ops.push(EnvOp::Remove("ALL_PROXY"));
    ops.push(EnvOp::Remove("all_proxy"));
    ops.push(EnvOp::Set("NODE_USE_ENV_PROXY", "1".into()));
    ops.push(EnvOp::Set("NO_PROXY", no_proxy.to_owned()));
    ops.push(EnvOp::Set("no_proxy", no_proxy.to_owned()));
    ops
}

/// 跟随系统不继承启动器的 NO_PROXY（可能是过期的 *），只合并设置及本地例外。
fn relay_no_proxy(state: &ProxyState, system: &SystemInjection) -> String {
    match system {
        SystemInjection::UseSystemProxy { no_proxy, .. } if state.mode != "manual" => {
            no_proxy.clone()
        }
        _ => combined_no_proxy(&state.no_proxy),
    }
}

/// 计算子进程代理环境操作（纯函数，便于测试）。
/// `relay_url` 为 `Some` 时两种模式统一指向中继；为 `None`（中继未启动，或
/// 当前上游不被中继支持）时显式按系统/手动模式注入，决不继承旧代理。
fn child_env_ops(
    state: &ProxyState,
    system: &SystemInjection,
    relay_url: Option<&str>,
) -> Vec<EnvOp> {
    if let Some(relay_url) = relay_url {
        return relay_env_ops(relay_url, &relay_no_proxy(state, system));
    }
    match state.mode.as_str() {
        "manual" => {
            let mut ops: Vec<EnvOp> = PROXY_ADDRESS_KEYS
                .into_iter()
                .map(|key| EnvOp::Set(key, state.url.clone()))
                .collect();
            ops.push(EnvOp::Remove("ALL_PROXY"));
            ops.push(EnvOp::Remove("all_proxy"));
            ops.push(EnvOp::Set("NODE_USE_ENV_PROXY", "1".into()));
            let no_proxy = combined_no_proxy(&state.no_proxy);
            ops.push(EnvOp::Set("NO_PROXY", no_proxy.clone()));
            ops.push(EnvOp::Set("no_proxy", no_proxy));
            ops
        }
        _ => match system {
            SystemInjection::UseSystemProxy { url, no_proxy } => {
                let mut ops: Vec<EnvOp> = PROXY_ADDRESS_KEYS
                    .into_iter()
                    .map(|key| EnvOp::Set(key, url.clone()))
                    .collect();
                ops.push(EnvOp::Remove("ALL_PROXY"));
                ops.push(EnvOp::Remove("all_proxy"));
                ops.push(EnvOp::Set("NODE_USE_ENV_PROXY", "1".into()));
                ops.push(EnvOp::Set("NO_PROXY", no_proxy.clone()));
                ops.push(EnvOp::Set("no_proxy", no_proxy.clone()));
                ops
            }
            SystemInjection::None => {
                let mut ops: Vec<EnvOp> = PROXY_VARIABLES.into_iter().map(EnvOp::Remove).collect();
                ops.push(EnvOp::Remove("NODE_USE_ENV_PROXY"));
                // 无中继退化路径也不得继承启动器的旧代理端口。
                ops.push(EnvOp::Set("NO_PROXY", "*".into()));
                ops.push(EnvOp::Set("no_proxy", "*".into()));
                ops
            }
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
/// 中继运行时三种模式统一指向回环中继（地址恒定，切模式无需重启子进程）；
/// 否则手动模式注入显式地址并补 `NODE_USE_ENV_PROXY=1`，否则 pi/DSH 的 fetch
/// 不会走代理。
pub fn apply_to_command(command: &mut Command) {
    let state = snapshot();
    let system = system_injection_for(&state);
    let relay = child_relay_url(&state, &system);
    for op in child_env_ops(&state, &system, relay.as_deref()) {
        op.apply_command(command);
    }
}

/// `portable-pty` 的命令构建器版本（走终端会话的 pi TUI）。
pub fn apply_to_pty(command: &mut portable_pty::CommandBuilder) {
    let state = snapshot();
    let system = system_injection_for(&state);
    let relay = child_relay_url(&state, &system);
    for op in child_env_ops(&state, &system, relay.as_deref()) {
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

/// ureq 代理与中继读取同一来源：手动地址，或已启用的 Windows 系统代理。
/// 不读取进程继承的代理环境变量（其中可能是已关闭的旧端口）。
pub fn ureq_proxy() -> Option<ureq::Proxy> {
    let state = snapshot();
    let url = if state.mode == "manual" {
        Some(state.url)
    } else {
        detect_system_proxy()
    };
    url.and_then(|url| build_ureq_proxy(&url, &combined_no_proxy(&state.no_proxy)))
}

/// 当前全局代理地址；不代表 PAC/按域名分流的最终出站路由。
pub fn current_url() -> Option<String> {
    let state = snapshot();
    if state.mode == "manual" {
        Some(state.url).filter(|url| !url.is_empty())
    } else {
        detect_system_proxy()
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
        "manual" => {
            let url = validate_url(&url)?;
            Some(
                ureq::Proxy::new(&url)
                    .map_err(|error| format!("proxy address is invalid: {error}"))?,
            )
        }
        // 测试和真实请求共用系统设置；不能报告旧环境代理「连通」却让 DSH 失败。
        _ => detect_system_proxy().and_then(|url| build_ureq_proxy(&url, "")),
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
        // 旧安装保存的 direct 必须迁移到「跟随系统」，不能留下隐藏的第三种模式。
        assert_eq!(normalize_mode("direct"), "system");
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
        let ops = child_env_ops(&state, &SystemInjection::None, None);
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
    fn system_without_proxy_clears_inherited_env_when_relay_is_absent() {
        let state = ProxyState {
            mode: "system".into(),
            url: String::new(),
            no_proxy: String::new(),
            saved_env: Vec::new(),
        };
        let ops = child_env_ops(&state, &SystemInjection::None, None);
        let removes = collect_removes(&ops);
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NODE_USE_ENV_PROXY",
        ] {
            assert!(removes.contains(&key), "{key} should be removed");
        }
        let sets = collect_sets(&ops);
        assert_eq!(sets.get("NO_PROXY").map(String::as_str), Some("*"));
        assert_eq!(sets.get("no_proxy").map(String::as_str), Some("*"));
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
        let ops = child_env_ops(&state, &system, None);
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

    /// 构造测试用状态（环境隔离：这些用例都不读写进程环境变量）。
    fn state_for(mode: &str, url: &str, no_proxy: &str) -> ProxyState {
        ProxyState {
            mode: mode.into(),
            url: url.into(),
            no_proxy: no_proxy.into(),
            saved_env: Vec::new(),
        }
    }

    /// 中继运行时：两种模式 × 系统启用/关闭代理，子进程环境必须**完全一致**
    /// ——这正是「地址恒定」的不变量：切模式不再改动子进程的环境。
    #[test]
    fn relay_url_is_identical_across_modes_and_system_decisions() {
        let relay = "http://127.0.0.1:5555";
        let system_decision = |no_proxy: &str| SystemInjection::UseSystemProxy {
            url: "http://127.0.0.1:7890".into(),
            no_proxy: no_proxy.into(),
        };
        let cases = [
            (
                state_for("manual", "http://127.0.0.1:7890", "mine.local"),
                SystemInjection::None,
            ),
            (
                state_for("manual", "http://127.0.0.1:7890", "mine.local"),
                system_decision("localhost"),
            ),
            (state_for("system", "", ""), SystemInjection::None),
            (state_for("system", "", "mine.local"), SystemInjection::None),
            (
                state_for("system", "", "mine.local"),
                system_decision("mine.local,localhost,127.0.0.1,::1,[::1]"),
            ),
        ];
        for (state, system) in cases {
            let ops = child_env_ops(&state, &system, Some(relay));
            let sets = collect_sets(&ops);
            for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
                assert_eq!(
                    sets.get(key).map(String::as_str),
                    Some(relay),
                    "mode={} key={key} must point at the relay",
                    state.mode
                );
            }
            assert_eq!(
                sets.get("NODE_USE_ENV_PROXY").map(String::as_str),
                Some("1")
            );
            // ALL_PROXY 必须清掉，否则部分客户端会绕过中继。
            let removes = collect_removes(&ops);
            assert!(removes.contains(&"ALL_PROXY"), "mode={}", state.mode);
            assert!(removes.contains(&"all_proxy"), "mode={}", state.mode);
            // 例外地址必须含本地默认，且大小写两份一致。
            let no_proxy = sets.get("NO_PROXY").expect("NO_PROXY must be set");
            assert!(no_proxy.contains("localhost"), "{no_proxy}");
            assert!(no_proxy.contains("127.0.0.1"), "{no_proxy}");
            assert_eq!(
                sets.get("no_proxy").map(String::as_str),
                Some(no_proxy.as_str())
            );
        }
    }

    /// 系统模式不继承启动器旧的 NO_PROXY=*，否则已开启的系统代理被整体绕过。
    #[test]
    fn relay_no_proxy_uses_settings_not_inherited_environment() {
        let relay = "http://127.0.0.1:5555";
        let state = state_for("system", "", "mine.local");
        let sets = collect_sets(&child_env_ops(
            &state,
            &SystemInjection::UseSystemProxy {
                url: "http://127.0.0.1:7890".into(),
                no_proxy: "mine.local,localhost,127.0.0.1,::1,[::1]".into(),
            },
            Some(relay),
        ));
        assert_eq!(
            sets.get("NO_PROXY").map(String::as_str),
            Some("mine.local,localhost,127.0.0.1,::1,[::1]")
        );
        let sets = collect_sets(&child_env_ops(&state, &SystemInjection::None, Some(relay)));
        assert_eq!(
            sets.get("NO_PROXY").map(String::as_str),
            Some("mine.local,localhost,127.0.0.1,::1,[::1]")
        );
    }

    /// 中继未启动时仍必须清除继承的过期代理，不暗中退回旧行为。
    #[test]
    fn without_relay_each_mode_explicitly_controls_inherited_env() {
        let manual = state_for("manual", "http://127.0.0.1:7890", "");
        let ops = child_env_ops(&manual, &SystemInjection::None, None);
        let sets = collect_sets(&ops);
        assert_eq!(
            sets.get("HTTPS_PROXY").map(String::as_str),
            Some("http://127.0.0.1:7890")
        );
        assert!(
            !sets.values().any(|value| value.contains("127.0.0.1:5555")),
            "no relay address may leak when the relay is absent"
        );

        let system = state_for("system", "", "");
        let ops = child_env_ops(&system, &SystemInjection::None, None);
        let removes = collect_removes(&ops);
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NODE_USE_ENV_PROXY",
        ] {
            assert!(removes.contains(&key), "system must remove stale {key}");
        }
        assert_eq!(
            collect_sets(&ops).get("NO_PROXY").map(String::as_str),
            Some("*")
        );
    }

    #[test]
    fn upstream_for_follows_system_setting_not_environment() {
        assert_eq!(
            upstream_for(
                &state_for("manual", "http://127.0.0.1:7890", ""),
                &SystemInjection::None
            ),
            Upstream::ViaProxy("http://127.0.0.1:7890".into())
        );
        let system = state_for("system", "", "");
        assert_eq!(
            upstream_for(&system, &SystemInjection::None),
            Upstream::Direct
        );
        assert_eq!(
            upstream_for(
                &system,
                &SystemInjection::UseSystemProxy {
                    url: "http://127.0.0.1:7890".into(),
                    no_proxy: String::new(),
                }
            ),
            Upstream::ViaProxy("http://127.0.0.1:7890".into())
        );
        // https 上游不被中继承载：调用方必须显式提示降级行为。
        assert!(!upstream_for(
            &state_for("manual", "https://proxy.local:8443", ""),
            &SystemInjection::None,
        )
        .is_relay_supported());
    }

    #[test]
    fn system_injection_ignores_stale_environment_and_uses_os_settings() {
        let inherited = |key: &str| match key {
            "HTTPS_PROXY" => Some("http://dead-proxy:7890".to_string()),
            "NO_PROXY" => Some("*".to_string()),
            _ => None,
        };
        // 核心回归：Windows 已关闭代理，即使进程继承旧端口也不使用它。
        assert_eq!(
            decide_system_injection(&inherited, None, ""),
            SystemInjection::None
        );
        // Windows 启用代理，以系统值为准，不把继承的 NO_PROXY=* 带给新上游。
        assert_eq!(
            decide_system_injection(
                &inherited,
                Some("http://127.0.0.1:7890".into()),
                "mine.local"
            ),
            SystemInjection::UseSystemProxy {
                url: "http://127.0.0.1:7890".into(),
                no_proxy: "mine.local,localhost,127.0.0.1,::1,[::1]".into(),
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
