use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    agent_config::{self, ConfigScope},
    app_paths::AppPaths,
    process_runner::{self, ProcessOutput},
};

const LIST_TIMEOUT: Duration = Duration::from_secs(180);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(310);
const MCP_LIST_CAPTURE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpScopeRequest {
    pub scope: ConfigScope,
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerActionRequest {
    pub name: String,
    pub scope: ConfigScope,
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiMcpServerStatus {
    pub name: String,
    pub scope: String,
    pub source: String,
    pub enabled: bool,
    pub exposure: String,
    pub transport: String,
    pub state: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub tool_exposure: Option<serde_json::Value>,
    #[serde(default)]
    pub resources: Option<usize>,
    #[serde(default)]
    pub resource_templates: Option<usize>,
    #[serde(default)]
    pub error: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiMcpListResult {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub servers: Vec<PiMcpServerStatus>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub note: Option<String>,
}

struct TemporaryWorkingDirectory(PathBuf);

impl TemporaryWorkingDirectory {
    fn create() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("deeppi-mcp-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path)
            .map_err(|error| format!("could not create temporary Pi working directory: {error}"))?;
        Ok(Self(path))
    }
}

impl Drop for TemporaryWorkingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct WorkingDirectory {
    path: PathBuf,
    _temporary: Option<TemporaryWorkingDirectory>,
}

fn working_directory(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
) -> Result<WorkingDirectory, String> {
    if scope == ConfigScope::Project {
        let pi_dir = agent_config::scope_pi_dir(paths, scope, project_path)?;
        let project = pi_dir
            .parent()
            .ok_or_else(|| "project Pi configuration has no parent directory".to_string())?;
        return Ok(WorkingDirectory {
            path: project.to_owned(),
            _temporary: None,
        });
    }
    let temporary = TemporaryWorkingDirectory::create()?;
    Ok(WorkingDirectory {
        path: temporary.0.clone(),
        _temporary: Some(temporary),
    })
}

fn managed_cli(paths: &AppPaths, cwd: &Path, args: &[String]) -> Result<ProcessOutput, String> {
    let Some(cli) = paths.pi_cli()? else {
        return Err(crate::message::msg("managed.pi_missing"));
    };
    let node = paths.node_runtime()?;
    let mut command = Command::new(node);
    command
        .arg(cli)
        .arg("mcp")
        .args(args)
        .current_dir(cwd)
        .env("PI_CODING_AGENT_DIR", &paths.pi_home)
        .env("PI_TELEMETRY", "0");
    crate::proxy::apply_to_command(&mut command);
    let timeout = if args.first().is_some_and(|arg| arg == "list") {
        LIST_TIMEOUT
    } else if args.first().is_some_and(|arg| arg == "login") {
        LOGIN_TIMEOUT
    } else {
        COMMAND_TIMEOUT
    };
    if args.first().is_some_and(|arg| arg == "list") {
        process_runner::run_cancellable_with_capture_limit(
            &mut command,
            timeout,
            None,
            MCP_LIST_CAPTURE_BYTES,
        )
    } else {
        process_runner::run_cancellable(&mut command, timeout, None)
    }
}

fn output_error(output: &ProcessOutput) -> String {
    let message = output.text();
    let redacted = crate::pi_auth::redact(&message);
    if redacted.trim().is_empty() {
        format!("Pi MCP command failed with status {}", output.status)
    } else {
        redacted
    }
}

fn cli_available(paths: &AppPaths) -> Result<bool, String> {
    Ok(paths.pi_cli()?.is_some() && paths.node_runtime().is_ok())
}

/// Return MCP server connection state from Pi's first-party `pi mcp list --json` command.
/// This checks the same managed Pi runtime/configuration that runs user sessions.
fn list_servers(paths: &AppPaths, request: McpScopeRequest) -> Result<PiMcpListResult, String> {
    if !cli_available(paths)? {
        return Ok(PiMcpListResult {
            available: false,
            servers: Vec::new(),
            errors: Vec::new(),
            note: Some("Install the managed Pi runtime to check MCP server connections.".into()),
        });
    }
    let cwd = working_directory(paths, request.scope, request.project_path.as_deref())?;
    let output = managed_cli(paths, &cwd.path, &["list".into(), "--json".into()])?;
    if output.truncated {
        return Err("Pi MCP status output exceeded the capture limit".into());
    }
    let mut result: PiMcpListResult = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "Pi MCP status returned invalid JSON: {error}; {}",
            output_error(&output)
        )
    })?;
    result.available = true;
    result.servers.retain(|server| match request.scope {
        ConfigScope::Global => server.scope == "global",
        ConfigScope::Project => server.scope == "project",
    });
    for error in &mut result.errors {
        *error = crate::pi_auth::redact(error);
    }
    if let Some(note) = result.note.as_mut() {
        *note = crate::pi_auth::redact(note);
    }
    for server in &mut result.servers {
        if let Some(error) = server.error.as_mut() {
            *error = crate::pi_auth::redact(error);
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn pi_mcp_list_servers(
    paths: State<'_, AppPaths>,
    request: McpScopeRequest,
) -> Result<PiMcpListResult, String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || list_servers(&paths, request))
        .await
        .map_err(|error| format!("Pi MCP status worker failed: {error}"))?
}

fn valid_env_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Only accept a complete environment-variable reference, never a partial template or command.
fn is_reference(value: &str) -> bool {
    let name = if value.starts_with("${") && value.ends_with('}') {
        value.get(2..value.len() - 1)
    } else if value.starts_with('$') {
        value.get(1..)
    } else {
        None
    };
    name.is_some_and(valid_env_name)
}

fn is_bearer_reference(value: &str) -> bool {
    value
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Bearer "))
        && value.get(7..).is_some_and(is_reference)
}

fn has_credential_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "token",
        "secret",
        "api-key",
        "api_key",
        "apikey",
        "access-key",
        "access_key",
        "password",
        "passwd",
        "credential",
        "authorization",
        "bearer",
        "private-key",
        "private_key",
        "jwt",
        "cookie",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || ["ghp_", "github_pat_", "xoxb-", "xoxp-", "glpat-", "sk-"]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        || {
            let segments = value.split('.').collect::<Vec<_>>();
            segments.len() == 3 && segments[0].starts_with("eyJ")
        }
}

fn cli_safe_http_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
        || url.contains('?')
        || url.contains('#')
        || has_credential_marker(url)
    {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !authority.is_empty() && !authority.contains('@')
}

fn sensitive_header_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let sensitive_markers = [
        "auth",
        "token",
        "secret",
        "api-key",
        "api_key",
        "apikey",
        "key",
        "credential",
        "password",
        "passwd",
        "cookie",
        "session",
        "signature",
        "jwt",
        "private",
        "access",
    ];
    sensitive_markers
        .iter()
        .any(|marker| lower.contains(marker))
}

fn safe_cli_header_value(name: &str, value: &str) -> bool {
    if is_reference(value)
        || (name.eq_ignore_ascii_case("authorization") && is_bearer_reference(value))
    {
        return true;
    }
    if sensitive_header_name(name) || has_credential_marker(value) {
        return false;
    }
    matches!(
        name.to_ascii_lowercase().as_str(),
        "accept"
            | "content-type"
            | "user-agent"
            | "cache-control"
            | "pragma"
            | "accept-encoding"
            | "accept-language"
    ) && value.len() <= 256
}

fn only_keys(object: &serde_json::Map<String, serde_json::Value>, allowed: &[&str]) -> bool {
    object.keys().all(|key| allowed.contains(&key.as_str()))
}

/// Build arguments for the official CLI when it can represent the exact config. Configurations
/// outside its current add-flag surface (for example per-tool exposure or raw secrets in argv) are
/// written to Pi's documented mcp.json format instead.
fn native_add_args(
    name: &str,
    scope: ConfigScope,
    config: &serde_json::Value,
) -> Option<Vec<String>> {
    let object = config.as_object()?;
    if !valid_pi_name(name) || object.get("enabled").is_some_and(|enabled| enabled != true) {
        return None;
    }
    let mut args = vec!["add".to_owned(), name.to_owned()];
    if scope == ConfigScope::Project {
        args.push("--local".into());
    }
    if let Some(exposure) = object.get("exposure") {
        args.extend(["--exposure".into(), exposure.as_str()?.to_owned()]);
    }
    match object.get("url").and_then(serde_json::Value::as_str) {
        Some(url) => {
            if !cli_safe_http_url(url)
                || !only_keys(
                    object,
                    &["url", "type", "headers", "oauth", "exposure", "enabled"],
                )
                || object
                    .get("type")
                    .is_some_and(|kind| !matches!(kind.as_str(), Some("http" | "streamable-http")))
            {
                return None;
            }
            args.extend(["--url".into(), url.into()]);
            if let Some(headers) = object.get("headers") {
                let headers = headers.as_object()?;
                for (key, value) in headers {
                    let value = value.as_str()?;
                    if !safe_cli_header_value(key, value) {
                        return None;
                    }
                    args.extend(["--header".into(), format!("{key}={value}")]);
                }
            }
            if let Some(oauth) = object.get("oauth") {
                let oauth = oauth.as_object()?;
                if !only_keys(oauth, &["clientId", "clientSecret", "callbackPort"])
                    || oauth
                        .get("clientSecret")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|secret| !is_reference(secret))
                {
                    return None;
                }
                if let Some(client_id) = oauth.get("clientId") {
                    let client_id = client_id.as_str()?;
                    if has_credential_marker(client_id) {
                        return None;
                    }
                    args.extend(["--oauth-client-id".into(), client_id.into()]);
                }
                if let Some(client_secret) = oauth.get("clientSecret") {
                    args.extend([
                        "--oauth-client-secret".into(),
                        client_secret.as_str()?.into(),
                    ]);
                }
                if let Some(port) = oauth.get("callbackPort") {
                    args.extend(["--oauth-callback-port".into(), port.as_u64()?.to_string()]);
                }
            }
        }
        None => {
            if !only_keys(
                object,
                &[
                    "command", "args", "env", "cwd", "type", "exposure", "enabled",
                ],
            ) || object.get("type").is_some_and(|kind| kind != "stdio")
            {
                return None;
            }
            let command = object.get("command")?.as_str()?;
            if let Some(env) = object.get("env") {
                for (key, value) in env.as_object()? {
                    let value = value.as_str()?;
                    if !valid_env_name(key) || !is_reference(value) {
                        return None;
                    }
                    args.extend(["--env".into(), format!("{key}={value}")]);
                }
            }
            if let Some(cwd) = object.get("cwd") {
                args.extend(["--cwd".into(), cwd.as_str()?.into()]);
            }
            args.push("--".into());
            args.push(command.into());
            if let Some(command_args) = object.get("args") {
                let command_args = command_args
                    .as_array()?
                    .iter()
                    .map(|argument| argument.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()?;
                if command_args
                    .iter()
                    .any(|argument| has_credential_marker(argument))
                {
                    return None;
                }
                args.extend(command_args);
            }
        }
    }
    Some(args)
}

fn valid_pi_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

pub(crate) fn save_server_config(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
    name: &str,
    config: &serde_json::Value,
) -> Result<(), String> {
    if !valid_pi_name(name) {
        return Err("MCP server names must use only ASCII letters, digits, '_' and '-'".into());
    }
    let Some(args) = native_add_args(name, scope, config) else {
        return agent_config::save_mcp_server_config(paths, scope, project_path, name, config);
    };
    if !cli_available(paths)? {
        return agent_config::save_mcp_server_config(paths, scope, project_path, name, config);
    }
    let cwd = working_directory(paths, scope, project_path)?;
    let output = managed_cli(paths, &cwd.path, &args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(output_error(&output))
    }
}

pub(crate) fn remove_server(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
    name: &str,
) -> Result<(), String> {
    if !valid_pi_name(name) {
        return Err("MCP server names must use only ASCII letters, digits, '_' and '-'".into());
    }
    if !cli_available(paths)? {
        return agent_config::delete_mcp_server_config(paths, scope, project_path, name);
    }
    let cwd = working_directory(paths, scope, project_path)?;
    let mut args = vec!["remove".to_owned(), name.to_owned()];
    if scope == ConfigScope::Project {
        args.push("--local".into());
    }
    let output = managed_cli(paths, &cwd.path, &args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(output_error(&output))
    }
}

fn run_auth_action(
    request: McpServerActionRequest,
    paths: &AppPaths,
    action: &str,
) -> Result<(), String> {
    if !valid_pi_name(&request.name) {
        return Err("MCP server names must use only ASCII letters, digits, '_' and '-'".into());
    }
    let cwd = working_directory(paths, request.scope, request.project_path.as_deref())?;
    let output = managed_cli(paths, &cwd.path, &[action.into(), request.name])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(output_error(&output))
    }
}

#[tauri::command]
pub async fn pi_mcp_login(
    paths: State<'_, AppPaths>,
    request: McpServerActionRequest,
) -> Result<(), String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run_auth_action(request, &paths, "login"))
        .await
        .map_err(|error| format!("Pi MCP login worker failed: {error}"))?
}

#[tauri::command]
pub async fn pi_mcp_logout(
    paths: State<'_, AppPaths>,
    request: McpServerActionRequest,
) -> Result<(), String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run_auth_action(request, &paths, "logout"))
        .await
        .map_err(|error| format!("Pi MCP logout worker failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::{native_add_args, valid_pi_name};
    use crate::agent_config::ConfigScope;
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn maps_remote_and_stdio_configs_to_pi_mcp_add_arguments() {
        assert_eq!(
            native_add_args(
                "docs",
                ConfigScope::Global,
                &json!({"url":"https://mcp.example.com/mcp","exposure":"direct","headers":{"Authorization":"Bearer ${DOCS_TOKEN}"}}),
            ),
            Some(vec![
                "add".into(),
                "docs".into(),
                "--exposure".into(),
                "direct".into(),
                "--url".into(),
                "https://mcp.example.com/mcp".into(),
                "--header".into(),
                "Authorization=Bearer ${DOCS_TOKEN}".into(),
            ]),
        );
        assert_eq!(
            native_add_args(
                "filesystem",
                ConfigScope::Project,
                &json!({"command":"npx","args":["-y","@modelcontextprotocol/server-filesystem","."],"cwd":"F:/project"}),
            ),
            Some(vec![
                "add".into(),
                "filesystem".into(),
                "--local".into(),
                "--cwd".into(),
                "F:/project".into(),
                "--".into(),
                "npx".into(),
                "-y".into(),
                "@modelcontextprotocol/server-filesystem".into(),
                ".".into(),
            ]),
        );
    }

    #[test]
    fn falls_back_for_urls_and_headers_that_could_expose_credentials() {
        let configs = [
            json!({"url":"https://user:password@mcp.example.com/mcp"}),
            json!({"url":"https://mcp.example.com/mcp?api_key=literal-secret"}),
            json!({"url":"https://mcp.example.com/mcp","headers":{"X-Client-Token":"literal-secret"}}),
            json!({"url":"https://mcp.example.com/mcp","headers":{"Authorization":"Bearer ${TOKEN}-suffix"}}),
            json!({"url":"https://mcp.example.com/mcp","headers":{"X-Internal":"arbitrary-custom-value"}}),
        ];
        for config in configs {
            assert!(
                native_add_args("private-api", ConfigScope::Global, &config).is_none(),
                "unsafe config should use file fallback: {config}"
            );
        }
    }

    #[test]
    fn accepts_complete_environment_references_but_not_literal_oauth_or_env_secrets() {
        assert!(native_add_args(
            "private-api",
            ConfigScope::Global,
            &json!({"url":"https://mcp.example.com/mcp","headers":{"X-Client-Token":"${DOCS_TOKEN}"},"oauth":{"clientSecret":"${OAUTH_SECRET}"}}),
        )
        .is_some());

        for config in [
            json!({"url":"https://mcp.example.com/mcp","oauth":{"clientSecret":"literal-secret"}}),
            json!({"url":"https://mcp.example.com/mcp","oauth":{"clientSecret":"!echo secret"}}),
            json!({"url":"https://mcp.example.com/mcp","oauth":{"clientSecret":"${SECRET}-suffix"}}),
            json!({"command":"server","env":{"API_KEY":"${API_KEY} trailing"}}),
        ] {
            assert!(
                native_add_args("private-api", ConfigScope::Global, &config).is_none(),
                "unsafe config should use file fallback: {config}"
            );
        }
    }

    #[test]
    fn falls_back_for_token_like_stdio_arguments() {
        for argument in [
            "--access-token=literal-token",
            "eyJhbGci.eyJzdWI.opaque-signature",
            "ghp_examplepersonalaccesstoken",
        ] {
            assert!(native_add_args(
                "private-stdio",
                ConfigScope::Global,
                &json!({"command":"server","args":[argument]}),
            )
            .is_none());
        }
    }

    #[test]
    fn pi_server_names_match_the_native_mcp_contract() {
        assert!(valid_pi_name("github_1"));
        assert!(!valid_pi_name("bad.name"));
        assert!(!valid_pi_name("bad name"));
        assert!(!valid_pi_name(""));
    }

    #[test]
    #[ignore = "requires managed Pi and Node runtimes; uses only a local stdio fixture"]
    fn deep_pi_native_mcp_bridge_adds_checks_and_removes_a_server() {
        use std::fs;

        use serde_json::json;

        use super::{list_servers, remove_server, save_server_config, McpScopeRequest};
        use crate::{agent_config::ConfigScope, app_paths::AppPaths};

        let runtimes = std::env::var_os("PI_MCP_TEST_RUNTIMES")
            .map(PathBuf::from)
            .expect("PI_MCP_TEST_RUNTIMES must point at managed runtime pointers");
        let root = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_roots_with_runtimes(
            root.path().join("roaming"),
            root.path().join("local"),
            runtimes,
            root.path().join("project-root"),
        )
        .unwrap();
        let node = paths.node_runtime().unwrap();
        let server = r#"const { createInterface } = require('node:readline');
const input = createInterface({ input: process.stdin });
input.on('line', line => {
  let request; try { request = JSON.parse(line); } catch { return; }
  if (request.id === undefined) return;
  let result = {};
  if (request.method === 'initialize') result = { protocolVersion: request.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: 'fixture', version: '1' } };
  if (request.method === 'tools/list') result = { tools: [{ name: 'fixture_echo', description: 'echo', inputSchema: { type: 'object', properties: {} } }] };
  if (request.method === 'resources/list') result = { resources: [] };
  if (request.method === 'resources/templates/list') result = { resourceTemplates: [] };
  if (request.method === 'prompts/list') result = { prompts: [] };
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result }) + '\n');
});"#;
        let config = json!({ "command": node, "args": ["-e", server] });

        save_server_config(&paths, ConfigScope::Global, None, "deeppi_fixture", &config).unwrap();
        let listed = list_servers(
            &paths,
            McpScopeRequest {
                scope: ConfigScope::Global,
                project_path: None,
            },
        )
        .unwrap();
        let fixture = listed
            .servers
            .iter()
            .find(|entry| entry.name == "deeppi_fixture")
            .unwrap();
        assert_eq!(fixture.state, "connected", "Pi MCP status: {listed:?}");
        assert_eq!(fixture.tools, ["fixture_echo"]);

        remove_server(&paths, ConfigScope::Global, None, "deeppi_fixture").unwrap();
        let after = list_servers(
            &paths,
            McpScopeRequest {
                scope: ConfigScope::Global,
                project_path: None,
            },
        )
        .unwrap();
        assert!(!after
            .servers
            .iter()
            .any(|entry| entry.name == "deeppi_fixture"));
        let _ = fs::remove_file(paths.pi_home.join("mcp.json"));
    }
}
