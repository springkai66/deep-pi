//! Resolve the current user's Windows Internet proxy settings for each target URL.
//!
//! This module never consults HTTP_PROXY or the raw ProxyServer registry value: the
//! WinHTTP current-user API reports the *enabled*, active-connection settings. A
//! disabled proxy (including stale ProxyServer data) and a transparent TUN are Direct.
//! The caller must not turn Unsupported into Direct: an enabled PAC/WPAD failure is
//! fail-closed. Only the first PAC proxy is representable here; ordered proxy/DIRECT
//! failover, authenticated HTTP proxies, HTTPS-to-proxy TLS and SOCKS are unsupported.

use tauri::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Direct,
    /// Plain HTTP forward proxy; HTTPS destinations use CONNECT through it.
    HttpProxy(String),
    /// An enabled setting could not safely be represented or evaluated.
    Unsupported(String),
}

fn unsupported(reason: &'static str) -> Route {
    Route::Unsupported(reason.into())
}

/// Parse the destination rather than interpolating untrusted URL/userinfo into errors.
fn target(url: &str) -> Result<(String, String, u16), Route> {
    let parsed = Url::parse(url).map_err(|_| unsupported("invalid target URL"))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(unsupported("only HTTP(S) targets are supported"));
    }
    let host = parsed
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| unsupported("target URL has no host"))?
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| unsupported("target URL has no port"))?;
    if port == 0 {
        return Err(unsupported("target URL has invalid port"));
    }
    Ok((scheme.to_owned(), host, port))
}

/// Glob match for WinHTTP/Internet Settings bypass host patterns (case-insensitive).
fn wildcard_match(pattern: &str, host: &str) -> bool {
    let pattern = pattern.as_bytes();
    let host = host.as_bytes();
    let (mut p, mut h, mut star, mut retry) = (0, 0, None, 0);
    while h < host.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p].eq_ignore_ascii_case(&host[h])) {
            p += 1;
            h += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = h;
        } else if let Some(last_star) = star {
            p = last_star + 1;
            retry += 1;
            h = retry;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

/// ProxyOverride/WinHTTP bypass entries are delimited by semicolon or whitespace.
/// `<local>` means a host without a dot; explicit IPv6 and optional :port work too.
fn bypasses(host: &str, port: u16, bypass: &str) -> bool {
    bypass
        .split(|c: char| c == ';' || c.is_whitespace())
        .filter(|entry| !entry.is_empty())
        .any(|entry| {
            if entry.eq_ignore_ascii_case("<local>") {
                return !host.contains('.');
            }
            let (pattern, required_port) = if entry.starts_with('[') {
                match entry.split_once(']') {
                    Some((address, rest)) if rest.is_empty() => {
                        (address.trim_start_matches('['), None)
                    }
                    Some((address, rest)) if rest.starts_with(':') => {
                        match rest[1..].parse::<u16>() {
                            Ok(port) => (address.trim_start_matches('['), Some(port)),
                            Err(_) => return false,
                        }
                    }
                    _ => return false,
                }
            } else if entry.matches(':').count() == 1 {
                match entry.rsplit_once(':') {
                    Some((name, number)) => match number.parse::<u16>() {
                        Ok(port) => (name, Some(port)),
                        Err(_) => return false,
                    },
                    None => return false,
                }
            } else {
                (entry, None)
            };
            required_port.is_none_or(|required| required == port) && wildcard_match(pattern, host)
        })
}

/// Windows' `http=...;https=...` entries apply only to their own URL scheme.
/// An unqualified entry applies to both; never reuse `https=` for HTTP or vice versa.
fn select_static_entry<'a>(raw: &'a str, scheme: &str) -> Option<&'a str> {
    let mut default = None;
    for entry in raw
        .split(|c: char| c == ';' || c.is_whitespace())
        .filter(|s| !s.is_empty())
    {
        if let Some((kind, value)) = entry.split_once('=') {
            if kind.eq_ignore_ascii_case(scheme) {
                return Some(value);
            }
        } else if default.is_none() {
            default = Some(entry);
        }
    }
    default
}

fn parse_http_proxy(endpoint: &str) -> Route {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() || endpoint.contains([';', ',', ' ', '\t', '\r', '\n']) {
        return unsupported("invalid proxy endpoint");
    }
    // `https=` specifies the *destination* scheme, not HTTPS transport to the
    // proxy. An actual https:// upstream needs TLS support in the caller.
    if endpoint.to_ascii_lowercase().starts_with("https://") {
        return unsupported("HTTPS proxy upstream is unsupported");
    }
    let candidate = if endpoint.contains("://") {
        endpoint.to_owned()
    } else {
        format!("http://{endpoint}")
    };
    let Ok(url) = Url::parse(&candidate) else {
        return unsupported("invalid proxy endpoint");
    };
    if url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return unsupported("unsupported proxy endpoint or authentication");
    }
    let Some(host) = url.host_str() else {
        return unsupported("proxy endpoint has no host");
    };
    let Some(port) = url.port_or_known_default().filter(|port| *port != 0) else {
        return unsupported("proxy endpoint has invalid port");
    };
    let host = host.trim_matches(['[', ']']);
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    Route::HttpProxy(format!("http://{host}:{port}"))
}

fn static_route(raw: &str, bypass: &str, scheme: &str, host: &str, port: u16) -> Route {
    let Some(endpoint) = select_static_entry(raw, scheme) else {
        // For example, a configured `https=` proxy does not apply to HTTP.
        return Route::Direct;
    };
    if bypasses(host, port, bypass) {
        Route::Direct
    } else {
        parse_http_proxy(endpoint)
    }
}

/// WinHttpGetProxyForUrl returns WinHTTP_PROXY_INFO, not the raw PAC script.
/// Named results can include an ordered list; only its first choice is supported.
/// Never silently advance to `DIRECT` after an unsupported or failed first proxy.
fn dynamic_route(raw: &str, bypass: &str, scheme: &str, host: &str, port: u16) -> Route {
    let first = raw.split([';', ',']).next().unwrap_or("").trim();
    if first.eq_ignore_ascii_case("DIRECT") {
        return Route::Direct;
    }
    if first.is_empty() {
        return unsupported("PAC returned no usable proxy");
    }
    if bypasses(host, port, bypass) {
        return Route::Direct;
    }
    let mut words = first.split_whitespace();
    let first_word = words.next().unwrap_or("");
    let endpoint =
        if first_word.eq_ignore_ascii_case("PROXY") || first_word.eq_ignore_ascii_case("HTTP") {
            match (words.next(), words.next()) {
                (Some(endpoint), None) => endpoint,
                _ => return unsupported("invalid PAC proxy result"),
            }
        } else if first_word.eq_ignore_ascii_case("HTTPS") {
            return unsupported("HTTPS proxy upstream is unsupported");
        } else if first_word.to_ascii_uppercase().starts_with("SOCKS") {
            return unsupported("SOCKS proxy upstream is unsupported");
        } else if words.next().is_some() {
            return unsupported("invalid PAC proxy result");
        } else {
            first_word
        };
    // WinHTTP can also return a scheme-qualified proxy list.
    if endpoint.contains('=') {
        return match select_static_entry(first, scheme) {
            Some(endpoint) => parse_http_proxy(endpoint),
            None => unsupported("PAC returned no proxy for target scheme"),
        };
    }
    parse_http_proxy(endpoint)
}

/// Re-read current settings for every URL. No process-environment proxy fallback.
#[cfg(windows)]
pub(crate) fn resolve(url: &str) -> Route {
    let (scheme, host, port) = match target(url) {
        Ok(target) => target,
        Err(error) => return error,
    };
    let config = match windows::ie_config() {
        Ok(config) => config,
        Err(error) => return error,
    };
    let pac_url = match config.pac_url() {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Some(pac_url) = pac_url {
        return windows::evaluate_pac(url, Some(&pac_url), &scheme, &host, port);
    }
    if config.auto_detect() {
        return windows::evaluate_pac(url, None, &scheme, &host, port);
    }
    let static_proxy = match config.static_proxy() {
        Ok(value) => value,
        Err(error) => return error,
    };
    let Some(static_proxy) = static_proxy else {
        return Route::Direct;
    };
    let bypass = match config.bypass() {
        Ok(value) => value,
        Err(error) => return error,
    };
    static_route(
        &static_proxy,
        bypass.as_deref().unwrap_or(""),
        &scheme,
        &host,
        port,
    )
}

/// Conservatively returns true when Windows settings cannot be read; callers must
/// not assume a fixed proxy in that case. This does not consult environment vars.
#[cfg(windows)]
pub(crate) fn has_dynamic_proxy_config() -> bool {
    match windows::ie_config() {
        Ok(config) => config.auto_detect() || config.pac_url().map_or(true, |url| url.is_some()),
        Err(_) => true,
    }
}

/// Other platforms have no Windows Internet settings. In particular, do not use
/// inherited HTTP_PROXY as a substitute for system mode.
#[cfg(not(windows))]
pub(crate) fn resolve(url: &str) -> Route {
    match target(url) {
        Ok(_) => Route::Direct,
        Err(error) => error,
    }
}

#[cfg(not(windows))]
pub(crate) fn has_dynamic_proxy_config() -> bool {
    false
}

#[cfg(windows)]
mod windows {
    use super::{dynamic_route, unsupported, Route};
    use std::{ffi::c_void, ptr};
    use windows_sys::Win32::{
        Foundation::{GetLastError, GlobalFree},
        Networking::WinHttp::{
            WinHttpCloseHandle, WinHttpGetIEProxyConfigForCurrentUser, WinHttpGetProxyForUrl,
            WinHttpOpen, WINHTTP_ACCESS_TYPE_NAMED_PROXY, WINHTTP_ACCESS_TYPE_NO_PROXY,
            WINHTTP_AUTOPROXY_AUTO_DETECT, WINHTTP_AUTOPROXY_CONFIG_URL, WINHTTP_AUTOPROXY_OPTIONS,
            WINHTTP_AUTO_DETECT_TYPE_DHCP, WINHTTP_AUTO_DETECT_TYPE_DNS_A,
            WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WINHTTP_PROXY_INFO,
        },
    };

    // WinHTTP allocates each output separately with GlobalAlloc. Ownership of the
    // three config strings and the two result strings stays in these drop guards.
    pub(super) struct IeConfig(WINHTTP_CURRENT_USER_IE_PROXY_CONFIG);
    impl Drop for IeConfig {
        fn drop(&mut self) {
            unsafe {
                for string in [
                    self.0.lpszAutoConfigUrl,
                    self.0.lpszProxy,
                    self.0.lpszProxyBypass,
                ] {
                    if !string.is_null() {
                        GlobalFree(string.cast::<c_void>());
                    }
                }
            }
        }
    }

    struct ProxyInfo(WINHTTP_PROXY_INFO);
    impl Drop for ProxyInfo {
        fn drop(&mut self) {
            unsafe {
                for string in [self.0.lpszProxy, self.0.lpszProxyBypass] {
                    if !string.is_null() {
                        GlobalFree(string.cast::<c_void>());
                    }
                }
            }
        }
    }

    struct Session(*mut c_void);
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }

    fn wide_string(ptr: *const u16) -> Result<Option<String>, Route> {
        if ptr.is_null() {
            return Ok(None);
        }
        let mut length = 0;
        unsafe {
            while length < 65536 && *ptr.add(length) != 0 {
                length += 1;
            }
            if length == 65536 {
                return Err(unsupported("Windows proxy setting is too long"));
            }
            String::from_utf16(std::slice::from_raw_parts(ptr, length))
                .map(Some)
                .map_err(|_| unsupported("Windows proxy setting is invalid UTF-16"))
        }
    }

    impl IeConfig {
        pub(super) fn auto_detect(&self) -> bool {
            self.0.fAutoDetect != 0
        }
        pub(super) fn pac_url(&self) -> Result<Option<String>, Route> {
            wide_string(self.0.lpszAutoConfigUrl)
        }
        pub(super) fn static_proxy(&self) -> Result<Option<String>, Route> {
            wide_string(self.0.lpszProxy)
        }
        pub(super) fn bypass(&self) -> Result<Option<String>, Route> {
            wide_string(self.0.lpszProxyBypass)
        }
    }

    pub(super) fn ie_config() -> Result<IeConfig, Route> {
        let mut config = IeConfig(WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default());
        // SAFETY: valid writable struct; IeConfig frees output even on partial failure.
        if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut config.0) } == 0 {
            let code = unsafe { GetLastError() };
            return Err(Route::Unsupported(format!(
                "Windows proxy settings unavailable (error {code})"
            )));
        }
        Ok(config)
    }

    pub(super) fn evaluate_pac(
        url: &str,
        pac_url: Option<&str>,
        scheme: &str,
        host: &str,
        port: u16,
    ) -> Route {
        let target_wide: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
        let pac_wide: Option<Vec<u16>> =
            pac_url.map(|url| url.encode_utf16().chain(Some(0)).collect());
        // WinHTTP uses null-terminated UTF-16; reject embedded NUL rather than
        // inadvertently querying a truncated (different) URL or PAC script.
        if url.contains('\0') || pac_url.is_some_and(|value| value.contains('\0')) {
            return unsupported("invalid target or PAC URL");
        }
        let mut options = WINHTTP_AUTOPROXY_OPTIONS {
            dwFlags: if pac_url.is_some() {
                WINHTTP_AUTOPROXY_CONFIG_URL
            } else {
                WINHTTP_AUTOPROXY_AUTO_DETECT
            },
            dwAutoDetectFlags: if pac_url.is_none() {
                WINHTTP_AUTO_DETECT_TYPE_DHCP | WINHTTP_AUTO_DETECT_TYPE_DNS_A
            } else {
                0
            },
            lpszAutoConfigUrl: pac_wide
                .as_ref()
                .map_or(ptr::null(), |value| value.as_ptr()),
            lpvReserved: ptr::null_mut(),
            dwReserved: 0,
            fAutoLogonIfChallenged: 0,
        };
        // NO_PROXY is deliberate: avoid inheriting any machine-wide WinHTTP
        // proxy while fetching/evaluating this user's PAC settings.
        let handle = unsafe {
            WinHttpOpen(
                ptr::null(),
                WINHTTP_ACCESS_TYPE_NO_PROXY,
                ptr::null(),
                ptr::null(),
                0,
            )
        };
        if handle.is_null() {
            let code = unsafe { GetLastError() };
            return Route::Unsupported(format!("Windows PAC session unavailable (error {code})"));
        }
        let session = Session(handle);
        let mut info = ProxyInfo(WINHTTP_PROXY_INFO::default());
        let success = unsafe {
            WinHttpGetProxyForUrl(session.0, target_wide.as_ptr(), &mut options, &mut info.0)
        };
        if success == 0 {
            let code = unsafe { GetLastError() };
            return Route::Unsupported(format!("Windows PAC evaluation failed (error {code})"));
        }
        match info.0.dwAccessType {
            WINHTTP_ACCESS_TYPE_NO_PROXY => Route::Direct,
            WINHTTP_ACCESS_TYPE_NAMED_PROXY => {
                let proxy = match wide_string(info.0.lpszProxy) {
                    Ok(Some(proxy)) => proxy,
                    Ok(None) => return unsupported("PAC returned no proxy address"),
                    Err(error) => return error,
                };
                let bypass = match wide_string(info.0.lpszProxyBypass) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                dynamic_route(&proxy, bypass.as_deref().unwrap_or(""), scheme, host, port)
            }
            _ => unsupported("PAC returned an unknown access type"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_specific_static_proxy_never_reuses_https_for_http() {
        let raw = "http=proxy-a.test:8080;https=proxy-b.test:8443";
        assert_eq!(
            static_route(raw, "", "http", "example.com", 80),
            Route::HttpProxy("http://proxy-a.test:8080".into())
        );
        assert_eq!(
            static_route(raw, "", "https", "example.com", 443),
            Route::HttpProxy("http://proxy-b.test:8443".into())
        );
        assert_eq!(
            static_route("https=proxy.test:443", "", "http", "example.com", 80),
            Route::Direct
        );
        assert_eq!(
            static_route("proxy.test:8080", "", "https", "example.com", 443),
            Route::HttpProxy("http://proxy.test:8080".into())
        );
    }

    #[test]
    fn bypass_patterns_cover_local_wildcards_ports_and_ipv6() {
        let bypass = "<local>;*.corp.example;api.example:8443;[::1]";
        assert!(bypasses("intranet", 80, bypass));
        assert!(bypasses("DEV.CORP.EXAMPLE", 443, bypass));
        assert!(!bypasses("corp.example", 443, "*.corp.example"));
        assert!(!bypasses("badcorp.example", 443, "*.corp.example"));
        assert!(bypasses("api.example", 8443, bypass));
        assert!(!bypasses("api.example", 80, bypass));
        assert!(bypasses("::1", 80, bypass));
        assert!(bypasses("anything", 80, "*"));
        assert_eq!(
            static_route("proxy:80", bypass, "https", "DEV.CORP.EXAMPLE", 443),
            Route::Direct
        );
    }

    #[test]
    fn pac_first_choice_does_not_fall_back_silently() {
        assert_eq!(
            dynamic_route("DIRECT; PROXY proxy:80", "", "https", "example.com", 443),
            Route::Direct
        );
        assert_eq!(
            dynamic_route("PROXY proxy:80; DIRECT", "", "https", "example.com", 443),
            Route::HttpProxy("http://proxy:80".into())
        );
        assert!(matches!(
            dynamic_route("SOCKS socks:1080; DIRECT", "", "https", "example.com", 443),
            Route::Unsupported(_)
        ));
        assert!(matches!(
            dynamic_route("HTTPS proxy:443; DIRECT", "", "https", "example.com", 443),
            Route::Unsupported(_)
        ));
        assert!(matches!(
            dynamic_route("", "", "https", "example.com", 443),
            Route::Unsupported(_)
        ));
    }

    #[test]
    fn proxy_credentials_and_https_upstream_are_rejected() {
        for endpoint in [
            "https://proxy:443",
            "http://user:secret@proxy:80",
            "socks5://proxy:1080",
            "proxy:0",
            "proxy:bad",
            "proxy:80/path",
        ] {
            assert!(matches!(parse_http_proxy(endpoint), Route::Unsupported(_)));
        }
        assert_eq!(
            parse_http_proxy("[::1]:7890"),
            Route::HttpProxy("http://[::1]:7890".into())
        );
    }

    #[test]
    fn invalid_targets_cannot_become_direct_or_leak_userinfo() {
        assert!(matches!(
            target("file:///secret"),
            Err(Route::Unsupported(_))
        ));
        assert!(matches!(target("https://"), Err(Route::Unsupported(_))));
        assert_eq!(
            target("https://user:secret@[::1]/").unwrap(),
            ("https".into(), "::1".into(), 443)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_stub_does_not_read_environment_proxy() {
        assert_eq!(resolve("https://example.com"), Route::Direct);
        assert!(!has_dynamic_proxy_config());
    }
}
