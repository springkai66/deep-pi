use std::{fs, path::Path};

use tauri::State;

use crate::{
    project_edit::FileEditGate,
    project_files::{
        excluded_directory, project_root, require_main, validate_relative, FileIdentity,
        GuardedPath,
    },
    task::TaskStore,
};

const MAX_DELETE_ENTRIES: usize = 2_000;
const MAX_DELETE_DEPTH: usize = 16;
const MAX_DELETE_BYTES: u64 = 256 * 1024 * 1024;

fn parent(relative: &str) -> &str {
    relative.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn check_visible_directory_components(relative: &str) -> Result<(), String> {
    if relative.split('/').any(excluded_directory) {
        return Err("Excluded project directory cannot be modified".into());
    }
    Ok(())
}

fn create_entry(root: &Path, relative: &str, directory: bool) -> Result<(), String> {
    validate_relative(relative, false)?;
    check_visible_directory_components(parent(relative))?;
    if directory {
        check_visible_directory_components(relative)?;
    }
    let _parent = GuardedPath::open(root, parent(relative), true)?;
    let path = root.join(relative);
    if directory {
        fs::create_dir(&path).map_err(|error| format!("Cannot create directory: {error}"))
    } else {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map(|_| ())
            .map_err(|error| format!("Cannot create file: {error}"))
    }
}

fn entry_type(root: &Path, relative: &str) -> Result<bool, String> {
    // A reparse point fails both guarded opens. Do not use metadata(), which follows links.
    if GuardedPath::open(root, relative, false).is_ok() {
        return Ok(false);
    }
    GuardedPath::open(root, relative, true)?;
    Ok(true)
}

fn rename_entry(root: &Path, relative: &str, new_name: &str) -> Result<String, String> {
    validate_relative(relative, false)?;
    validate_relative(new_name, false)?;
    if new_name.contains('/') {
        return Err("New name must be a single path component".into());
    }
    check_visible_directory_components(parent(relative))?;
    let directory = entry_type(root, relative)?;
    if directory {
        check_visible_directory_components(relative)?;
        check_visible_directory_components(new_name)?;
    }
    let destination = if parent(relative).is_empty() {
        new_name.to_owned()
    } else {
        format!("{}/{new_name}", parent(relative))
    };
    if relative.eq_ignore_ascii_case(&destination) {
        return Err("New name must differ from the current name".into());
    }
    let source = GuardedPath::open_for_rename(root, relative, directory)?;
    // The source handle binds the rename to the inspected entry. The kernel's
    // ReplaceIfExists=false prevents replacing a concurrent destination creator.
    source.rename_no_replace(&root.join(&destination))?;
    Ok(destination)
}

struct DeleteItem {
    relative: String,
    directory: bool,
    identity: FileIdentity,
}

fn inspect_directory(
    root: &Path,
    relative: &str,
    depth: usize,
    items: &mut Vec<DeleteItem>,
    bytes: &mut u64,
) -> Result<(), String> {
    if depth > MAX_DELETE_DEPTH || items.len() >= MAX_DELETE_ENTRIES {
        return Err("Directory is too large for safe deletion".into());
    }
    check_visible_directory_components(relative)?;
    let guard = GuardedPath::open(root, relative, true)?;
    let identity = guard.identity()?;
    for entry in fs::read_dir(&guard.path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Directory contains an unsupported name")?;
        validate_relative(&name, false)?;
        let child = format!("{relative}/{name}");
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                return Err("Directory contains a reparse point".into());
            }
        }
        if metadata.is_dir() {
            inspect_directory(root, &child, depth + 1, items, bytes)?;
        } else if metadata.is_file() {
            let file = GuardedPath::open(root, &child, false)?;
            *bytes = bytes.saturating_add(
                file.path
                    .metadata()
                    .map_err(|error| error.to_string())?
                    .len(),
            );
            if *bytes > MAX_DELETE_BYTES || items.len() >= MAX_DELETE_ENTRIES {
                return Err("Directory is too large for safe deletion".into());
            }
            items.push(DeleteItem {
                relative: child,
                directory: false,
                identity: file.identity()?,
            });
        } else {
            return Err("Directory contains an unsupported entry type".into());
        }
    }
    if items.len() >= MAX_DELETE_ENTRIES {
        return Err("Directory is too large for safe deletion".into());
    }
    items.push(DeleteItem {
        relative: relative.to_owned(),
        directory: true,
        identity,
    });
    Ok(())
}

#[cfg(windows)]
#[windows::core::implement(windows::Win32::UI::Shell::IFileOperationProgressSink)]
struct RecycleProgress {
    recycled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(windows)]
#[allow(non_snake_case)]
impl windows::Win32::UI::Shell::IFileOperationProgressSink_Impl for RecycleProgress_Impl {
    fn StartOperations(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _: windows::core::HRESULT) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreRenameItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostRenameItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
        _: windows::core::HRESULT,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreMoveItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
        _: windows::core::HRESULT,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreCopyItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
        _: windows::core::HRESULT,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreDeleteItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostDeleteItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        result: windows::core::HRESULT,
        recycled: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        if result.is_ok() && !recycled.is_null() {
            self.recycled
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    }
    fn PreNewItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
        _: &windows::core::PCWSTR,
        _: u32,
        _: windows::core::HRESULT,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
fn send_to_recycle_bin(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::{
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, IBindCtx, CLSCTX_INPROC_SERVER,
                COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            },
            UI::Shell::{
                FileOperation, IFileOperation, IFileOperationProgressSink, IShellItem,
                SHCreateItemFromParsingName, FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE,
                FOF_ALLOWUNDO, FOF_NOERRORUI, FOF_WANTNUKEWARNING,
            },
        },
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .map_err(|error| format!("Cannot initialize Recycle Bin operation: {error}"))?;
    let result = (|| {
        let operation: IFileOperation =
            unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| format!("Cannot open Recycle Bin operation: {error}"))?;
        unsafe {
            operation.SetOperationFlags(
                FOFX_RECYCLEONDELETE
                    | FOF_ALLOWUNDO
                    | FOFX_EARLYFAILURE
                    | FOF_NOERRORUI
                    | FOF_WANTNUKEWARNING,
            )
        }
        .map_err(|error| format!("Cannot configure Recycle Bin operation: {error}"))?;
        let item: IShellItem =
            unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>) }
                .map_err(|error| format!("Cannot inspect Recycle Bin item: {error}"))?;
        let recycled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sink: IFileOperationProgressSink = RecycleProgress {
            recycled: recycled.clone(),
        }
        .into();
        unsafe { operation.DeleteItem(&item, &sink) }
            .map_err(|error| format!("Cannot queue Recycle Bin item: {error}"))?;
        unsafe { operation.PerformOperations() }
            .map_err(|error| format!("Cannot move item to Recycle Bin: {error}"))?;
        if unsafe { operation.GetAnyOperationsAborted() }
            .map_err(|error| format!("Cannot confirm Recycle Bin operation: {error}"))?
            .as_bool()
        {
            return Err("Recycle Bin operation was cancelled".into());
        }
        if !recycled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("Shell did not confirm a Recycle Bin entry".into());
        }
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err("Project entry remains after Recycle Bin operation".into()),
            Err(error) => Err(format!("Cannot confirm Recycle Bin operation: {error}")),
        }
    })();
    unsafe { CoUninitialize() };
    result
}

#[cfg(not(windows))]
fn send_to_recycle_bin(_path: &Path) -> Result<(), String> {
    Err("Moving project entries to the Recycle Bin currently requires Windows".into())
}

fn recycle_entry_with(
    root: &Path,
    relative: &str,
    recursive: bool,
    recycle: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    validate_relative(relative, false)?;
    check_visible_directory_components(relative)?;
    let _parent = GuardedPath::open(root, parent(relative), true)?;
    let directory = entry_type(root, relative)?;
    if directory {
        if recursive {
            let mut items = Vec::new();
            inspect_directory(root, relative, 0, &mut items, &mut 0)?;
            for item in items {
                let guard = GuardedPath::open(root, &item.relative, item.directory)?;
                if guard.identity()? != item.identity {
                    return Err("Project entry changed before recycling".into());
                }
            }
        } else {
            let guard = GuardedPath::open(root, relative, true)?;
            if fs::read_dir(&guard.path)
                .map_err(|error| error.to_string())?
                .next()
                .is_some()
            {
                return Err("Directory is not empty".into());
            }
        }
    }
    let path = root.join(relative);
    recycle(&path)
}

fn recycle_entry(root: &Path, relative: &str, recursive: bool) -> Result<(), String> {
    recycle_entry_with(root, relative, recursive, send_to_recycle_bin)
}

fn absolute_path(root: &Path, relative: &str, directory: bool) -> Result<String, String> {
    validate_relative(relative, directory)?;
    let guard = GuardedPath::open(root, relative, directory)?;
    guard
        .path
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Project path cannot be represented as Unicode".into())
}

#[tauri::command]
pub async fn create_project_entry(
    webview: tauri::Webview,
    store: State<'_, TaskStore>,
    gate: State<'_, FileEditGate>,
    project_id: String,
    relative_path: String,
    is_directory: bool,
) -> Result<(), String> {
    require_main(&webview)?;
    let root = project_root(&store, &project_id)?;
    let gate = gate.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lock = gate.lock().map_err(|_| "File edit lock is poisoned")?;
        create_entry(&root, &relative_path, is_directory)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn rename_project_entry(
    webview: tauri::Webview,
    store: State<'_, TaskStore>,
    gate: State<'_, FileEditGate>,
    project_id: String,
    relative_path: String,
    new_name: String,
) -> Result<String, String> {
    require_main(&webview)?;
    let root = project_root(&store, &project_id)?;
    let gate = gate.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lock = gate.lock().map_err(|_| "File edit lock is poisoned")?;
        rename_entry(&root, &relative_path, &new_name)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn delete_project_entry(
    webview: tauri::Webview,
    store: State<'_, TaskStore>,
    gate: State<'_, FileEditGate>,
    project_id: String,
    relative_path: String,
    recursive: bool,
) -> Result<(), String> {
    require_main(&webview)?;
    let root = project_root(&store, &project_id)?;
    let gate = gate.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lock = gate.lock().map_err(|_| "File edit lock is poisoned")?;
        recycle_entry(&root, &relative_path, recursive)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn project_entry_absolute_path(
    webview: tauri::Webview,
    store: State<'_, TaskStore>,
    project_id: String,
    relative_path: String,
    is_directory: bool,
) -> Result<String, String> {
    require_main(&webview)?;
    let root = project_root(&store, &project_id)?;
    tauri::async_runtime::spawn_blocking(move || absolute_path(&root, &relative_path, is_directory))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    fn simulate_recycle(root: &Path, relative: &str, recursive: bool) -> Result<(), String> {
        recycle_entry_with(root, relative, recursive, |path| {
            let holding = tempfile::tempdir().map_err(|error| error.to_string())?;
            let name = path.file_name().ok_or("Missing entry name")?;
            fs::rename(path, holding.path().join(name)).map_err(|error| error.to_string())
        })
    }

    #[test]
    fn create_rename_and_absolute_path() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        create_entry(root, "src", true).unwrap();
        create_entry(root, "src/main.rs", false).unwrap();
        assert!(create_entry(root, "src/main.rs", false).is_err());
        assert!(create_entry(root, "src", true).is_err());
        assert_eq!(
            absolute_path(root, "src", true).unwrap(),
            root.join("src").to_string_lossy()
        );
        assert_eq!(
            absolute_path(root, "src/main.rs", false).unwrap(),
            root.join("src/main.rs").to_string_lossy()
        );
        assert!(absolute_path(root, "missing", false).is_err());
        assert_eq!(
            rename_entry(root, "src/main.rs", "lib.rs").unwrap(),
            "src/lib.rs"
        );
        assert_eq!(rename_entry(root, "src", "source").unwrap(), "source");
        assert!(root.join("source/lib.rs").is_file());
    }

    #[test]
    fn rename_never_overwrites_and_rejects_bad_names() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        create_entry(root, "first", false).unwrap();
        create_entry(root, "second", false).unwrap();
        create_entry(root, "one", true).unwrap();
        create_entry(root, "two", true).unwrap();
        assert!(rename_entry(root, "first", "second").is_err());
        assert!(rename_entry(root, "one", "two").is_err());
        assert!(root.join("first").exists());
        assert!(root.join("one").exists());
        for name in ["../escape", "a/b", "a\\b", ".git", "CON", ""] {
            assert!(rename_entry(root, "first", name).is_err(), "{name}");
        }
        assert!(rename_entry(root, "", "other").is_err());
        assert!(rename_entry(root, "../outside", "other").is_err());
    }

    #[test]
    fn recycles_files_empty_and_recursive_directories() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        create_entry(root, "plain", false).unwrap();
        simulate_recycle(root, "plain", false).unwrap();
        create_entry(root, "empty", true).unwrap();
        simulate_recycle(root, "empty", false).unwrap();
        create_entry(root, "tree", true).unwrap();
        create_entry(root, "tree/sub", true).unwrap();
        create_entry(root, "tree/sub/a", false).unwrap();
        assert!(simulate_recycle(root, "tree", false).is_err());
        let holding = tempfile::tempdir().unwrap();
        recycle_entry_with(root, "tree", true, |path| {
            fs::rename(path, holding.path().join("tree")).map_err(|error| error.to_string())
        })
        .unwrap();
        assert!(holding.path().join("tree/sub/a").is_file());
        assert!(!root.join("tree").exists());
    }

    #[test]
    fn refuses_oversized_subtree_without_deleting_anything() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        create_entry(root, "large", true).unwrap();
        let file = fs::File::create(root.join("large/big.bin")).unwrap();
        file.set_len(MAX_DELETE_BYTES + 1).unwrap();
        drop(file);
        assert!(simulate_recycle(root, "large", true).is_err());
        assert!(root.join("large/big.bin").exists());
    }

    #[test]
    fn rejects_escape_exclusions_and_links_before_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = temp.path();
        assert!(create_entry(root, "../outside", false).is_err());
        assert!(simulate_recycle(root, "", true).is_err());
        assert!(absolute_path(root, "../outside", false).is_err());
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), b"keep").unwrap();
        assert!(simulate_recycle(root, ".git", true).is_err());
        create_entry(root, "tree", true).unwrap();
        fs::create_dir(root.join("tree/node_modules")).unwrap();
        assert!(simulate_recycle(root, "tree", true).is_err());
        assert!(root.join("tree/node_modules").exists());
        fs::remove_dir(root.join("tree/node_modules")).unwrap();
        let linked =
            std::os::windows::fs::symlink_dir(outside.path(), root.join("tree").join("link"));
        if linked.is_err() {
            let created = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(root.join("tree").join("link"))
                .arg(outside.path())
                .output()
                .unwrap();
            assert!(
                created.status.success(),
                "junction creation failed: {created:?}"
            );
        }
        assert!(simulate_recycle(root, "tree", true).is_err());
        assert!(outside.path().exists());
        assert!(absolute_path(root, "tree/link", true).is_err());
        assert!(create_entry(root, "tree/link/unsafe", false).is_err());
    }

    #[test]
    fn failed_recycling_keeps_the_original_file_and_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        create_entry(root, "plain", false).unwrap();
        create_entry(root, "folder", true).unwrap();
        create_entry(root, "folder/a", false).unwrap();
        for (entry, recursive) in [("plain", false), ("folder", true)] {
            assert!(recycle_entry_with(root, entry, recursive, |_| Err(
                "Recycle Bin unavailable".into()
            ))
            .is_err());
        }
        assert!(root.join("plain").is_file());
        assert!(root.join("folder/a").is_file());
    }
}
