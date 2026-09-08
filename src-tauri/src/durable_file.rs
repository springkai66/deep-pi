use std::{fs, io::Write, path::Path};

#[cfg(windows)]
fn move_file(from: &Path, to: &Path, replace: bool) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let from = fs::canonicalize(from)?;
    let parent = to
        .parent()
        .ok_or_else(|| std::io::Error::other("target has no parent"))?;
    let to = fs::canonicalize(parent)?.join(
        to.file_name()
            .ok_or_else(|| std::io::Error::other("target has no name"))?,
    );
    let from = from
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let flags = MOVEFILE_WRITE_THROUGH
        | if replace {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        move_file(from, to, false)
    }
    #[cfg(not(windows))]
    {
        fs::rename(from, to)
    }
}

pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    crate::snapshot::reject_link(path)?;
    #[cfg(windows)]
    {
        let temporary = path.with_file_name(format!(".deeppi-write-{}", uuid::Uuid::new_v4()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        let result = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        let result = result.and_then(|()| move_file(&temporary, path, true));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        let mut file =
            atomic_write_file::AtomicWriteFile::open(path).map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.commit().map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_metadata_without_leaving_temporary_files() {
        let root = std::env::temp_dir().join(format!("deeppi-durable-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("state.json");
        write(&target, b"old").unwrap();
        write(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
