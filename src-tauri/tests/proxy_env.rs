//! 代理设置的进程环境集成测试：独立测试进程运行，与库内单测进程隔离，
//! 避免环境变量改写影响并行用例（git 子进程等）。
//!
//! 覆盖三类真实边界：
//! 1. `configure()` 的手动/系统模式切换与环境隔离（旧代理不泄漏）；
//! 2. tauri 更新器所依赖的 reqwest→hyper-util system 匹配器对
//!    `NO_PROXY=*` 与环境代理的处理（显式绕过/环境代理匹配）；
//! 3. 回环中继上线前后，真实子进程环境里代理地址的变化（#27：地址恒定）。

use deeppi_lib::proxy::configure;
use deeppi_lib::settings::AppSettings;

/// 本进程内的环境测试串行化；进程隔离后与其它测试二进制互不影响。
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 测试会改写或向子进程注入的全部代理相关环境键。
const TOUCHED_KEYS: [&str; 9] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "http_proxy",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
    "NODE_USE_ENV_PROXY",
];

/// RAII 保存/恢复：测试结束（含断言失败）时恢复所有触碰键的原始值。
struct EnvGuard {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    fn capture() -> Self {
        let saved = TOUCHED_KEYS
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
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

/// 共享夹具：串行化 + 把进程级代理状态归一化。
///
/// `proxy::configure` 把手动模式前捕获的 `saved_env` 存在模块级 `STATE` 里，
/// 会跨用例存活。每个用例进出时都切回系统模式、清除快照，避免用例顺序
/// 影响新一轮手动模式的环境捕获；系统模式按实际 OS 设置重新映射，而不复用
/// 启动器继承的旧代理地址。
struct ProxyTestState {
    /// 先 drop：把环境恢复成用例开始前的样子。
    _env: EnvGuard,
    /// 后 drop：释放串行锁。
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Drop for ProxyTestState {
    fn drop(&mut self) {
        // 清空跨用例的 saved_env；然后 _env 恢复用例外的原始（包括非 UTF-8）环境。
        // 此时仍持有锁，其他用例不会看到清理到一半的进程环境。
        configure(&settings_for("system", "", ""));
    }
}

fn reset_proxy_state() -> ProxyTestState {
    // 某个用例 panic 不该把后续用例一起带走：毒化的锁直接取回内部值。
    let lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = EnvGuard::capture();
    // 归一化 STATE：system 模式清空 saved_env 并重建真实 OS 代理映射。
    configure(&settings_for("system", "", ""));
    clear_proxy_env();
    ProxyTestState {
        _env: env,
        _lock: lock,
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

/// 系统模式的进程环境必须按**当前 OS 检测结果**映射；机器可能启用了
/// WinINET 显式代理，也可能关闭了它，测试不能写死其中一种状态。
fn assert_system_process_env(stale_proxy: &str) -> Option<String> {
    let os_proxy = deeppi_lib::proxy::current_url();
    assert_ne!(
        os_proxy.as_deref(),
        Some(stale_proxy),
        "test sentinel must not coincidentally be the OS proxy"
    );
    match &os_proxy {
        Some(url) => {
            for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
                assert_eq!(
                    std::env::var(key).as_deref(),
                    Ok(url.as_str()),
                    "{key} must use the OS proxy"
                );
            }
            for key in ["ALL_PROXY", "all_proxy"] {
                assert!(
                    std::env::var(key).is_err(),
                    "{key} must not bypass the OS proxy"
                );
            }
            assert_ne!(std::env::var("NO_PROXY").as_deref(), Ok("*"));
        }
        None => {
            for key in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "http_proxy",
                "https_proxy",
                "ALL_PROXY",
                "all_proxy",
            ] {
                assert!(
                    std::env::var(key).is_err(),
                    "{key} must not retain an inherited proxy"
                );
            }
            assert_eq!(std::env::var("NO_PROXY").as_deref(), Ok("*"));
            assert_eq!(std::env::var("no_proxy").as_deref(), Ok("*"));
        }
    }
    os_proxy
}

#[test]
fn configure_switch_manual_to_system_ignores_inherited_proxy() {
    let _state = reset_proxy_state();
    let stale = "http://stale-config.invalid:43129";
    std::env::set_var("HTTPS_PROXY", stale);
    std::env::set_var("ALL_PROXY", stale);
    // 手动模式覆盖继承代理、清除 ALL_PROXY，防止绕过手动配置。
    configure(&settings_for("manual", "http://127.0.0.1:7890", ""));
    for key in ["HTTP_PROXY", "HTTPS_PROXY"] {
        assert_eq!(std::env::var(key).as_deref(), Ok("http://127.0.0.1:7890"));
    }
    assert!(std::env::var("ALL_PROXY").is_err());
    // 系统模式不能恢复旧启动器代理：以 OS 当前设置为准，关闭则直连。
    configure(&settings_for("system", "", ""));
    assert_system_process_env(stale);
}

#[test]
fn configure_repeated_manual_saves_keep_latest_then_return_to_os() {
    let _state = reset_proxy_state();
    let stale = "http://stale-config.invalid:43129";
    std::env::set_var("HTTPS_PROXY", stale);
    // 多次手动保存（防抖写入）只能使用最新显式地址，不能重捕获上次覆盖值。
    configure(&settings_for("manual", "http://first:1", ""));
    assert_eq!(
        std::env::var("HTTPS_PROXY").as_deref(),
        Ok("http://first:1")
    );
    configure(&settings_for("manual", "http://second:2", ""));
    for key in ["HTTP_PROXY", "HTTPS_PROXY"] {
        assert_eq!(std::env::var(key).as_deref(), Ok("http://second:2"));
    }
    // 手动切回系统后，既不能留 first/second，也不能还原继承的旧端口。
    configure(&settings_for("system", "", ""));
    assert_system_process_env(stale);
}

/// reqwest 每次构造 hyper-util system 匹配器时读取环境和 Windows 注册表。
/// 本测试锁定其低层机制，与应用代理模式无关：
/// A) 已有环境代理 + `NO_PROXY=*` → 匹配器一律绕过（不受注册表影响）；
/// B) 清空例外列表后 → 环境代理拦截 https 目标（环境键非空时注册表
///    只填充空位，无法覆盖环境值）。
#[test]
fn system_matcher_honors_no_proxy_wildcard_and_env_precedence() {
    use hyper_util::client::proxy::matcher::Matcher;

    let _state = reset_proxy_state();
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

/// 真实子进程的环境（`cmd /c set`），键统一大写以便大小写无关地断言。
#[cfg(windows)]
fn child_process_env() -> std::collections::HashMap<String, String> {
    use deeppi_lib::proxy::apply_to_command;

    let mut command = std::process::Command::new("cmd");
    command.args(["/c", "set"]);
    apply_to_command(&mut command);
    let output = command.output().expect("cmd /c set should run");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_ascii_uppercase(), value.trim().to_owned()))
        .collect()
}

/// #27 的端到端回归：中继上线前手动模式注入真实地址，上线后系统/手动
/// 模式都指向同一个回环地址，且**切换模式不改变地址**——已运行的 DSH/Pi
/// 会话无需重启。即使父进程原本有代理环境变量，也不能让子进程绕过中继。
#[cfg(windows)]
#[test]
fn child_process_env_points_at_a_constant_relay_address() {
    use deeppi_lib::proxy_relay;

    let _state = reset_proxy_state();
    let stale = "http://stale-config.invalid:43129";
    // 先验证中继尚未启动时的 system 路径：不论 OS 是否启用显式代理，
    // 子进程绝不能继承旧代理地址。
    assert!(
        proxy_relay::global_port().is_none(),
        "fixture needs an unstarted relay"
    );
    std::env::set_var("HTTP_PROXY", stale);
    std::env::set_var("HTTPS_PROXY", stale);
    std::env::set_var("ALL_PROXY", stale);
    std::env::set_var("NODE_USE_ENV_PROXY", "1");
    configure(&settings_for("system", "", ""));
    let os_proxy = assert_system_process_env(stale);
    let without_relay = child_process_env();
    assert!(
        !without_relay.contains_key("ALL_PROXY"),
        "child must not inherit ALL_PROXY"
    );
    match os_proxy {
        Some(url) => {
            for key in ["HTTP_PROXY", "HTTPS_PROXY"] {
                assert_eq!(
                    without_relay.get(key).map(String::as_str),
                    Some(url.as_str())
                );
            }
            assert_eq!(
                without_relay.get("NODE_USE_ENV_PROXY").map(String::as_str),
                Some("1")
            );
        }
        None => {
            for key in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "NODE_USE_ENV_PROXY",
            ] {
                assert!(
                    !without_relay.contains_key(key),
                    "system-off child must clear {key}"
                );
            }
            assert_eq!(without_relay.get("NO_PROXY").map(String::as_str), Some("*"));
        }
    }

    // 1) 中继未启动：手动模式注入显式地址。
    configure(&settings_for("manual", "http://upstream.invalid:1", ""));
    let before = child_process_env();
    assert_eq!(
        before.get("HTTP_PROXY").map(String::as_str),
        Some("http://upstream.invalid:1"),
        "without a relay the manual address must still be injected"
    );

    // 2) 中继启动（幂等）：子进程改指向回环中继。
    let port = proxy_relay::start_global().expect("the relay must bind loopback");
    let relay = format!("http://127.0.0.1:{port}");
    let after = child_process_env();
    for key in ["HTTP_PROXY", "HTTPS_PROXY"] {
        assert_eq!(after.get(key).map(String::as_str), Some(relay.as_str()));
    }
    assert_eq!(
        after.get("NODE_USE_ENV_PROXY").map(String::as_str),
        Some("1")
    );
    assert!(
        !after.contains_key("ALL_PROXY"),
        "ALL_PROXY must be cleared so children cannot bypass the relay"
    );

    // 3) 切换模式：地址恒定，系统模式不能把陈旧地址传给子进程。
    for mode in ["system", "manual"] {
        let url = if mode == "manual" {
            "http://upstream.invalid:2"
        } else {
            ""
        };
        configure(&settings_for(mode, url, ""));
        let env = child_process_env();
        for key in ["HTTP_PROXY", "HTTPS_PROXY"] {
            assert_eq!(
                env.get(key).map(String::as_str),
                Some(relay.as_str()),
                "mode {mode} must keep the constant relay address for {key}"
            );
        }
        assert!(!env.contains_key("ALL_PROXY"));
        assert_eq!(env.get("NODE_USE_ENV_PROXY").map(String::as_str), Some("1"));
    }
}
