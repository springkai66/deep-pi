use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const CODEX_SOURCE: &str = include_str!("../resources/pi-codex-transport.mjs");
pub const CODEX_TRANSPORTS: &[&str] = &["sse", "auto", "websocket", "websocket-cached"];

fn install_source(cache: &Path, source: &str) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    let directory = cache.join("codex-transport");
    fs::create_dir_all(&directory).map_err(|_| "Cannot create Pi transport extension directory")?;
    let digest = format!("{:x}", Sha256::digest(source.as_bytes()));
    let path = directory.join(format!("{digest}.mjs"));
    if fs::read_to_string(&path).ok().as_deref() != Some(source) {
        let mut file = atomic_write_file::AtomicWriteFile::open(&path)
            .map_err(|_| "Cannot install Pi transport extension")?;
        file.write_all(source.as_bytes())
            .map_err(|_| "Cannot write Pi transport extension")?;
        file.commit()
            .map_err(|_| "Cannot commit Pi transport extension")?;
    }
    Ok(path)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
