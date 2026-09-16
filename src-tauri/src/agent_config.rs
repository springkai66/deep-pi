use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::State;

use crate::{
    app_paths::AppPaths,
    message::{msg, msg_with},
};

const MAX_JSON_BYTES: usize = 1024 * 1024;
const MAX_NAME_LENGTH: usize = 64;
/// 单个 SKILL.md 的内容上限：站点上最大的技能约 90 KB（旗舰技能 87 KB），
/// 因此上限放宽到 256 KiB，既能装下真实技能，也仍然有界（技能数上限 200）。
pub(crate) const MAX_SKILL_CONTENT_BYTES: usize = 256 * 1024;
pub(crate) const MAX_SKILL_DESCRIPTION_LENGTH: usize = 512;
const MAX_MCP_SERVERS: usize = 64;
const MAX_MCP_CONFIG_BYTES: usize = 8 * 1024;
const MAX_SKILLS: usize = 200;
const MAX_PROJECT_PATH_LENGTH: usize = 1_000;

/// 配置范围：global = 托管 Pi 主目录（~/.pi/agent），project = 项目 .pi/ 目录。
/// 与 Pi 的发现顺序一致：项目层优先于全局层。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigScope {
    Global,
    Project,
}

fn validate_project_path(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_PROJECT_PATH_LENGTH {
        return Err(msg("agent_config.project_path_invalid"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(msg("agent_config.project_path_invalid_chars"));
    }
    let project = Path::new(trimmed);
    if !project.is_dir() {
        return Err(msg("agent_config.project_missing"));
    }
    fs::canonicalize(project)
        .map_err(|error| format!("failed to resolve project directory: {error}"))
}

fn scope_pi_dir(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
) -> Result<PathBuf, String> {
    match scope {
        ConfigScope::Global => Ok(paths.pi_home.clone()),
        ConfigScope::Project => {
            let project = validate_project_path(
                project_path.ok_or_else(|| msg("agent_config.project_path_required"))?,
            )?;
            Ok(project.join(".pi"))
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    pub name: String,
    pub config: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMcpServerRequest {
    pub name: String,
    pub config: Value,
    #[serde(default)]
    pub scope: Option<ConfigScope>,
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteByNameRequest {
    pub name: String,
    #[serde(default)]
    pub scope: Option<ConfigScope>,
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSkillRequest {
    pub name: String,
    pub description: String,
    pub content: String,
    #[serde(default)]
    pub scope: Option<ConfigScope>,
    #[serde(default)]
    pub project_path: Option<String>,
}

pub fn validate_entry_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_NAME_LENGTH {
        return Err(msg("agent_config.name_length"));
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(msg("agent_config.name_charset"));
    }
    Ok(())
}

/// 在阻塞线程池中执行文件系统工作，避免占用 Tauri 主线程（WebView 事件循环）。
/// 与 `runtime.rs` / `dsh.rs` 的既有做法一致：技能目录遍历与递归删除在慢速磁盘上
/// 可达数十毫秒到数秒，留在主线程会直接表现为界面卡顿。
async fn run_blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| format!("agent config worker failed: {error}"))?
}

fn read_json_file(path: &Path) -> Result<Value, String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Value::Object(Map::new()))
        }
        Err(error) => return Err(format!("failed to read Pi config: {error}")),
    };
    if content.len() > MAX_JSON_BYTES {
        return Err(msg("agent_config.file_too_large"));
    }
    let value: Value = serde_json::from_str(&content).map_err(|error| {
        msg_with(
            "agent_config.invalid_json",
            &[("error", &error.to_string())],
        )
    })?;
    if !value.is_object() {
        return Err(msg("agent_config.not_object"));
    }
    Ok(value)
}

fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize Pi config: {error}"))?;
    let mut file = AtomicWriteFile::open(path)
        .map_err(|error| format!("failed to open Pi config: {error}"))?;
    file.write_all(&content)
        .map_err(|error| format!("failed to write Pi config: {error}"))?;
    file.commit()
        .map_err(|error| format!("failed to commit Pi config: {error}"))
}

fn mcp_file(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
) -> Result<PathBuf, String> {
    Ok(scope_pi_dir(paths, scope, project_path)?.join("mcp.json"))
}

#[tauri::command]
pub async fn list_mcp_servers(
    paths: State<'_, AppPaths>,
    scope: ConfigScope,
    project_path: Option<String>,
) -> Result<Vec<McpServerEntry>, String> {
    let paths = paths.inner().clone();
    run_blocking(move || {
        let value = read_json_file(&mcp_file(&paths, scope, project_path.as_deref())?)?;
        let Some(servers) = value.get("mcpServers").and_then(Value::as_object) else {
            return Ok(Vec::new());
        };
        if servers.len() > MAX_MCP_SERVERS {
            return Err(msg("agent_config.mcp_limit_reached"));
        }
        Ok(servers
            .iter()
            .map(|(name, config)| McpServerEntry {
                name: name.clone(),
                config: config.clone(),
            })
            .collect())
    })
    .await
}

pub(crate) fn save_mcp_server_config(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
    name: &str,
    config: &Value,
) -> Result<(), String> {
    let name = name.trim();
    validate_entry_name(name)?;
    if !config.is_object() {
        return Err(msg("agent_config.mcp_not_object"));
    }
    if serde_json::to_vec(config)
        .map_err(|error| error.to_string())?
        .len()
        > MAX_MCP_CONFIG_BYTES
    {
        return Err(msg("agent_config.mcp_too_large"));
    }
    let path = mcp_file(paths, scope, project_path)?;
    let mut value = read_json_file(&path)?;
    let mut servers = value
        .get("mcpServers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if !servers.contains_key(name) && servers.len() >= MAX_MCP_SERVERS {
        return Err(msg("agent_config.mcp_limit_reached"));
    }
    servers.insert(name.to_string(), config.clone());
    value["mcpServers"] = Value::Object(servers);
    write_json_file(&path, &value)
}

#[tauri::command]
pub async fn save_mcp_server(
    paths: State<'_, AppPaths>,
    request: SaveMcpServerRequest,
) -> Result<(), String> {
    let paths = paths.inner().clone();
    run_blocking(move || {
        save_mcp_server_config(
            &paths,
            request.scope.unwrap_or(ConfigScope::Global),
            request.project_path.as_deref(),
            &request.name,
            &request.config,
        )
    })
    .await
}

#[tauri::command]
pub async fn delete_mcp_server(
    paths: State<'_, AppPaths>,
    request: DeleteByNameRequest,
) -> Result<(), String> {
    let paths = paths.inner().clone();
    run_blocking(move || {
        let name = request.name.trim();
        validate_entry_name(name)?;
        let path = mcp_file(
            &paths,
            request.scope.unwrap_or(ConfigScope::Global),
            request.project_path.as_deref(),
        )?;
        let mut value = read_json_file(&path)?;
        let Some(mut servers) = value.get("mcpServers").and_then(Value::as_object).cloned() else {
            return Ok(());
        };
        servers.remove(name);
        value["mcpServers"] = Value::Object(servers);
        write_json_file(&path, &value)
    })
    .await
}

fn skills_dir(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
) -> Result<PathBuf, String> {
    Ok(scope_pi_dir(paths, scope, project_path)?.join("skills"))
}

fn skill_dir(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
    name: &str,
) -> Result<PathBuf, String> {
    validate_entry_name(name)?;
    Ok(skills_dir(paths, scope, project_path)?.join(name.trim()))
}

pub fn parse_skill_description(content: &str) -> String {
    let mut in_frontmatter = false;
    for line in content.lines().take(40) {
        let trimmed = line.trim();
        if trimmed == "---" {
            if in_frontmatter {
                break;
            }
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            if let Some(rest) = trimmed.strip_prefix("description:") {
                return rest
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim()
                    .to_string();
            }
        }
    }
    String::new()
}

#[tauri::command]
pub async fn list_skills(
    paths: State<'_, AppPaths>,
    scope: ConfigScope,
    project_path: Option<String>,
) -> Result<Vec<SkillEntry>, String> {
    let paths = paths.inner().clone();
    run_blocking(move || {
        let entries = match fs::read_dir(skills_dir(&paths, scope, project_path.as_deref())?) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("failed to read skills directory: {error}")),
        };
        let mut result = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_file = path.join("SKILL.md");
            if !skill_file.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let content = fs::read_to_string(&skill_file).unwrap_or_default();
            let description = parse_skill_description(&content);
            result.push(SkillEntry {
                name,
                description,
                path: skill_file.to_string_lossy().to_string(),
            });
            if result.len() >= MAX_SKILLS {
                break;
            }
        }
        result.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(result)
    })
    .await
}

pub(crate) fn save_skill_content(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
    name: &str,
    description: &str,
    content: &str,
) -> Result<(), String> {
    let name = name.trim();
    validate_entry_name(name)?;
    let description = description.trim();
    if description.is_empty() || description.len() > MAX_SKILL_DESCRIPTION_LENGTH {
        return Err(msg("agent_config.skill_description_length"));
    }
    if content.is_empty() || content.len() > MAX_SKILL_CONTENT_BYTES {
        return Err(msg("agent_config.skill_content_length"));
    }
    let dir = skill_dir(paths, scope, project_path, name)?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create skill directory: {error}"))?;
    let payload = if content.trim_start().starts_with("---") {
        content.to_owned()
    } else {
        format!(
            "---\nname: {name}\ndescription: {description}\n---\n\n{}",
            content.trim_start()
        )
    };
    let mut file = AtomicWriteFile::open(dir.join("SKILL.md"))
        .map_err(|error| format!("failed to open SKILL.md: {error}"))?;
    file.write_all(payload.as_bytes())
        .map_err(|error| format!("failed to write SKILL.md: {error}"))?;
    file.commit()
        .map_err(|error| format!("failed to commit SKILL.md: {error}"))
}

#[tauri::command]
pub async fn save_skill(
    paths: State<'_, AppPaths>,
    request: SaveSkillRequest,
) -> Result<(), String> {
    let paths = paths.inner().clone();
    run_blocking(move || {
        save_skill_content(
            &paths,
            request.scope.unwrap_or(ConfigScope::Global),
            request.project_path.as_deref(),
            &request.name,
            &request.description,
            &request.content,
        )
    })
    .await
}

#[tauri::command]
pub async fn delete_skill(
    paths: State<'_, AppPaths>,
    request: DeleteByNameRequest,
) -> Result<(), String> {
    let paths = paths.inner().clone();
    run_blocking(move || {
        let dir = skill_dir(
            &paths,
            request.scope.unwrap_or(ConfigScope::Global),
            request.project_path.as_deref(),
            &request.name,
        )?;
        if !dir.is_dir() {
            return Ok(());
        }
        fs::remove_dir_all(&dir)
            .map_err(|error| format!("failed to delete skill directory: {error}"))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::{parse_skill_description, validate_entry_name};

    #[test]
    fn rejects_empty_or_unsafe_entry_names() {
        assert!(validate_entry_name("context7").is_ok());
        assert!(validate_entry_name("my-skill_1.0").is_ok());
        assert!(validate_entry_name("").is_err());
        assert!(validate_entry_name("  ").is_err());
        assert!(validate_entry_name("../escape").is_err());
        assert!(validate_entry_name("a/b").is_err());
        assert!(validate_entry_name(&"x".repeat(65)).is_err());
    }

    #[test]
    fn parses_description_from_frontmatter() {
        let content = "---\nname: demo\ndescription: 何时使用该技能\n---\n\n正文\n";
        assert_eq!(parse_skill_description(content), "何时使用该技能");
        assert_eq!(parse_skill_description("no frontmatter"), "");
        assert_eq!(parse_skill_description("---\nother: x\n---\n"), "");
    }
}
