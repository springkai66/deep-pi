//! 网络请求重试：临时性故障（DNS/连接/超时/5xx/429）自动退避重试几次，
//! 确定性失败（4xx、代理与 URL 配置错误、请求构造错误）立即返回。
//!
//! 语义约定：
//! - 重试对用户透明，仅记日志（event=network_retry），最终错误文案不变；
//! - 非幂等 POST（模型调用）只重试"连接建立失败"与网关类状态
//!   （429/502/503/504），避免超时后服务端已处理完成的重放；
//! - 429 的 Retry-After 头在可得时被尊重：等待超过预算直接失败；
//! - 阻塞等待均发生在 spawn_blocking 线程内。

use std::time::Duration;

/// 网络请求总尝试次数（首次 + 2 次重试）。
pub const NETWORK_ATTEMPTS: u32 = 3;
/// 各次重试前的退避等待（毫秒）。
pub const NETWORK_DELAYS_MS: [u64; 2] = [600, 1500];
/// Retry-After 等待上限：服务端要求等待更久时直接失败，不挂起调用方。
const MAX_RETRY_AFTER: Duration = Duration::from_secs(10);

/// 一次尝试的失败：message 是最终呈现给用户的错误，retryable 决定是否
/// 继续重试，status/retry_after 在响应已到达时携带（供调用方还原错误类型）。
#[derive(Debug)]
pub struct AttemptFailure {
    pub message: String,
    pub retryable: bool,
    pub status: Option<u16>,
    pub retry_after: Option<Duration>,
}

impl AttemptFailure {
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: true,
            status: None,
            retry_after: None,
        }
    }
}

/// HTTP 状态码可重试判定：429 与 5xx。
pub fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

/// ureq 错误的可重试判定（默认配置下 4xx/5xx 以 StatusCode 错误出现）。
pub fn is_retryable_ureq_error(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::StatusCode(status) => is_retryable_status(*status),
        ureq::Error::Io(_)
        | ureq::Error::Timeout(_)
        | ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed => true,
        _ => false,
    }
}

/// 非幂等 POST 的可重试判定：仅连接建立失败与网关类状态；超时与一般
/// IO 可能发生在服务端已受理之后，重放有重复计费风险，不重试。
pub fn is_retryable_post_error(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::StatusCode(status) => matches!(status, 429 | 502 | 503 | 504),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => true,
        _ => false,
    }
}

fn backoff_delay(attempt: u32, delays_ms: &[u64]) -> Duration {
    Duration::from_millis(
        delays_ms
            .get((attempt - 1) as usize)
            .copied()
            .unwrap_or(*delays_ms.last().unwrap_or(&1000)),
    )
}

/// 通用重试驱动：op 收到 1 起始的尝试序号；is_retryable 决定 Err 是否
/// 继续；retry_after 给出服务端要求的等待（超预算则放弃并返回错误）；
/// sleep 可注入；describe 给出一条日志用的失败描述（调用方负责脱敏与限长）。
/// 可重试错误逐次记日志（不含请求体与鉴权信息）。
// 参数都是注入点（重试策略 + 时钟 + 描述），拆成结构体反而更难读。
#[allow(clippy::too_many_arguments)]
pub fn run<T, E>(
    attempts: u32,
    delays_ms: &[u64],
    max_retry_after: Duration,
    mut op: impl FnMut(u32) -> Result<T, E>,
    is_retryable: &dyn Fn(&E) -> bool,
    retry_after: &dyn Fn(&E) -> Option<Duration>,
    describe: &dyn Fn(&E) -> String,
    sleep: &dyn Fn(Duration),
) -> Result<T, E> {
    let mut attempt: u32 = 1;
    loop {
        match op(attempt) {
            Ok(value) => return Ok(value),
            Err(error) if attempt < attempts && is_retryable(&error) => {
                // Retry-After 超预算：不等，直接返回错误；无头则按退避表。
                let delay = match retry_after(&error) {
                    Some(requested) if requested > max_retry_after => return Err(error),
                    Some(requested) => requested,
                    None => backoff_delay(attempt, delays_ms),
                };
                log::warn!(
                    "event=network_retry attempt={attempt} delay_ms={} {}",
                    delay.as_millis(),
                    describe(&error)
                );
                sleep(delay);
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// 失败类别：事后区分「DNS 失败 / TCP 连接失败 / CONNECT 被代理拒绝 / TLS
/// 失败 / 超时 / 上游 5xx」靠的就是这一列（见 #30）。分类依据是 ureq 的
/// Display 文案与已到达的状态码 —— 覆盖其 error.rs 中列出的几种措辞。
fn failure_category(failure: &AttemptFailure) -> &'static str {
    if let Some(status) = failure.status {
        return if status == 429 {
            "http_429"
        } else if (500..600).contains(&status) {
            "upstream_5xx"
        } else {
            "http_status"
        };
    }
    let message = failure.message.to_ascii_lowercase();
    if message.contains("proxy") && message.contains("connect") {
        // `CONNECT proxy failed`：隧道被上游代理拒绝或不可达。
        "proxy_connect"
    } else if message.contains("dns") || message.contains("lookup") || message.contains("resolve") {
        "dns"
    } else if message.contains("tls")
        || message.contains("certificate")
        || message.contains("handshake")
    {
        "tls"
    } else if message.contains("timed out") || message.contains("timeout") {
        "timeout"
    } else if message.contains("connection") || message.contains("connect") {
        "tcp_connect"
    } else {
        "other"
    }
}

/// 日志用的失败描述：类别 + 脱敏后的路由来源 + 脱敏限长的错误文本。
fn describe_failure(failure: &AttemptFailure) -> String {
    format!(
        "category={} {} error={}",
        failure_category(failure),
        crate::proxy::route_summary(),
        log_text(&failure.message)
    )
}

/// 日志文本上限：足够定位，又不至于把日志刷爆。
const MAX_LOG_CHARS: usize = 300;

/// 去掉 URL 里的 userinfo：`scheme://user:pass@host` → `scheme://[redacted]@host`。
/// `pi_auth::redact` 只覆盖 ≥32 位的长串，**短口令不会被它处理**，而日志里
/// 最可能出现的凭据形态就是 URL 的 userinfo，因此这里单独兜一层。
fn strip_userinfo(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find("://") {
        let (head, tail) = rest.split_at(index + 3);
        output.push_str(head);
        // authority 截止到第一个 '/'、'?'、'#' 或空白。
        let end = tail
            .find(|character: char| ['/', '?', '#', ' ', '\t'].contains(&character))
            .unwrap_or(tail.len());
        let authority = &tail[..end];
        match authority.rsplit_once('@') {
            Some((_, host)) => {
                output.push_str("[redacted]@");
                output.push_str(host);
            }
            None => output.push_str(authority),
        }
        rest = &tail[end..];
    }
    output.push_str(rest);
    output
}

/// 脱敏（先剥离 URL userinfo，再复用 pi_auth 的令牌规则：≥32 位
/// `[A-Za-z0-9_-]` 连续串整体替换）并截断。凭据因此不会进入日志，
/// 符合 SECURITY.md 的约束。
fn log_text(text: &str) -> String {
    let redacted = strip_userinfo(&crate::pi_auth::redact(text));
    if redacted.chars().count() <= MAX_LOG_CHARS {
        return redacted;
    }
    let mut truncated: String = redacted.chars().take(MAX_LOG_CHARS).collect();
    truncated.push('…');
    truncated
}

/// 网络请求重试入口：标准 3 次尝试 + 退避表 + Retry-After 上限。
pub fn retry_network<T>(
    op: impl FnMut(u32) -> Result<T, AttemptFailure>,
) -> Result<T, AttemptFailure> {
    run(
        NETWORK_ATTEMPTS,
        &NETWORK_DELAYS_MS,
        MAX_RETRY_AFTER,
        op,
        &|failure| failure.retryable,
        &|failure| failure.retry_after,
        &describe_failure,
        &|delay| std::thread::sleep(delay),
    )
}

/// 读取响应体（UTF-8，限制大小）。
pub fn read_body_limited(body: &mut ureq::Body, limit: u64) -> Result<String, String> {
    body.with_config()
        .limit(limit)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(|error| error.to_string())
}

/// 从响应头解析 Retry-After（仅支持秒数形式，HTTP 日期形式忽略）。
pub fn parse_retry_after_header(headers: &ureq::http::HeaderMap) -> Option<Duration> {
    let value = headers.get("retry-after")?.to_str().ok()?;
    let seconds: u64 = value.trim().parse().ok()?;
    (seconds > 0).then(|| Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug)]
    enum TestError {
        Transient(&'static str),
        Fatal(&'static str),
    }

    #[test]
    fn returns_first_success_without_sleeping() {
        let sleeps = RefCell::new(Vec::new());
        let mut calls = 0;
        let result = run(
            3,
            &[10, 20],
            MAX_RETRY_AFTER,
            |_attempt| {
                calls += 1;
                Ok::<_, TestError>("ok")
            },
            &|error| matches!(error, TestError::Transient(_)),
            &|_| None,
            &|_| String::new(),
            &|delay| sleeps.borrow_mut().push(delay),
        );
        assert_eq!(result.unwrap(), "ok");
        assert_eq!(calls, 1);
        assert!(sleeps.borrow().is_empty());
    }

    #[test]
    fn retries_transient_failure_then_succeeds_with_backoff() {
        let sleeps = RefCell::new(Vec::new());
        let mut calls = 0;
        let result = run(
            3,
            &[10, 20],
            MAX_RETRY_AFTER,
            |attempt| {
                calls += 1;
                if attempt < 3 {
                    Err(TestError::Transient("blip"))
                } else {
                    Ok("ok")
                }
            },
            &|error| matches!(error, TestError::Transient(_)),
            &|_| None,
            &|_| String::new(),
            &|delay| sleeps.borrow_mut().push(delay),
        );
        assert_eq!(result.unwrap(), "ok");
        assert_eq!(calls, 3);
        assert_eq!(
            *sleeps.borrow(),
            [Duration::from_millis(10), Duration::from_millis(20)]
        );
    }

    #[test]
    fn fails_fast_on_non_retryable_error() {
        let mut calls = 0;
        let result: Result<(), TestError> = run(
            3,
            &[10, 20],
            MAX_RETRY_AFTER,
            |_attempt| {
                calls += 1;
                Err(TestError::Fatal("bad request"))
            },
            &|error| matches!(error, TestError::Transient(_)),
            &|_| None,
            &|_| String::new(),
            &|_| panic!("must not sleep"),
        );
        assert!(matches!(result, Err(TestError::Fatal("bad request"))));
        assert_eq!(calls, 1);
    }

    #[test]
    fn gives_up_after_exhausting_attempts_with_last_error() {
        let sleeps = RefCell::new(Vec::new());
        let mut calls = 0;
        let result: Result<(), TestError> = run(
            3,
            &[10, 20],
            MAX_RETRY_AFTER,
            |_attempt| {
                calls += 1;
                Err(TestError::Transient("still down"))
            },
            &|error| matches!(error, TestError::Transient(_)),
            &|_| None,
            &|_| String::new(),
            &|delay| sleeps.borrow_mut().push(delay),
        );
        assert!(matches!(result, Err(TestError::Transient("still down"))));
        assert_eq!(calls, 3);
        assert_eq!(sleeps.borrow().len(), 2);
    }

    #[test]
    fn honors_retry_after_header_within_budget() {
        let sleeps = RefCell::new(Vec::new());
        let mut calls = 0;
        let result: Result<(), TestError> = run(
            2,
            &[10, 20],
            MAX_RETRY_AFTER,
            |_attempt| {
                calls += 1;
                if calls == 1 {
                    Err(TestError::Transient("429"))
                } else {
                    Ok(())
                }
            },
            &|error| matches!(error, TestError::Transient(_)),
            &|_| Some(Duration::from_secs(3)),
            &|_| String::new(),
            &|delay| sleeps.borrow_mut().push(delay),
        );
        assert!(result.is_ok());
        assert_eq!(*sleeps.borrow(), [Duration::from_secs(3)]);
    }

    #[test]
    fn gives_up_when_retry_after_exceeds_budget() {
        let mut calls = 0;
        let result: Result<(), TestError> = run(
            3,
            &[10, 20],
            MAX_RETRY_AFTER,
            |_attempt| {
                calls += 1;
                Err(TestError::Transient("429"))
            },
            &|error| matches!(error, TestError::Transient(_)),
            &|_| Some(Duration::from_secs(60)),
            &|_| String::new(),
            &|_| panic!("must not sleep beyond budget"),
        );
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }

    /// #30：日志描述必须含**失败类别**与**代理模式**，且不含凭据。
    /// 这三种故障此前在日志里完全无法区分。
    #[test]
    fn failure_description_carries_category_and_route_without_secrets() {
        let failure = |message: &str, status: Option<u16>| AttemptFailure {
            message: message.into(),
            retryable: true,
            status,
            retry_after: None,
        };
        // 隧道被上游代理拒绝 —— 与 DNS/TCP 失败必须分开。
        let described =
            describe_failure(&failure("CONNECT proxy failed: connection refused", None));
        assert!(described.contains("category=proxy_connect"), "{described}");
        // 每一次日志都带当前生效的模式与（脱敏的）来源。
        assert!(described.contains("mode="), "{described}");
        assert!(described.contains("route="), "{described}");
        // 其余类别各自成类。
        assert!(
            describe_failure(&failure("bad gateway", Some(503))).contains("category=upstream_5xx")
        );
        assert!(describe_failure(&failure("too many requests", Some(429)))
            .contains("category=http_429"));
        assert!(describe_failure(&failure("dns lookup failed", None)).contains("category=dns"));
        assert!(
            describe_failure(&failure("connection timed out", None)).contains("category=timeout")
        );
        assert!(describe_failure(&failure("tls handshake failed", None)).contains("category=tls"));
        assert!(
            describe_failure(&failure("connection refused", None)).contains("category=tcp_connect")
        );

        // 凭据与查询串绝不出现：短口令靠 userinfo 剥离，长令牌靠 redact。
        let token = "a".repeat(40);
        let described = describe_failure(&failure(
            &format!("failed for http://user:secret@proxy.local:8080/path?token={token}"),
            None,
        ));
        assert!(!described.contains("secret"), "{described}");
        assert!(!described.contains(&token), "{described}");
        // 查询串的**值**必须消失；参数名保留（它不是凭据，且有助于定位）。
        assert!(
            !described.contains(&format!("token={token}")),
            "{described}"
        );
        assert!(
            described.contains("[redacted]@proxy.local:8080"),
            "{described}"
        );

        // 超长文本被截断，避免刷爆日志。
        let capped = log_text(&"err ".repeat(200));
        assert_eq!(capped.chars().count(), MAX_LOG_CHARS + 1);
    }

    #[test]
    fn classifies_statuses_and_ureq_errors() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(503));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(404));
        // 类型层面确认分类器接受真实 ureq 错误。
        let status_error = ureq::Error::StatusCode(503);
        assert!(is_retryable_ureq_error(&status_error));
        let client_error = ureq::Error::StatusCode(404);
        assert!(!is_retryable_ureq_error(&client_error));
    }
}
