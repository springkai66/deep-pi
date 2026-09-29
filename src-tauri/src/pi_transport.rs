use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};

const SOURCE: &str = include_str!("../resources/pi-transport-observer.mjs");
const CODEX_SOURCE: &str = include_str!("../resources/pi-codex-transport.mjs");
pub const CODEX_TRANSPORTS: &[&str] = &["sse", "auto", "websocket", "websocket-cached"];

fn install_source(cache: &Path, source: &str) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    let directory = cache.join("transport-observer");
    fs::create_dir_all(&directory).map_err(|_| "Cannot create transport observer directory")?;
    let digest = format!("{:x}", Sha256::digest(source.as_bytes()));
    let path = directory.join(format!("{digest}.mjs"));
    if fs::read_to_string(&path).ok().as_deref() != Some(source) {
        let mut file = atomic_write_file::AtomicWriteFile::open(&path)
            .map_err(|_| "Cannot install transport observer")?;
        file.write_all(source.as_bytes())
            .map_err(|_| "Cannot write transport observer")?;
        file.commit()
            .map_err(|_| "Cannot commit transport observer")?;
    }
    Ok(path)
}

/// Immutable, host-owned preload: never edits the installed Pi or user settings.
pub fn configure(command: &mut Command, cache: &Path) -> Result<(), String> {
    let path = install_source(cache, SOURCE)?;
    let url = tauri::Url::from_file_path(&path).map_err(|_| "Invalid transport observer path")?;
    command
        .arg("--import")
        .arg(url.as_str())
        .env("DEEPPI_TRANSPORT_DIAGNOSTICS", "1");
    Ok(())
}

/// Add after the Pi CLI argument: this is a Pi extension, not a Node preload.
pub fn configure_codex(
    command: &mut Command,
    cache: &Path,
    cli: &Path,
    transport: &str,
) -> Result<(), String> {
    if !CODEX_TRANSPORTS.contains(&transport) {
        return Err("Unsupported Codex transport".into());
    }
    let package = cli.ancestors().nth(3).ok_or("Invalid managed Pi entry")?;
    let nested =
        package.join("node_modules/@earendil-works/pi-ai/dist/api/openai-codex-responses.js");
    let hoisted = package
        .ancestors()
        .nth(2)
        .ok_or("Invalid managed Pi package")?
        .join("@earendil-works/pi-ai/dist/api/openai-codex-responses.js");
    let module = [nested, hoisted]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or("Managed Pi is missing the Codex streaming API")?;
    let module_url = tauri::Url::from_file_path(module).map_err(|_| "Invalid Codex API path")?;
    let extension = install_source(cache, CODEX_SOURCE)?;
    command
        .arg("--extension")
        .arg(extension)
        .env("DEEPPI_CODEX_API_MODULE", module_url.as_str());
    command.env("DEEPPI_CODEX_TRANSPORT", transport);
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Sse,
    Websocket,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Headers,
    Body,
    Stream,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Cause {
    UndErrBodyTimeout,
    UndErrHeadersTimeout,
    UndErrConnectTimeout,
    UndErrSocket,
    Econnreset,
    Econnrefused,
    Etimedout,
    Epipe,
    Enotfound,
    EaiAgain,
    CertHasExpired,
    DepthZeroSelfSignedCert,
    UnableToVerifyLeafSignature,
    ErrTlsCertAltnameInvalid,
    WsAbnormalClose,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransportFailure {
    pub protocol: Protocol,
    pub phase: Phase,
    pub cause: Cause,
    pub duration_ms: u64,
    pub idle_ms: Option<u64>,
    pub close_code: Option<u16>,
}

impl TransportFailure {
    pub fn parse(value: &serde_json::Value) -> Option<Self> {
        let event: Self = serde_json::from_value(value.clone()).ok()?;
        if event.duration_ms > 86_400_000
            || event.idle_ms.is_some_and(|ms| ms > 86_400_000)
            || event
                .close_code
                .is_some_and(|code| !(1002..=4999).contains(&code))
        {
            return None;
        }
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transport_schema_rejects_untrusted_text_and_out_of_range_values() {
        let mut value = json!({"protocol":"sse","phase":"body","cause":"UND_ERR_SOCKET","durationMs":50,"idleMs":40,"closeCode":null});
        assert!(TransportFailure::parse(&value).is_some());
        value["cause"] = json!("arbitrary secret");
        assert!(TransportFailure::parse(&value).is_none());
        value["cause"] = json!("UND_ERR_BODY_TIMEOUT");
        value["url"] = json!("must never log this");
        assert!(TransportFailure::parse(&value).is_none());
        value.as_object_mut().unwrap().remove("url");
        value["durationMs"] = json!(86_400_001u64);
        assert!(TransportFailure::parse(&value).is_none());
    }

    #[test]
    fn codex_extension_uses_managed_api_without_writing_user_settings() {
        for hoisted in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let package = root
                .path()
                .join("node_modules/@earendil-works/pi-coding-agent");
            let cli = package.join("dist/bundle/cli.js");
            let api = if hoisted {
                root.path().to_path_buf()
            } else {
                package.clone()
            }
            .join("node_modules/@earendil-works/pi-ai/dist/api/openai-codex-responses.js");
            fs::create_dir_all(api.parent().unwrap()).unwrap();
            fs::write(&api, "export const streamSimple = () => {};\n").unwrap();
            let cache = root.path().join("cache");
            let mut command = Command::new("node");
            command.arg(&cli);
            configure_codex(&mut command, &cache, &cli, "websocket").unwrap();
            let args: Vec<_> = command.get_args().collect();
            assert_eq!(args[0], cli.as_os_str());
            assert_eq!(args[1], "--extension");
            assert_eq!(
                fs::read_to_string(Path::new(args[2])).unwrap(),
                CODEX_SOURCE
            );
            let (_, url) = command
                .get_envs()
                .find(|(key, _)| *key == "DEEPPI_CODEX_API_MODULE")
                .unwrap();
            assert_eq!(
                tauri::Url::parse(url.unwrap().to_str().unwrap())
                    .unwrap()
                    .to_file_path()
                    .unwrap(),
                api
            );
            assert!(command
                .get_envs()
                .any(|(key, value)| key == "DEEPPI_CODEX_TRANSPORT"
                    && value == Some(std::ffi::OsStr::new("websocket"))));
            assert!(!root.path().join("settings.json").exists());
            assert!(!package.join("settings.json").exists());
        }
    }

    #[test]
    fn codex_extension_rejects_missing_managed_api_without_falling_back_to_global_pi() {
        let root = tempfile::tempdir().unwrap();
        let cli = root
            .path()
            .join("node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js");
        let mut command = Command::new("node");
        assert!(configure_codex(&mut command, root.path(), &cli, "sse").is_err());
        assert_eq!(command.get_args().count(), 0);
    }

    #[test]
    fn preload_uses_file_url_and_leaves_pi_entry_as_the_main_program() {
        let cache =
            std::env::temp_dir().join(format!("deeppi-transport-test-{}", uuid::Uuid::new_v4()));
        let mut command = Command::new("node");
        configure(&mut command, &cache).unwrap();
        command.arg("pi-entry.js");
        let args: Vec<_> = command
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "--import");
        let url = tauri::Url::parse(&args[1]).unwrap();
        assert_eq!(
            fs::read_to_string(url.to_file_path().unwrap()).unwrap(),
            SOURCE
        );
        assert_eq!(args[2], "pi-entry.js");
        let mut second = Command::new("node");
        configure(&mut second, &cache).unwrap();
        assert_eq!(second.get_args().nth(1), command.get_args().nth(1));
        fs::remove_dir_all(cache).unwrap();
    }
}
