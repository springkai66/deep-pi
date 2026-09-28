# Security Notes

DeepPi is a Windows-first local desktop application. The security boundary is the Tauri command layer and the child processes it starts.

## Threat Model

| Boundary | Main abuse case | Control |
|---|---|---|
| Frontend to Tauri commands | Malformed paths, package specs, provider URLs, or settings | Rust boundary validation, allowlists, size limits, parameterized SQLite queries |
| DeepPi to Provider | SSRF, credential disclosure, hostile model JSON | HTTPS/loopback URL rules, proxy rules, Credential Manager, bounded JSON parsing |
| DeepPi to Pi Package registry | Command injection or package metadata abuse | Exact npm version confirmation, package-source validation, no shell interpolation, CI install scripts disabled |
| DeepPi to Pi/DSH child process | Secret inheritance or unexpected process access | Minimal environment variables, no key in arguments/logs, managed process paths, activity checks before runtime swap |
| Main window to DSH WebView | LAN exposure or navigation escape | DSH binds to `127.0.0.1`, dynamic port, active-origin allowlist, external opener for other origins |
| Loopback proxy relay | Local port abuse, tunnelled credential leakage, silent proxy bypass | `127.0.0.1`-only bind, bounded request head with handshake timeout, no SOCKS/PAC/auth, explicit `502` instead of a direct fallback |
| Runtime update source | Malicious or partial update replacing the active runtime | HTTPS registry, exact version validation, `--ignore-scripts`, staging verification, atomic swap, rollback backup |
| DeepPi self updater | Unsigned installer or downgrade from an untrusted endpoint | Tauri updater public-key verification, HTTPS-only `latest.json`, signed installer artifacts, Release secret gates |

## STRIDE Review

| Boundary | S | T | R | I | D | E |
|---|---|---|---|---|---|---|
| Frontend to Tauri | forged command payload | settings/package mutation | lifecycle logs | error and path disclosure | bounded input and timeouts | typed commands and state checks |
| Provider and registry HTTP | hostile endpoint identity | HTTPS and schema validation | structured request logs | credential and response limits | 15s timeout and size caps | allowlisted URL/proxy rules |
| Pi/DSH child processes | inherited identity and environment | runtime replacement | process/update events | no secrets in args/logs | mutexes and idle checks | managed paths and explicit stop |
| DSH WebView | navigation impersonation | hostile page navigation | host status events | local data exposure | loopback and bounded WebView | active-origin allowlist |
| Package and runtime install | untrusted package source | staged atomic activation | install/rollback logs | bounded child output | retries, timeouts, no lifecycle scripts | exact versions and compatibility checks |

The review is intentionally tied to the runtime boundaries above. Any new command, external URL, child process, package source, or credential path must add a validation rule and a focused regression test before release.

## Controls

- Provider API keys are stored in Windows Credential Manager. `models.json`, logs, and frontend state never contain the key.
- Provider URLs accept HTTPS or loopback HTTP only. Query strings, URL credentials, invalid headers, and unsafe proxy URLs are rejected.
- External model, package, and update responses are size-limited and parsed as data before use.
- Pi package specs are validated before they reach the managed Pi CLI. Package operations use a global mutex to prevent concurrent settings writes.
- DSH starts on `127.0.0.1` with a dynamic port. Its child Webview only allows the active loopback origin; other navigation is sent to the system opener.
- The WebView CSP allows only the application origin, Tauri IPC, and declared local resources.
- Pi bridge status uses a task-scoped Windows Named Pipe with remote clients rejected; the state-file fallback contains only `running`/`waiting` and is bounded to the per-user temp directory.
- Lifecycle scripts are disabled for CI dependency installation, and production dependency audit and license validation are required CI steps.
- Runtime update requests honor only validated `HTTPS_PROXY`/`HTTP_PROXY` environment proxies; invalid proxy URLs fail closed.
- DeepPi runs a loopback-only proxy relay so child processes keep a constant proxy address while the proxy mode changes underneath them. It binds `127.0.0.1` on an ephemeral port, bounds the request line and header block, times out the handshake, rejects upstream URLs carrying credentials, never forwards `Proxy-Authorization` or any other proxy credential header, and answers `502` when the upstream is unreachable instead of falling back to a direct connection.
- DeepPi self updates use the Tauri updater signature embedded in `latest.json`; development builds without a release overlay cannot install an update.

## Checks

```powershell
pnpm install --frozen-lockfile --ignore-scripts
pnpm security:audit
pnpm licenses:check
pnpm release:check
pnpm check
pnpm test
pnpm build
```

Rust checks run with `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo audit`.

CI also runs Gitleaks against repository history and disables dependency lifecycle scripts during install.

## Known Boundaries

OAuth and subscription login remain inside Pi's native `/login` flow. Runtime replacement and Windows code signing require a signed release source and certificate managed outside this repository. DeepPi self-updates additionally require the Tauri updater private key, public key, and HTTPS `latest.json`; update checks do not install unsigned artifacts.
