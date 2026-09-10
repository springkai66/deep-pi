use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::{Command, Stdio},
};
use tauri::{Manager, State};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorKind {
    Vscode,
    NotepadPlusPlus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEditor {
    pub kind: EditorKind,
    pub executable: String,
}

impl ExternalEditor {
    pub fn validate(&self) -> Result<(), String> {
        let path = Path::new(&self.executable);
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let supported = match self.kind {
            EditorKind::Vscode => {
                name.eq_ignore_ascii_case("Code.exe")
                    || name.eq_ignore_ascii_case("Code - Insiders.exe")
            }
            EditorKind::NotepadPlusPlus => name.eq_ignore_ascii_case("notepad++.exe"),
        };
        if self.executable.len() > 4096
            || self.executable.chars().any(char::is_control)
            || !path.is_absolute()
            || !supported
        {
            return Err("请选择对应编辑器的绝对 .exe 路径".into());
        }
        Ok(())
    }
}

fn arguments(
    kind: EditorKind,
    path: &Path,
    line: Option<u32>,
    column: Option<u32>,
) -> Result<Vec<String>, String> {
    if line == Some(0)
        || column == Some(0)
        || line.is_some_and(|value| value > 100_000_000)
        || column.is_some_and(|value| value > 100_000_000)
    {
        return Err("文件行列必须在 1 到 100000000 之间".into());
    }
    let path = path.to_str().ok_or("文件路径不是有效 Unicode")?;
    match kind {
        EditorKind::Vscode if line.is_some() => Ok(vec![
            "--goto".into(),
            format!("{path}:{}:{}", line.unwrap(), column.unwrap_or(1)),
        ]),
        EditorKind::NotepadPlusPlus if line.is_some() => Ok(vec![
            format!("-n{}", line.unwrap()),
            format!("-c{}", column.unwrap_or(1)),
            path.into(),
        ]),
        _ => Ok(vec![path.into()]),
    }
}

#[tauri::command]
pub async fn open_project_in_editor(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    project_id: String,
    relative_path: String,
    line: Option<u32>,
    column: Option<u32>,
) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("Editor access requires the main Webview".into());
    }
    let root = app
        .state::<crate::task::TaskStore>()
        .project_path(&project_id)?;
    let editor = app
        .state::<crate::settings::SettingsStore>()
        .get()?
        .external_editor
        .ok_or("尚未配置外部编辑器，请先打开设置")?;
    tauri::async_runtime::spawn_blocking(move || {
        editor.validate()?;
        let root = Path::new(&root);
        let file = crate::project_files::GuardedPath::open(root, &relative_path, false)?;
        let executable =
            crate::project_files::pin_external_executable(root, Path::new(&editor.executable))?;
        let mut command = Command::new(&executable.path);
        command
            .args(arguments(editor.kind, &file.path, line, column)?)
            .current_dir(executable.path.parent().ok_or("编辑器路径无父目录")?)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        // The user's editor is independent of the host's task process trees.
        let child = command
            .spawn()
            .map_err(|error| format!("无法启动外部编辑器: {error}"))?;
        drop(child);
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn save_external_editor(
    webview: tauri::Webview,
    store: State<'_, crate::settings::SettingsStore>,
    editor: Option<ExternalEditor>,
) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("Editor configuration requires the main Webview".into());
    }
    store.set_external_editor(editor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_literal_arguments_for_spaces_unicode_and_shell_characters() {
        let path = std::path::Path::new("F:\\项目 & test\\file name.ts");
        let vscode = arguments(EditorKind::Vscode, path, Some(20), Some(4)).unwrap();
        assert_eq!(vscode, vec!["--goto", "F:\\项目 & test\\file name.ts:20:4"]);
        let npp = arguments(EditorKind::NotepadPlusPlus, path, Some(20), Some(4)).unwrap();
        assert_eq!(npp, vec!["-n20", "-c4", "F:\\项目 & test\\file name.ts"]);
        assert!(arguments(EditorKind::Vscode, path, Some(0), None).is_err());
    }

    #[test]
    fn rejects_shells_relative_executables_and_mismatched_editor_types() {
        for path in [
            "cmd.exe",
            "C:\\Windows\\System32\\cmd.exe",
            "C:\\Tools\\code.cmd",
            "C:\\Tools\\Code.exe\n",
        ] {
            let editor = ExternalEditor {
                kind: EditorKind::Vscode,
                executable: path.into(),
            };
            assert!(editor.validate().is_err(), "{path}");
        }
        assert!(ExternalEditor {
            kind: EditorKind::Vscode,
            executable: "C:\\Tools\\Code.exe".into()
        }
        .validate()
        .is_ok());
        assert!(ExternalEditor {
            kind: EditorKind::NotepadPlusPlus,
            executable: "C:\\Tools\\Code.exe".into()
        }
        .validate()
        .is_err());
    }
}
