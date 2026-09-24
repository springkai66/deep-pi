//! 本地回环中继（#27）：给子进程一个**恒定不变**的代理地址，把「切换代理模式」
//! 搬到我们自己控制的这一跳里。
//!
//! 动机：子进程的代理环境在 spawn 那一刻冻结（`dsh.rs:604`），而 DSH 侧的代理
//! 策略由 `@deepseek-ai/dsh-http-proxy` 在进程启动时解析一次后也不再重读。于是
//! 唯一能做到「切模式不重启子进程」的形态，就是让子进程持有的地址从生到死不变，
//! 上游在我们这边换：**监听 socket 不变，新连接走新上游**。
//!
//! 安全边界（`SECURITY.md`）：
//! - 只绑 `127.0.0.1`，绝不监听全网卡；
//! - 入站连接一律当不可信输入：请求行/头部有字节上限、握手有超时、畸形请求
//!   直接应答 400 并关闭，任何路径都不 panic；
//! - 上游不可达或地址非法时**返回 502，绝不静默回退直连**（静默回退会在用户
//!   以为在走代理时把流量泄漏出去）；
//! - 不支持代理认证：不转发 `Proxy-Authorization`，上游 URL 里的 userinfo 一律
//!   拒绝，也从不把上游地址写进日志或响应体。
//!
//! 已知边界（未处理，explicitly deferred）：
//! - 隧道建立后会清掉读写超时（否则会掐断长空闲的流式响应），因此一条**半开**
//!   连接（对端断电或挂起但未关闭）可以让一个管道线程一直阻塞。std 没有暴露
//!   `SO_KEEPALIVE`，要收紧需要 `socket2` 或 `windows-sys` 的 WinSock feature；
//!   线程本身有界（每个连接一条），所以先记为后续项而不是在本次引入新依赖。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock, RwLock};
use std::thread;
use std::time::Duration;

use tauri::Url;

/// 中继只绑这一个回环地址。
const LOOPBACK: &str = "127.0.0.1";
/// 请求行长度上限。
const MAX_REQUEST_LINE: usize = 8 * 1024;
/// 头部块总字节上限（含逐行长度上限）。
const MAX_HEADER_BYTES: usize = 32 * 1024;
/// 头部行数上限。
const MAX_HEADER_LINES: usize = 100;
/// chunked 分块长度行上限。
const MAX_CHUNK_LINE: usize = 1024;
/// 握手（读请求头）超时：只约束头部，隧道建立后不再限制空闲。
const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// 连接上游/目标的超时。
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// 拒绝请求时排空输入的字节上限（只为让对端读到响应，见 `drain_before_close`）。
const MAX_DRAIN_BYTES: usize = 256 * 1024;
/// 排空输入的读超时。
const DRAIN_TIMEOUT: Duration = Duration::from_millis(250);

/// 中继的上游：直连目标，或把请求交给上游 HTTP 代理。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Upstream {
    /// 直连目标。应用层「直连」的语义止于此：TUN 仍可能接管流量（见 #20）。
    #[default]
    Direct,
    /// 经上游 HTTP 代理（`http://host:port`）。
    ViaProxy(String),
    /// 系统已启用某项设置，但本模块无法安全地表达或求值（如 PAC 求值失败、
    /// 需要认证、仅 SOCKS）。调用方**不得**把它当作 Direct：会泄漏本应走代理
    /// 的流量，因此中继对该目标一律 502。
    Unsupported(String),
}

impl Upstream {
    /// 中继能否承载该上游。只支持 `http://` 上游：`https://` 上游要求中继自己
    /// 先与代理做 TLS 握手，本模块不引入 TLS 栈，因此调用方应保持原有注入行为。
    pub fn is_relay_supported(&self) -> bool {
        match self {
            Upstream::Direct => true,
            Upstream::Unsupported(_) => false,
            Upstream::ViaProxy(raw) => Url::parse(raw.trim())
                .map(|url| url.scheme() == "http")
                .unwrap_or(false),
        }
    }
}

/// 按目标解析上游：`CONNECT` 用合成的 `https://host:port/`，普通 HTTP 用绝对 URL。
/// 系统模式据此实现按协议分流、PAC 与例外列表；手动模式忽略目标。
type Resolver = Arc<dyn Fn(&str) -> Upstream + Send + Sync>;

fn resolve_target(head: &RequestHead) -> String {
    if head.method == "CONNECT" {
        format!("https://{}/", head.target)
    } else {
        head.target.clone()
    }
}

/// 全局上游：`configure()` 写入，接受循环按**连接**读取。
static GLOBAL_UPSTREAM: RwLock<Upstream> = RwLock::new(Upstream::Direct);
/// 全局中继端口；`None` 表示未启动（此时调用方保持既有注入行为）。
static GLOBAL_PORT: OnceLock<u16> = OnceLock::new();

/// 绑定回环随机端口。单独暴露以便测试断言绑定地址是回环。
pub fn bind_loopback() -> std::io::Result<TcpListener> {
    TcpListener::bind((LOOPBACK, 0))
}

/// 当前中继端口；未启动为 `None`。
pub fn global_port() -> Option<u16> {
    GLOBAL_PORT.get().copied()
}

/// 当前中继地址（`http://127.0.0.1:<port>`）；未启动为 `None`。
pub fn global_url() -> Option<String> {
    global_port().map(|port| format!("http://{LOOPBACK}:{port}"))
}

/// 切换全局上游。中继未启动时同样记录，启动后立即生效。
pub fn set_global_upstream(upstream: Upstream) {
    if let Ok(mut guard) = GLOBAL_UPSTREAM.write() {
        *guard = upstream;
    }
}

fn global_upstream() -> Upstream {
    GLOBAL_UPSTREAM
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

/// 启动全局中继（幂等）。返回实际端口。
pub fn start_global() -> Result<u16, String> {
    if let Some(port) = global_port() {
        return Ok(port);
    }
    let listener =
        bind_loopback().map_err(|error| format!("failed to bind proxy relay: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("failed to read proxy relay address: {error}"))?
        .port();
    // 只对全局中继按连接重读系统设置；测试用 spawn_with 仍取注入的共享上游。
    // Windows 设置变化（开启/关闭代理）与 PAC 求值都无需重启 DeepPi 或 DSH。
    spawn_accept_loop(
        listener,
        |target: &str| crate::proxy::live_upstream_for(target).unwrap_or_else(global_upstream),
        Limits::default(),
    );
    let _ = GLOBAL_PORT.set(port);
    Ok(port)
}

/// 在给定监听器上启动中继，上游由共享状态读取（测试与嵌入式使用）。
/// 该入口不按目标分流：整个中继共用一个上游，供测试直接驱动切换语义。
pub fn spawn_with(listener: TcpListener, upstream: Arc<RwLock<Upstream>>) {
    spawn_accept_loop(
        listener,
        move |_target: &str| {
            upstream
                .read()
                .map(|guard| guard.clone())
                .unwrap_or_default()
        },
        Limits::default(),
    );
}

/// 中继的超时预算。测试用短超时验证握手超时真的生效。
#[derive(Clone, Copy)]
struct Limits {
    handshake: Duration,
    connect: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            handshake: DEFAULT_HANDSHAKE_TIMEOUT,
            connect: DEFAULT_CONNECT_TIMEOUT,
        }
    }
}

fn spawn_accept_loop<F>(listener: TcpListener, upstream: F, limits: Limits)
where
    F: Fn(&str) -> Upstream + Send + Sync + 'static,
{
    let upstream: Resolver = Arc::new(upstream);
    let spawned = thread::Builder::new()
        .name("proxy-relay-accept".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(client) = stream else { break };
                // 解析器按连接带入线程；上游在读到请求目标后才解析（见
                // `handle_connection`），因此 PAC/按协议分流能对每条请求生效。
                let upstream = upstream.clone();
                let spawned = thread::Builder::new()
                    .name("proxy-relay-conn".into())
                    .spawn(move || handle_connection(client, upstream, limits));
                if spawned.is_err() {
                    // 线程耗尽：丢弃该连接，不 panic、不影响监听循环。
                    log::debug!("event=proxy_relay status=conn_thread_unavailable");
                }
            }
        });
    if spawned.is_err() {
        log::warn!("event=proxy_relay status=accept_thread_unavailable");
    }
}

/// 请求头解析结果。
struct RequestHead {
    method: String,
    target: String,
    version: String,
    headers: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum HeadError {
    /// 对端在读完请求头前关闭。
    Eof,
    /// 请求行/头部超过上限。
    TooLarge,
    /// 请求行结构非法。
    Malformed,
}

fn handle_connection(mut client: TcpStream, resolve: Resolver, limits: Limits) {
    let _ = client.set_read_timeout(Some(limits.handshake));
    let _ = client.set_write_timeout(Some(limits.handshake));
    let Ok(reader_stream) = client.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(reader_stream);
    let head = match read_head(&mut reader) {
        Ok(head) => head,
        Err(HeadError::Eof) => return,
        Err(HeadError::TooLarge) => {
            log::debug!("event=proxy_relay status=request_head_too_large");
            respond_status(
                &mut client,
                400,
                "Bad Request",
                "proxy request head is too large",
            );
            drain_before_close(&mut client);
            return;
        }
        Err(HeadError::Malformed) => {
            log::debug!("event=proxy_relay status=malformed_request");
            respond_status(&mut client, 400, "Bad Request", "malformed proxy request");
            drain_before_close(&mut client);
            return;
        }
    };
    // 握手完成：隧道可能是长时间空闲的流式响应（SSE），不保留读超时。
    let _ = client.set_read_timeout(None);
    let _ = client.set_write_timeout(None);
    // 上游在这里才解析：系统模式的按协议分流、PAC 与例外列表都依赖目标。
    let upstream = resolve(&resolve_target(&head));
    if head.method == "CONNECT" {
        relay_connect(client, reader, head, upstream, limits);
    } else {
        relay_http(client, reader, head, upstream, limits);
    }
}

/// `CONNECT host:port`：与目标（或经上游代理）建立隧道后双向透传。
fn relay_connect(
    mut client: TcpStream,
    reader: BufReader<TcpStream>,
    head: RequestHead,
    upstream: Upstream,
    limits: Limits,
) {
    let Some((host, port)) = parse_authority(&head.target) else {
        respond_status(&mut client, 400, "Bad Request", "invalid CONNECT target");
        return;
    };
    match upstream {
        Upstream::Unsupported(reason) => {
            // 系统启用了本模块无法安全表达的路由：明确失败，绝不改走直连。
            log::warn!("event=proxy_relay status=unsupported_route");
            respond_status(&mut client, 502, "Bad Gateway", &reason);
        }
        Upstream::Direct => match dial(&host, port, limits) {
            Ok(target) => {
                respond_connect_established(&mut client);
                let Ok(target_reader) = target.try_clone().map(BufReader::new) else {
                    return;
                };
                tunnel(reader, client, target_reader, target);
            }
            Err(_) => {
                log::debug!("event=proxy_relay status=target_unreachable");
                respond_status(
                    &mut client,
                    502,
                    "Bad Gateway",
                    "cannot reach CONNECT target",
                );
            }
        },
        Upstream::ViaProxy(raw) => {
            let (proxy_host, proxy_port) = match upstream_proxy_addr(&raw) {
                Ok(address) => address,
                Err(_) => {
                    log::debug!("event=proxy_relay status=upstream_address_rejected");
                    respond_status(
                        &mut client,
                        502,
                        "Bad Gateway",
                        "upstream proxy address is unusable",
                    );
                    return;
                }
            };
            let mut proxy = match dial(&proxy_host, proxy_port, limits) {
                Ok(proxy) => proxy,
                Err(_) => {
                    log::debug!("event=proxy_relay status=upstream_unreachable");
                    respond_status(
                        &mut client,
                        502,
                        "Bad Gateway",
                        "cannot reach upstream proxy",
                    );
                    return;
                }
            };
            let Ok(mut proxy_reader) = proxy.try_clone().map(BufReader::new) else {
                respond_status(
                    &mut client,
                    502,
                    "Bad Gateway",
                    "upstream proxy is unusable",
                );
                return;
            };
            // 只重建 CONNECT 行与 Host：客户端携带的 Proxy-* 凭据头一律不转发。
            let request = format!(
                "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n",
                target = head.target
            );
            if proxy.write_all(request.as_bytes()).is_err() || proxy.flush().is_err() {
                respond_status(
                    &mut client,
                    502,
                    "Bad Gateway",
                    "upstream proxy write failed",
                );
                return;
            }
            // 一直复用同一个 BufReader：缓冲区里可能已经读进的隧道数据不会丢。
            let status = match read_head_raw(&mut proxy_reader) {
                Ok(Some((status, _))) => status,
                _ => {
                    log::debug!("event=proxy_relay status=upstream_no_response");
                    respond_status(
                        &mut client,
                        502,
                        "Bad Gateway",
                        "upstream proxy gave no response",
                    );
                    return;
                }
            };
            if !(200..300).contains(&status) {
                log::debug!("event=proxy_relay status=upstream_refused_tunnel");
                respond_status(
                    &mut client,
                    502,
                    "Bad Gateway",
                    "upstream proxy refused the tunnel",
                );
                return;
            }
            respond_connect_established(&mut client);
            tunnel(reader, client, proxy_reader, proxy);
        }
    }
}

/// 普通 HTTP 请求：`Direct` 重写为 origin-form 直连目标；`ViaProxy` 原样交给
/// 上游代理。一次连接只处理一个请求（转发时声明 `Connection: close`）。
fn relay_http(
    mut client: TcpStream,
    reader: BufReader<TcpStream>,
    head: RequestHead,
    upstream: Upstream,
    limits: Limits,
) {
    let (connect_host, connect_port, request_line) = match &upstream {
        Upstream::Unsupported(reason) => {
            log::warn!("event=proxy_relay status=unsupported_route");
            respond_status(&mut client, 502, "Bad Gateway", reason);
            return;
        }
        Upstream::Direct => match absolute_form_target(&head.target) {
            Some((host, port, origin_form)) => (
                host,
                port,
                format!("{} {} {}", head.method, origin_form, head.version),
            ),
            None => {
                log::debug!("event=proxy_relay status=invalid_request_target");
                respond_status(
                    &mut client,
                    400,
                    "Bad Request",
                    "proxy request target is not an absolute http URL",
                );
                return;
            }
        },
        Upstream::ViaProxy(raw) => match upstream_proxy_addr(raw) {
            Ok((host, port)) => (
                host,
                port,
                format!("{} {} {}", head.method, head.target, head.version),
            ),
            Err(_) => {
                log::debug!("event=proxy_relay status=upstream_address_rejected");
                respond_status(
                    &mut client,
                    502,
                    "Bad Gateway",
                    "upstream proxy address is unusable",
                );
                return;
            }
        },
    };
    let mut outbound = match dial(&connect_host, connect_port, limits) {
        Ok(outbound) => outbound,
        Err(_) => {
            log::debug!("event=proxy_relay status=upstream_unreachable");
            respond_status(
                &mut client,
                502,
                "Bad Gateway",
                "cannot reach the request target",
            );
            return;
        }
    };
    let Ok(mut outbound_reader) = outbound.try_clone().map(BufReader::new) else {
        respond_status(&mut client, 502, "Bad Gateway", "upstream is unusable");
        return;
    };

    let expect_continue = head.headers.iter().any(|line| {
        header_is(line, "expect") && header_value(line).eq_ignore_ascii_case("100-continue")
    });
    let mut outbound_head = String::with_capacity(256);
    outbound_head.push_str(&request_line);
    outbound_head.push_str("\r\n");
    for line in forwardable_request_headers(&head.headers) {
        outbound_head.push_str(&line);
        outbound_head.push_str("\r\n");
    }
    outbound_head.push_str("Connection: close\r\n\r\n");
    if outbound.write_all(outbound_head.as_bytes()).is_err() || outbound.flush().is_err() {
        respond_status(&mut client, 502, "Bad Gateway", "upstream write failed");
        return;
    }

    // 客户端在等 100-continue 时不会发消息体；我们自己应答，避免双向等待。
    if expect_continue
        && (client.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").is_err() || client.flush().is_err())
    {
        return;
    }

    let mut client_reader = reader;
    if forward_body(
        &mut client_reader,
        &mut outbound,
        request_framing(&head.headers),
    )
    .is_err()
    {
        return;
    }
    let _ = outbound.flush();

    let _ = relay_response(&mut outbound_reader, &mut client, &head.method);
    let _ = client.flush();
}

/// 把上游/目标的响应转发给客户端：状态行与头部原样转发（滤掉逐跳头并声明
/// `Connection: close`），消息体按 framing 转发。
fn relay_response<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    method: &str,
) -> std::io::Result<()> {
    loop {
        let Some((status, headers)) = read_head_raw(reader)? else {
            return Err(unexpected_eof());
        };
        let status_line = headers.first().cloned().unwrap_or_default();
        writer.write_all(status_line.as_bytes())?;
        writer.write_all(b"\r\n")?;
        for line in headers.iter().skip(1) {
            if is_hop_by_hop(line) {
                continue;
            }
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\r\n")?;
        }
        writer.write_all(b"Connection: close\r\n\r\n")?;
        // 1xx 是过渡响应：转发后继续等真正的响应头。
        if (100..200).contains(&status) {
            continue;
        }
        let framing = if method == "HEAD" || status == 204 || status == 304 {
            Framing::None
        } else {
            framing_of(&headers)
        };
        return forward_body(reader, writer, framing);
    }
}

/// 上游 CONNECT 的响应头：只关心状态码，其余读掉即可。
fn read_head_raw<R: BufRead>(reader: &mut R) -> std::io::Result<Option<(u16, Vec<String>)>> {
    let Some(first) = read_line_bounded(reader, MAX_REQUEST_LINE)? else {
        return Ok(None);
    };
    let first = first.trim_end_matches(['\r', '\n']).to_string();
    let status = first
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok());
    let mut lines = vec![first];
    let mut total = 0usize;
    loop {
        let Some(line) = read_line_bounded(reader, MAX_HEADER_BYTES)? else {
            break;
        };
        let trimmed = line.trim_end_matches(['\r', '\n']).to_string();
        if trimmed.is_empty() {
            break;
        }
        total += trimmed.len();
        if total > MAX_HEADER_BYTES || lines.len() > MAX_HEADER_LINES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "head too large",
            ));
        }
        lines.push(trimmed);
    }
    Ok(status.map(|status| (status, lines)))
}

/// 转发消息体。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Framing {
    /// 无消息体（HEAD / 204 / 304 / 1xx）。
    None,
    /// `Content-Length`。
    Length(u64),
    /// `Transfer-Encoding: chunked`。
    Chunked,
    /// 读到对端关闭为止。
    UntilClose,
}

fn framing_of(headers: &[String]) -> Framing {
    let chunked = headers.iter().any(|line| {
        header_is(line, "transfer-encoding")
            && header_value(line).to_ascii_lowercase().contains("chunked")
    });
    if chunked {
        return Framing::Chunked;
    }
    if let Some(length) = headers
        .iter()
        .find(|line| header_is(line, "content-length"))
        .and_then(|line| header_value(line).trim().parse::<u64>().ok())
    {
        return Framing::Length(length);
    }
    Framing::UntilClose
}

/// 请求消息体的框架。与响应不同：请求既没有 `Content-Length` 也没有
/// `Transfer-Encoding` 就是**没有消息体**，不能按「读到对端关闭」处理，
/// 否则一个普通 GET 会让中继永久阻塞。
fn request_framing(headers: &[String]) -> Framing {
    match framing_of(headers) {
        Framing::UntilClose => Framing::None,
        framing => framing,
    }
}

fn forward_body<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    framing: Framing,
) -> std::io::Result<()> {
    match framing {
        Framing::None => Ok(()),
        Framing::Length(0) => Ok(()),
        Framing::Length(length) => {
            let copied = std::io::copy(&mut (&mut *reader).take(length), writer)?;
            if copied == length {
                Ok(())
            } else {
                Err(unexpected_eof())
            }
        }
        Framing::UntilClose => {
            std::io::copy(reader, writer)?;
            Ok(())
        }
        Framing::Chunked => loop {
            let Some(line) = read_line_bounded(reader, MAX_CHUNK_LINE)? else {
                return Err(unexpected_eof());
            };
            writer.write_all(line.as_bytes())?;
            let size = chunk_size(&line)?;
            if size == 0 {
                // 末块之后是 trailer，直到空行。
                loop {
                    let Some(trailer) = read_line_bounded(reader, MAX_CHUNK_LINE)? else {
                        return Err(unexpected_eof());
                    };
                    writer.write_all(trailer.as_bytes())?;
                    if trailer.trim_end_matches(['\r', '\n']).is_empty() {
                        return Ok(());
                    }
                }
            }
            let copied = std::io::copy(&mut (&mut *reader).take(size), writer)?;
            if copied != size {
                return Err(unexpected_eof());
            }
            let Some(crlf) = read_line_bounded(reader, 2)? else {
                return Err(unexpected_eof());
            };
            writer.write_all(crlf.as_bytes())?;
        },
    }
}

/// 双向透传。两侧的读半边与写半边都是各自克隆的句柄；`client_reader` 是已缓冲
/// 的读半边（可能已经读进了一部分数据），`target_reader` 同理（上游 CONNECT 的
/// 响应头可能已把隧道数据带进缓冲区）。
fn tunnel<R: Read + Send + 'static>(
    client_reader: R,
    mut client_writer: TcpStream,
    mut target_reader: impl Read,
    mut target_writer: TcpStream,
) {
    let upstream = thread::Builder::new()
        .name("proxy-relay-pipe".into())
        .spawn(move || {
            let mut reader = client_reader;
            let _ = std::io::copy(&mut reader, &mut target_writer);
            let _ = target_writer.shutdown(Shutdown::Write);
        });
    let _ = std::io::copy(&mut target_reader, &mut client_writer);
    let _ = client_writer.shutdown(Shutdown::Write);
    if let Ok(handle) = upstream {
        let _ = handle.join();
    }
}

/// 读一行（含换行符），带字节上限：EOF 返回 `Ok(None)`，超限返回 `InvalidData`。
/// 用 `take` 限制读取量，畸形输入既不会 panic 也不会无限分配。
fn read_line_bounded<R: BufRead>(reader: &mut R, limit: usize) -> std::io::Result<Option<String>> {
    let mut buffer = Vec::new();
    let read = (&mut *reader)
        .take(limit as u64)
        .read_until(b'\n', &mut buffer)?;
    if read == 0 {
        return Ok(None);
    }
    if buffer.last() != Some(&b'\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "line exceeds limit",
        ));
    }
    Ok(Some(String::from_utf8_lossy(&buffer).into_owned()))
}

fn read_head<R: BufRead>(reader: &mut R) -> Result<RequestHead, HeadError> {
    let line = read_line_bounded(reader, MAX_REQUEST_LINE).map_err(|error| match error.kind() {
        std::io::ErrorKind::InvalidData => HeadError::TooLarge,
        _ => HeadError::Eof,
    })?;
    let Some(line) = line else {
        return Err(HeadError::Eof);
    };
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.split_whitespace();
    let method = parts
        .next()
        .ok_or(HeadError::Malformed)?
        .to_ascii_uppercase();
    let target = parts.next().ok_or(HeadError::Malformed)?.to_string();
    let version = parts.next().unwrap_or("HTTP/1.1").to_string();
    let mut headers = Vec::new();
    let mut total = 0usize;
    loop {
        let line =
            read_line_bounded(reader, MAX_HEADER_BYTES).map_err(|error| match error.kind() {
                std::io::ErrorKind::InvalidData => HeadError::TooLarge,
                _ => HeadError::Eof,
            })?;
        let Some(line) = line else {
            return Err(HeadError::Eof);
        };
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        total += trimmed.len();
        if total > MAX_HEADER_BYTES || headers.len() >= MAX_HEADER_LINES {
            return Err(HeadError::TooLarge);
        }
        headers.push(trimmed.to_string());
    }
    Ok(RequestHead {
        method,
        target,
        version,
        headers,
    })
}

/// `host:port` / `[v6]:port`。
fn parse_authority(target: &str) -> Option<(String, u16)> {
    let (host, port) = target.rsplit_once(':')?;
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    let port: u16 = port.trim().parse().ok()?;
    if host.is_empty() || port == 0 {
        return None;
    }
    Some((host.to_string(), port))
}

/// 绝对形式请求目标 → (host, port, origin-form)。
fn absolute_form_target(target: &str) -> Option<(String, u16, String)> {
    let url = Url::parse(target.trim()).ok()?;
    if url.scheme() != "http" {
        return None;
    }
    let host = url.host_str()?.to_string();
    let port = url.port_or_known_default()?;
    let mut origin = if url.path().is_empty() {
        "/".to_string()
    } else {
        url.path().to_string()
    };
    if let Some(query) = url.query() {
        origin.push('?');
        origin.push_str(query);
    }
    Some((host, port, origin))
}

/// 上游代理地址 → (host, port)。**fail closed**：带 userinfo、非 http、缺主机或
/// 端口的上游一律拒绝，由调用方返回 502，绝不退化成直连。
fn upstream_proxy_addr(raw: &str) -> Result<(String, u16), String> {
    let url = Url::parse(raw.trim()).map_err(|error| error.to_string())?;
    if url.scheme() != "http" {
        return Err("upstream proxy must use http://".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("upstream proxy must not carry credentials".into());
    }
    let host = url
        .host_str()
        .ok_or("upstream proxy host is missing")?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or("upstream proxy port is missing")?;
    Ok((host, port))
}

fn dial(host: &str, port: u16, limits: Limits) -> std::io::Result<TcpStream> {
    let mut last = None;
    for address in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, limits.connect) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(error) => last = Some(error),
        }
    }
    Err(last
        .unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no usable address")))
}

fn header_is(line: &str, name: &str) -> bool {
    line.split(':')
        .next()
        .is_some_and(|candidate| candidate.trim().eq_ignore_ascii_case(name))
}

fn header_value(line: &str) -> &str {
    line.split_once(':')
        .map(|(_, value)| value.trim())
        .unwrap_or("")
}

/// 逐跳与代理专用头：不透传（凭据类尤其不能透传）。
fn is_hop_by_hop(line: &str) -> bool {
    [
        "connection",
        "keep-alive",
        "proxy-connection",
        "proxy-authorization",
        "te",
        "trailer",
        "upgrade",
    ]
    .iter()
    .any(|name| header_is(line, name))
}

/// 转发给目标/上游的请求头：滤掉逐跳与代理凭据头，`Expect` 由中继自己应答。
fn forwardable_request_headers(headers: &[String]) -> Vec<String> {
    headers
        .iter()
        .filter(|line| !is_hop_by_hop(line) && !header_is(line, "expect"))
        .cloned()
        .collect()
}

fn chunk_size(line: &str) -> std::io::Result<u64> {
    let digits = line.split(';').next().unwrap_or("").trim();
    u64::from_str_radix(digits, 16)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid chunk size"))
}

fn unexpected_eof() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "peer closed early")
}

fn respond_connect_established(stream: &mut TcpStream) {
    let _ = stream.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
    let _ = stream.flush();
}

/// 应答之后、关闭之前有界地排空输入：Windows 在接收缓冲区还有未读数据时关闭
/// socket 会直接发 RST，把已经写出的响应一起丢掉（客户端只看到连接被重置，
/// 读不到 400）。排空上限固定，对端持续灌数据也只是多读固定字节数。
fn drain_before_close(stream: &mut TcpStream) {
    let _ = stream.set_read_timeout(Some(DRAIN_TIMEOUT));
    let mut scratch = [0u8; 4096];
    let mut drained = 0usize;
    while drained < MAX_DRAIN_BYTES {
        match stream.read(&mut scratch) {
            Ok(0) => break,
            Ok(read) => drained += read,
            Err(_) => break,
        }
    }
}

fn respond_status(stream: &mut TcpStream, status: u16, reason: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 起一个单连接的假服务器，返回端口与被连接计数。
    fn spawn_server<F>(handler: F) -> (u16, Arc<AtomicUsize>)
    where
        F: Fn(TcpStream) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind((LOOPBACK, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                counter.fetch_add(1, Ordering::SeqCst);
                handler(stream);
            }
        });
        (port, hits)
    }

    /// 直连目标：CONNECT 隧道里没有请求头可读，accept 后立刻回一段标记数据并关闭。
    fn spawn_direct_target(marker: &'static str) -> (u16, Arc<AtomicUsize>) {
        spawn_server(move |mut stream| {
            let _ = stream.write_all(format!("direct:{marker}").as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Write);
        })
    }

    /// 假上游代理：应答 CONNECT 后回一段标记数据并关闭。
    fn spawn_upstream_proxy(
        marker: &'static str,
    ) -> (u16, Arc<AtomicUsize>, Arc<std::sync::Mutex<Vec<String>>>) {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = seen.clone();
        let (port, hits) = spawn_server(move |mut stream| {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            if let Ok(head) = read_head(&mut reader) {
                recorder
                    .lock()
                    .unwrap()
                    .push(format!("{} {}", head.method, head.target));
            }
            let _ = stream.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
            let _ = stream.write_all(format!("proxied:{marker}").as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Write);
        });
        (port, hits, seen)
    }

    fn relay_with(upstream: Upstream) -> (u16, Arc<RwLock<Upstream>>) {
        let listener = bind_loopback().unwrap();
        let port = listener.local_addr().unwrap().port();
        let shared = Arc::new(RwLock::new(upstream));
        spawn_with(listener, shared.clone());
        (port, shared)
    }

    fn connect(port: u16, request: &str) -> String {
        let mut stream = TcpStream::connect((LOOPBACK, port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
        String::from_utf8_lossy(&response).into_owned()
    }

    fn read_head<R: BufRead>(reader: &mut R) -> Result<RequestHead, ()> {
        super::read_head(reader).map_err(|_| ())
    }

    /// 按目标分流的解析器：系统模式的「按协议分流 / PAC / 例外列表」全依赖这一
    /// 能力。中继必须在读到请求目标后再解析上游，而不是整个进程共用一个上游。
    #[test]
    fn resolver_routes_each_target_independently() {
        let (proxy_port, _proxy_hits, seen) = spawn_upstream_proxy("P");
        let (direct_port, _direct_hits) = spawn_direct_target("D");
        let listener = bind_loopback().unwrap();
        let relay_port = listener.local_addr().unwrap().port();
        let proxied = format!("http://127.0.0.1:{proxy_port}");
        spawn_accept_loop(
            listener,
            move |target: &str| {
                // 只有目标端口等于假上游代理端口时才走代理，其余直连。
                if target.contains(&format!(":{proxy_port}")) {
                    Upstream::ViaProxy(proxied.clone())
                } else {
                    Upstream::Direct
                }
            },
            Limits {
                handshake: Duration::from_secs(5),
                connect: Duration::from_secs(5),
            },
        );
        // HTTP 绝对形式：解析器看到完整 URL。
        let via = connect(
            relay_port,
            &format!("GET http://127.0.0.1:{proxy_port}/x HTTP/1.1\r\nHost: 127.0.0.1:{proxy_port}\r\n\r\n"),
        );
        assert!(via.contains("proxied:P"), "response: {via:?}");
        // CONNECT：解析器看到合成的 https://host:port/。
        let direct = connect(
            relay_port,
            &format!(
                "CONNECT 127.0.0.1:{direct_port} HTTP/1.1\r\nHost: 127.0.0.1:{direct_port}\r\n\r\n"
            ),
        );
        assert!(direct.starts_with("HTTP/1.1 200"), "response: {direct:?}");
        assert!(direct.contains("direct:D"), "response: {direct:?}");
        // 同一监听 socket 上，两条连接按各自目标选择了不同上游。
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            [format!("GET http://127.0.0.1:{proxy_port}/x")]
        );
    }

    /// `Unsupported` 是 fail-closed：系统启用了中继无法表达的路由时绝不改走直连。
    #[test]
    fn unsupported_route_returns_502_for_both_request_forms() {
        let (target_port, hits) = spawn_direct_target("D");
        let (relay_port, _shared) =
            relay_with(Upstream::Unsupported("PAC evaluation failed".into()));
        for request in [
            format!(
                "CONNECT 127.0.0.1:{target_port} HTTP/1.1\r\nHost: 127.0.0.1:{target_port}\r\n\r\n"
            ),
            format!("GET http://127.0.0.1:{target_port}/x HTTP/1.1\r\nHost: 127.0.0.1:{target_port}\r\n\r\n"),
        ] {
            let response = connect(relay_port, &request);
            assert!(response.starts_with("HTTP/1.1 502"), "response: {response:?}");
            assert!(
                !response.contains("direct:D"),
                "an unsupported route must not reach the target: {response:?}"
            );
        }
        assert_eq!(
            hits.load(Ordering::Acquire),
            0,
            "no connection may reach the target when the route is unsupported"
        );
    }

    #[test]
    fn binds_loopback_only() {
        let listener = bind_loopback().unwrap();
        let address = listener.local_addr().unwrap();
        assert!(
            address.ip().is_loopback(),
            "relay must bind a loopback address, got {address}"
        );
        assert_eq!(address.ip().to_string(), "127.0.0.1");
        assert_ne!(
            address.port(),
            0,
            "an ephemeral port must be resolved before use"
        );
    }

    #[test]
    fn start_global_is_idempotent_and_publishes_a_loopback_url() {
        let first = start_global().unwrap();
        let second = start_global().unwrap();
        assert_eq!(first, second);
        assert_eq!(global_port(), Some(first));
        assert_eq!(global_url().unwrap(), format!("http://127.0.0.1:{first}"));
    }

    #[test]
    fn connect_direct_reaches_the_target() {
        let (target_port, _target_hits) = spawn_direct_target("A");
        let (relay_port, _shared) = relay_with(Upstream::Direct);
        let response = connect(
            relay_port,
            &format!(
                "CONNECT 127.0.0.1:{target_port} HTTP/1.1\r\nHost: 127.0.0.1:{target_port}\r\n\r\n"
            ),
        );
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "response: {response:?}"
        );
        assert!(response.contains("direct:A"), "response: {response:?}");
    }

    #[test]
    fn connect_via_upstream_proxy_uses_the_proxy() {
        let (target_port, target_hits) = spawn_direct_target("B");
        let (proxy_port, proxy_hits, proxy_seen) = spawn_upstream_proxy("B");
        let (relay_port, _shared) =
            relay_with(Upstream::ViaProxy(format!("http://127.0.0.1:{proxy_port}")));
        let response = connect(
            relay_port,
            &format!(
                "CONNECT 127.0.0.1:{target_port} HTTP/1.1\r\nHost: 127.0.0.1:{target_port}\r\n\r\n"
            ),
        );
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "response: {response:?}"
        );
        assert!(response.contains("proxied:B"), "response: {response:?}");
        assert_eq!(proxy_hits.load(Ordering::SeqCst), 1);
        assert_eq!(
            target_hits.load(Ordering::SeqCst),
            0,
            "target must not be dialled directly"
        );
        assert_eq!(
            proxy_seen.lock().unwrap().as_slice(),
            [format!("CONNECT 127.0.0.1:{target_port}")]
        );
    }

    #[test]
    fn upstream_switch_takes_effect_on_the_same_listener() {
        let (target_port, target_hits) = spawn_direct_target("SWITCH");
        let (proxy_port, proxy_hits, _seen) = spawn_upstream_proxy("SWITCH");
        let (relay_port, shared) = relay_with(Upstream::Direct);
        let authority = format!("127.0.0.1:{target_port}");
        let request = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n\r\n");

        let first = connect(relay_port, &request);
        assert!(first.contains("direct:SWITCH"), "response: {first}");
        assert_eq!(target_hits.load(Ordering::SeqCst), 1);

        // 监听 socket 与端口保持不变，只换上游。
        *shared.write().unwrap() = Upstream::ViaProxy(format!("http://127.0.0.1:{proxy_port}"));
        let second = connect(relay_port, &request);
        assert!(second.contains("proxied:SWITCH"), "response: {second}");
        assert_eq!(
            proxy_hits.load(Ordering::SeqCst),
            1,
            "new connection must use the new upstream"
        );
        assert_eq!(
            target_hits.load(Ordering::SeqCst),
            1,
            "target must not be dialled after the switch"
        );
    }

    #[test]
    fn unreachable_upstream_returns_502_without_direct_fallback() {
        // 占一个端口再释放，保证该端口上没有任何监听者。
        let closed_port = {
            let listener = TcpListener::bind((LOOPBACK, 0)).unwrap();
            listener.local_addr().unwrap().port()
        };
        let (target_port, target_hits) = spawn_direct_target("LEAK");
        let (relay_port, _shared) = relay_with(Upstream::ViaProxy(format!(
            "http://127.0.0.1:{closed_port}"
        )));
        let response = connect(
            relay_port,
            &format!(
                "CONNECT 127.0.0.1:{target_port} HTTP/1.1\r\nHost: 127.0.0.1:{target_port}\r\n\r\n"
            ),
        );
        assert!(
            response.starts_with("HTTP/1.1 502"),
            "response: {response:?}"
        );
        assert_eq!(
            target_hits.load(Ordering::SeqCst),
            0,
            "an unreachable upstream must never fall back to a direct connection"
        );
    }

    #[test]
    fn credentialed_or_non_http_upstream_is_rejected() {
        let (relay_port, _shared) =
            relay_with(Upstream::ViaProxy("http://user:secret@127.0.0.1:1".into()));
        let response = connect(
            relay_port,
            "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n",
        );
        assert!(
            response.starts_with("HTTP/1.1 502"),
            "response: {response:?}"
        );
        assert!(
            !response.contains("secret"),
            "credentials must never be echoed: {response}"
        );
        assert!(
            !response.contains("user:"),
            "credentials must never be echoed: {response}"
        );

        assert!(upstream_proxy_addr("http://user:secret@127.0.0.1:1").is_err());
        assert!(upstream_proxy_addr("https://127.0.0.1:1").is_err());
        assert!(upstream_proxy_addr("http://127.0.0.1").is_ok());
        // https 上游不由中继承载，调用方应保持原有注入行为。
        assert!(!Upstream::ViaProxy("https://proxy.local:8443".into()).is_relay_supported());
        assert!(Upstream::ViaProxy("http://proxy.local:8443".into()).is_relay_supported());
        assert!(Upstream::Direct.is_relay_supported());
    }

    #[test]
    fn plain_http_direct_is_rewritten_to_origin_form() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = seen.clone();
        let (origin_port, _hits) = spawn_server(move |mut stream| {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            if let Ok(head) = read_head(&mut reader) {
                recorder
                    .lock()
                    .unwrap()
                    .push(format!("{} {}", head.method, head.target));
            }
            let body = "origin-ok";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        });
        let (relay_port, _shared) = relay_with(Upstream::Direct);
        let response = connect(
            relay_port,
            &format!("GET http://127.0.0.1:{origin_port}/path?q=1 HTTP/1.1\r\nHost: 127.0.0.1:{origin_port}\r\n\r\n"),
        );
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "response: {response:?}"
        );
        assert!(response.ends_with("origin-ok"), "response: {response:?}");
        assert_eq!(seen.lock().unwrap().as_slice(), ["GET /path?q=1"]);
    }

    #[test]
    fn plain_http_via_upstream_proxy_keeps_absolute_form() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = seen.clone();
        let (proxy_port, _hits) = spawn_server(move |mut stream| {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            if let Ok(head) = read_head(&mut reader) {
                recorder
                    .lock()
                    .unwrap()
                    .push(format!("{} {}", head.method, head.target));
            }
            let body = "via-proxy-ok";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        });
        let (relay_port, _shared) =
            relay_with(Upstream::ViaProxy(format!("http://127.0.0.1:{proxy_port}")));
        let response = connect(
            relay_port,
            "GET http://example.test/thing HTTP/1.1\r\nHost: example.test\r\n\r\n",
        );
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "response: {response:?}"
        );
        assert!(response.ends_with("via-proxy-ok"), "response: {response:?}");
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            ["GET http://example.test/thing"]
        );
    }

    /// 测试专用的严格 HTTP body 解析：既接受拦截器合法地重分块/改用长度头，
    /// 又要求读足声明的长度、chunk 终止块与 trailer 结束空行，不把截断当作成功。
    #[derive(Debug)]
    struct ReceivedBody {
        bytes: Vec<u8>,
        chunks: Vec<Vec<u8>>,
        trailers: Vec<String>,
    }

    fn receive_body<R: BufRead>(reader: &mut R, framing: Framing) -> std::io::Result<ReceivedBody> {
        let mut received = ReceivedBody {
            bytes: Vec::new(),
            chunks: Vec::new(),
            trailers: Vec::new(),
        };
        match framing {
            Framing::None => {}
            Framing::Length(length) => {
                let mut limited = reader.take(length);
                limited.read_to_end(&mut received.bytes)?;
                if received.bytes.len() as u64 != length {
                    return Err(unexpected_eof());
                }
            }
            Framing::UntilClose => {
                reader.read_to_end(&mut received.bytes)?;
            }
            Framing::Chunked => loop {
                let line = read_line_bounded(reader, MAX_CHUNK_LINE)?.ok_or_else(unexpected_eof)?;
                let size = chunk_size(&line)?;
                if size == 0 {
                    loop {
                        let trailer = read_line_bounded(reader, MAX_CHUNK_LINE)?
                            .ok_or_else(unexpected_eof)?;
                        let trailer = trailer.trim_end_matches(['\r', '\n']);
                        if trailer.is_empty() {
                            return Ok(received);
                        }
                        received.trailers.push(trailer.to_owned());
                    }
                }
                // Fixtures are tiny; a corrupt length must fail instead of allocating
                // attacker-sized memory in the test runner.
                let size = usize::try_from(size)
                    .ok()
                    .filter(|size| *size <= 64 * 1024)
                    .ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "test chunk too large")
                    })?;
                let mut chunk = vec![0; size];
                reader.read_exact(&mut chunk)?;
                let mut ending = [0; 2];
                reader.read_exact(&mut ending)?;
                if ending != *b"\r\n" {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid chunk terminator",
                    ));
                }
                received.bytes.extend_from_slice(&chunk);
                received.chunks.push(chunk);
            },
        }
        Ok(received)
    }

    /// 分块转发必须**逐字节原样**（含块扩展与 trailer），并且**不能**先缓冲整个
    /// body——两条合起来才是「流式透传」，也是 SSE 这类响应不被退化的保证。
    ///
    /// 这里刻意用内存读写器、不走回环 socket：本机存在 HTTP 感知的外部组件，会在
    /// 回环上把 chunked 帧重写成单个块。对照实验（同一台机器、同样两条回环连接）：
    /// 非 chunked 的原始字节 `RAW\r\nZZ\r\nQQ\r\n` 逐字节不变，而 chunked 负载
    /// `4\r\nabcd\r\n5\r\nEFGHI\r\n0\r\n\r\n` 被读回为 `9\r\nabcdEFGHI\r\n0\r\n\r\n`
    /// ——**完全不经中继**的直连路径同样如此，所以 socket 级断言测不出中继自身的
    /// 行为，只有内存级断言才能锁住「原样转发」。
    #[test]
    fn chunked_frames_are_forwarded_verbatim_and_streamed() {
        use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

        /// 每次只喂几字节的读取器：模拟分片到达的 socket，并记录已消费字节数。
        struct Trickle {
            data: Vec<u8>,
            pos: usize,
            step: usize,
            consumed: Arc<AtomicUsize>,
        }
        impl Read for Trickle {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let end = (self.pos + self.step.min(buf.len())).min(self.data.len());
                if end == self.pos {
                    return Ok(0);
                }
                let length = end - self.pos;
                buf[..length].copy_from_slice(&self.data[self.pos..end]);
                self.pos = end;
                self.consumed.store(self.pos, AtomicOrdering::SeqCst);
                Ok(length)
            }
        }
        impl BufRead for Trickle {
            fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
                let end = (self.pos + self.step).min(self.data.len());
                Ok(&self.data[self.pos..end])
            }
            fn consume(&mut self, amount: usize) {
                self.pos = (self.pos + amount).min(self.data.len());
                self.consumed.store(self.pos, AtomicOrdering::SeqCst);
            }
        }

        /// 记录「第一次写出时读取器已消费多少字节」，用来证明没有整体缓冲。
        struct Probe {
            bytes: Vec<u8>,
            consumed: Arc<AtomicUsize>,
            consumed_at_first_write: Option<usize>,
        }
        impl Write for Probe {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                if self.consumed_at_first_write.is_none() {
                    self.consumed_at_first_write = Some(self.consumed.load(AtomicOrdering::SeqCst));
                }
                self.bytes.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let input = "4;ext=1\r\nabcd\r\n5\r\nEFGHI\r\n0\r\nX-Trailer: 1\r\n\r\n";
        let consumed = Arc::new(AtomicUsize::new(0));
        let mut reader = Trickle {
            data: input.as_bytes().to_vec(),
            pos: 0,
            step: 4,
            consumed: consumed.clone(),
        };
        let mut probe = Probe {
            bytes: Vec::new(),
            consumed,
            consumed_at_first_write: None,
        };
        forward_body(&mut reader, &mut probe, Framing::Chunked).expect("chunked body must forward");

        assert_eq!(
            String::from_utf8(probe.bytes).unwrap(),
            input,
            "chunk frames, extensions and trailers must survive verbatim"
        );
        let at_first_write = probe.consumed_at_first_write.expect("at least one write");
        assert!(
            at_first_write < input.len(),
            "the body must be streamed chunk by chunk, not buffered whole first \
             (first write happened after {at_first_write}/{} bytes)",
            input.len()
        );
    }

    /// 内存级响应路径：中继自身必须保持 chunk payload、终止块与 trailers，
    /// 不依赖回环网络上可能重写 framing 的 HTTP 感知拦截器。
    #[test]
    fn chunked_response_trailers_survive_relay_response() {
        let original = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\nX-Final: done\r\n\r\n";
        let mut forwarded = Vec::new();
        relay_response(&mut &original[..], &mut forwarded, "GET").unwrap();
        let mut reader = BufReader::new(forwarded.as_slice());
        let (status, headers) = read_head_raw(&mut reader).unwrap().unwrap();
        assert_eq!(status, 200);
        assert!(headers
            .iter()
            .any(|line| line.eq_ignore_ascii_case("Transfer-Encoding: chunked")));
        assert!(headers
            .iter()
            .any(|line| line.eq_ignore_ascii_case("Connection: close")));
        let body = receive_body(&mut reader, framing_of(&headers)).unwrap();
        assert_eq!(body.bytes, b"hello world");
        assert_eq!(body.chunks, [b"hello".to_vec(), b" world".to_vec()]);
        assert_eq!(body.trailers, ["X-Final: done"]);
    }

    /// 端到端断言语义，不假设回环上的 chunk 边界与源服务器相同；
    /// 上面的内存级用例另行保证中继本身逐字节转发并在末块前开始写出。
    #[test]
    fn chunked_response_body_is_relayed() {
        let (origin_port, _hits) = spawn_server(move |mut stream| {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let _ = read_head(&mut reader);
            let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: X-Final\r\nConnection: close\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\nX-Final: done\r\n\r\n";
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        });
        let (relay_port, _shared) = relay_with(Upstream::Direct);
        let response = connect(
            relay_port,
            &format!("GET http://127.0.0.1:{origin_port}/chunked HTTP/1.1\r\nHost: 127.0.0.1:{origin_port}\r\n\r\n"),
        );
        let mut reader = BufReader::new(response.as_bytes());
        let (status, headers) = read_head_raw(&mut reader)
            .unwrap()
            .expect("complete HTTP response head");
        assert_eq!(status, 200, "headers: {headers:?}");
        // Close-delimited is valid too if an HTTP-aware hop rewrites the framing;
        // the decoded payload and complete message are what matter on this socket.
        let framing = framing_of(&headers);
        let body = receive_body(&mut reader, framing)
            .expect("complete response body including final chunk");
        assert_eq!(body.bytes, b"hello world", "response: {response:?}");
        if framing == Framing::Chunked {
            assert!(
                !body.chunks.is_empty(),
                "nonempty chunked response must have data chunks"
            );
        }
        // HTTP-aware loopback filters may coalesce chunks or strip optional trailers.
        // A trailer that does arrive must still have the right meaning.
        for trailer in &body.trailers {
            assert_eq!(trailer, "X-Final: done", "unexpected trailer: {body:?}");
        }
        let mut extra = Vec::new();
        reader.read_to_end(&mut extra).unwrap();
        assert!(
            extra.is_empty(),
            "unexpected bytes after response body: {extra:?}"
        );
    }

    #[test]
    fn incomplete_chunked_messages_are_rejected_by_semantic_assertions() {
        for truncated in [
            "5\r\nhel",
            "5\r\nhello\r\n",
            "5\r\nhello\r\n0\r\nX-Final: done\r\n",
        ] {
            assert!(
                receive_body(&mut truncated.as_bytes(), Framing::Chunked).is_err(),
                "truncated chunked message must fail: {truncated:?}"
            );
        }
    }

    #[test]
    fn chunked_request_body_is_relayed() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = seen.clone();
        let (origin_port, _hits) = spawn_server(move |mut stream| {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let head = read_head(&mut reader).expect("origin receives request head");
            let framing = request_framing(&head.headers);
            let body =
                receive_body(&mut reader, framing).expect("origin receives complete request body");
            recorder.lock().unwrap().push((
                head.method,
                head.target,
                framing,
                body.bytes,
                body.trailers,
            ));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
                .unwrap();
            stream.flush().unwrap();
        });
        let (relay_port, _shared) = relay_with(Upstream::Direct);
        let response = connect(
            relay_port,
            &format!("POST http://127.0.0.1:{origin_port}/upload HTTP/1.1\r\nHost: 127.0.0.1:{origin_port}\r\nTransfer-Encoding: chunked\r\nTrailer: X-Final\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\nX-Final: done\r\n\r\n"),
        );
        let mut reader = BufReader::new(response.as_bytes());
        let (status, headers) = read_head_raw(&mut reader)
            .unwrap()
            .expect("complete HTTP response head");
        assert_eq!(status, 200, "response: {response:?}");
        assert_eq!(
            receive_body(&mut reader, framing_of(&headers))
                .unwrap()
                .bytes,
            b"OK"
        );
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "origin must see exactly one request");
        let (method, target, framing, body, trailers) = &seen[0];
        assert_eq!((method.as_str(), target.as_str()), ("POST", "/upload"));
        assert!(
            matches!(framing, Framing::Chunked | Framing::Length(_)),
            "request framing: {framing:?}"
        );
        assert_eq!(
            body, b"hello world",
            "origin must receive the entire decoded upload"
        );
        for trailer in trailers {
            assert_eq!(
                trailer, "X-Final: done",
                "unexpected request trailer: {trailers:?}"
            );
        }
    }

    #[test]
    fn malformed_and_oversized_requests_close_without_panic() {
        let (relay_port, _shared) = relay_with(Upstream::Direct);
        let malformed = connect(relay_port, "GARBAGE\r\n\r\n");
        assert!(
            malformed.starts_with("HTTP/1.1 400"),
            "response: {malformed}"
        );

        let oversized = connect(
            relay_port,
            &format!(
                "GET /{} HTTP/1.1\r\n\r\n",
                "a".repeat(MAX_REQUEST_LINE + 64)
            ),
        );
        assert!(
            oversized.starts_with("HTTP/1.1 400"),
            "response: {oversized}"
        );

        // 只发半截请求行也必须被超时收掉，而不是永久占用线程。
        let truncated = connect(relay_port, "GET http://example.test/");
        assert!(truncated.is_empty(), "unexpected response: {truncated}");
    }

    #[test]
    fn handshake_timeout_closes_idle_connections() {
        let listener = bind_loopback().unwrap();
        let port = listener.local_addr().unwrap().port();
        let shared = Arc::new(RwLock::new(Upstream::Direct));
        spawn_accept_loop(
            listener,
            move |_target: &str| shared.read().map(|guard| guard.clone()).unwrap_or_default(),
            Limits {
                handshake: Duration::from_millis(250),
                connect: Duration::from_millis(250),
            },
        );
        let mut stream = TcpStream::connect((LOOPBACK, port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let started = std::time::Instant::now();
        let mut buffer = [0u8; 16];
        let read = stream.read(&mut buffer).unwrap_or(0);
        assert_eq!(
            read, 0,
            "an idle connection must be closed by the handshake timeout"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timeout must not hang"
        );
    }

    #[test]
    fn frames_are_parsed_conservatively() {
        assert_eq!(
            parse_authority("example.com:443"),
            Some(("example.com".into(), 443))
        );
        assert_eq!(parse_authority("[::1]:8443"), Some(("::1".into(), 8443)));
        assert_eq!(parse_authority("example.com"), None);
        assert_eq!(parse_authority("example.com:0"), None);

        assert_eq!(
            absolute_form_target("http://example.com/a?b=1"),
            Some(("example.com".into(), 80, "/a?b=1".into()))
        );
        assert_eq!(absolute_form_target("https://example.com/a"), None);

        assert_eq!(chunk_size("1f\r\n").unwrap(), 31);
        assert!(chunk_size("zz\r\n").is_err());
    }
}
