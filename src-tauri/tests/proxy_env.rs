//! 代理设置的进程环境集成测试：独立测试进程运行，与库内单测进程隔离，
//! 避免环境变量改写影响并行用例（git 子进程等）。
//!
//! 覆盖两类真实边界：
//! 1. `configure()` 的模式切换环境保存/恢复（manual/direct → system）；
//! 2. tauri 更新器所依赖的 reqwest→hyper-util system 匹配器对
//!    `NO_PROXY=*` 与环境代理的处理（直连/跟随系统模式的更新器路由）。

use deeppi_lib::proxy::configure;
use deeppi_lib::settings::AppSettings;

/// 本进程内的环境测试串行化；进程隔离后与其它测试二进制互不影响。
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// configure 可能改写的全部环境键。
const TOUCHED_KEYS: [&str; 8] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "http_proxy",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// RAII 保存/恢复：测试结束（含断言失败）时恢复所有触碰键的原始值。
struct EnvGuard {
    saved: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn capture() -> Self {
        let saved = TOUCHED_KEYS
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect();
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

/// 测试前置：清空全部代理环境键，保证断言不依赖宿主机配置。
fn clear_proxy_env() {
    for key in TOUCHED_KEYS {
        std::env::remove_var(key);
    }
}

fn settings_for(mode: &str, url: &str, no_proxy: &str) -> AppSettings {
    AppSettings {
        proxy_mode: mode.into(),
        proxy_url: url.into(),
        proxy_no_proxy: no_proxy.into(),
        ..AppSettings::default()
    }
}

#[test]
fn configure_switch_manual_to_system_restores_user_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::capture();
    clear_proxy_env();
    // 用户原始环境：HTTPS_PROXY 自设，其余未设置。
    std::env::set_var("HTTPS_PROXY", "http://user-env:1");
    // 首次 configure：手动模式覆写。
    configure(&settings_for("manual", "http://127.0.0.1:7890", ""));
    assert_eq!(
        std::env::var("HTTPS_PROXY").unwrap(),
        "http://127.0.0.1:7890"
    );
    // 切回跟随系统：恢复用户原始值（原值恢复、原本未设的移除）。
    configure(&settings_for("system", "", ""));
    assert_eq!(std::env::var("HTTPS_PROXY").unwrap(), "http://user-env:1");
    assert!(std::env::var("HTTP_PROXY").is_err());
    assert!(std::env::var("NO_PROXY").is_err());
}

#[test]
fn configure_switch_direct_to_system_restores_user_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::capture();
    clear_proxy_env();
    std::env::set_var("HTTPS_PROXY", "http://user-env:1");
    // 直连：清空代理变量 + NO_PROXY=*。
    configure(&settings_for("direct", "", ""));
    assert!(std::env::var("HTTPS_PROXY").is_err());
    assert_eq!(std::env::var("NO_PROXY").unwrap(), "*");
    // 切回跟随系统：恢复用户原始值，NO_PROXY=* 一并清除。
    configure(&settings_for("system", "", ""));
    assert_eq!(std::env::var("HTTPS_PROXY").unwrap(), "http://user-env:1");
    assert!(std::env::var("NO_PROXY").is_err());
}

#[test]
fn configure_repeated_manual_saves_do_not_recapture() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::capture();
    clear_proxy_env();
    std::env::set_var("HTTPS_PROXY", "http://user-env:1");
    // 多次手动保存（防抖写入）不会把覆写后的值误存为原始值。
    configure(&settings_for("manual", "http://first:1", ""));
    configure(&settings_for("manual", "http://second:2", ""));
    assert_eq!(std::env::var("HTTPS_PROXY").unwrap(), "http://second:2");
    configure(&settings_for("system", "", ""));
    assert_eq!(
        std::env::var("HTTPS_PROXY").unwrap(),
        "http://user-env:1",
        "restore must use the original value, not the last manual override"
    );
}

/// 更新器直连路径的回归测试：tauri 更新器每次请求用 reqwest::ClientBuilder
/// 新建客户端，reqwest 在 build() 内构造 hyper-util 的 system 匹配器（每次
/// 直接读环境变量 + Windows 注册表，无缓存）。本测试锁定该机制：
/// A) 已有环境代理 + `NO_PROXY=*` → 一律直连（直连模式更新器路由生效，
///    且不受宿主机注册表配置影响）；
/// B) 清空例外列表后 → 环境代理拦截 https 目标（跟随系统语义；环境键
///    非空时注册表只填充空位，无法覆盖环境值）。
#[test]
fn system_matcher_honors_no_proxy_wildcard_and_env_precedence() {
    use hyper_util::client::proxy::matcher::Matcher;

    let _guard = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::capture();
    clear_proxy_env();
    let uri: http::Uri = "https://api.github.com/latest.json".parse().unwrap();

    // A) 存在确定的环境代理 + NO_PROXY=*：无论注册表是否有系统代理都直连。
    std::env::set_var("HTTPS_PROXY", "http://env-proxy:1");
    std::env::set_var("NO_PROXY", "*");
    let matcher = Matcher::from_system();
    assert!(
        matcher.intercept(&uri).is_none(),
        "NO_PROXY=* must bypass the present env proxy (and any registry proxy)"
    );

    // B) 清空例外列表（大小写两套）：环境代理拦截。
    for key in ["NO_PROXY", "no_proxy"] {
        std::env::remove_var(key);
    }
    let matcher = Matcher::from_system();
    let intercepted = matcher
        .intercept(&uri)
        .expect("HTTPS_PROXY env should intercept https targets");
    assert_eq!(
        intercepted.uri().host(),
        Some("env-proxy"),
        "env proxy must take precedence over the registry"
    );
}
