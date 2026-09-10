use std::{fs, path::PathBuf};

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub database: PathBuf,
    pub settings: PathBuf,
    pub backups: PathBuf,
    pub pi_home: PathBuf,
    pub dsh_home: PathBuf,
    pub runtimes: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
    pub temp: PathBuf,
    pub project_root: PathBuf,
}

impl AppPaths {
    pub fn from_roots(
        roaming: PathBuf,
        local: PathBuf,
        project_root: PathBuf,
    ) -> Result<Self, String> {
        let paths = Self {
            database: roaming.join("deeppi.db"),
            settings: roaming.join("settings.json"),
            backups: roaming.join("backups"),
            pi_home: roaming.join("agents").join("pi"),
            dsh_home: roaming.join("agents").join("dsh"),
            runtimes: local.join("runtimes"),
            cache: local.join("cache"),
            logs: local.join("logs"),
            temp: local.join("temp"),
            project_root,
        };
        for directory in [
            &roaming,
            &local,
            &paths.backups,
            &paths.pi_home,
            &paths.dsh_home,
            &paths.runtimes,
            &paths.cache,
            &paths.logs,
            &paths.temp,
        ] {
            fs::create_dir_all(directory)
                .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
        }
        Ok(paths)
    }

    pub fn managed_node_executable(&self) -> PathBuf {
        self.runtimes.join("node").join("current").join("node.exe")
    }

    pub fn node_executable(&self) -> PathBuf {
        let managed = self.managed_node_executable();
        if managed.is_file() {
            managed
        } else {
            PathBuf::from("node.exe")
        }
    }

    pub fn managed_pi_runtime(&self) -> Result<PathBuf, String> {
        crate::runtime_pointer::current(&self.runtimes.join("pi"))
    }

    pub fn pi_cli(&self) -> Result<Option<PathBuf>, String> {
        let cli = self
            .managed_pi_runtime()?
            .join("node_modules")
            .join("@earendil-works")
            .join("pi-coding-agent")
            .join("dist")
            .join("bundle")
            .join("cli.js");
        if cli.is_file() {
            Ok(Some(cli))
        } else if self.runtimes.join("pi/active.json").exists() {
            Err("active Pi runtime has no executable entry".into())
        } else {
            Ok(None)
        }
    }

    pub fn managed_dsh_runtime(&self) -> Result<PathBuf, String> {
        crate::runtime_pointer::current(&self.runtimes.join("dsh"))
    }

    pub fn development_dsh_runtime(&self) -> PathBuf {
        self.project_root.join(".deeppi-runtime").join("dsh")
    }

    pub fn dsh_runtime(&self) -> Result<PathBuf, String> {
        let managed = self.managed_dsh_runtime()?;
        if Self::dsh_cli_path(&managed).is_file() {
            Ok(managed)
        } else if self.runtimes.join("dsh/active.json").exists() {
            Err("active DSH runtime has no executable entry".into())
        } else {
            Ok(self.development_dsh_runtime())
        }
    }

    pub fn dsh_cli_path(runtime: &std::path::Path) -> PathBuf {
        runtime
            .join("node_modules")
            .join("@deepseek-ai")
            .join("dsh")
            .join("lib")
            .join("bin.js")
    }

    pub fn pi_models_file(&self) -> PathBuf {
        self.pi_home.join("models.json")
    }

    pub fn pi_auth_file(&self) -> PathBuf {
        self.pi_home.join("auth.json")
    }

    pub fn pi_environment_home(&self, environment: &str) -> Result<PathBuf, String> {
        match environment {
            "managed" => Ok(self.pi_home.clone()),
            _ => Err("Unknown Pi environment".into()),
        }
    }

    pub fn new_pi_environment(&self, preference: &str) -> Result<&'static str, String> {
        match preference {
            "managed" | "auto" | "native" => Ok("managed"),
            _ => Err("Unknown Pi environment".into()),
        }
    }

    pub fn task_pi_home(&self, record: &crate::task::TaskRecord) -> Result<PathBuf, String> {
        // v1 只支持托管环境。旧的 native 任务记录保留在数据库中，但不能启动或恢复。
        if record.pi_environment != "managed" {
            return Err("当前稳定版仅支持 DeepPi 托管任务，本机会话记录已保留".into());
        }
        let home = record
            .pi_agent_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.pi_home.clone());
        crate::snapshot::reject_link(&home)?;
        if !home.is_absolute() || !home.is_dir() {
            return Err("任务绑定的 Pi 配置目录不可用".into());
        }
        Ok(home)
    }

    pub fn system_pi_cli(&self) -> Option<PathBuf> {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut directories: Vec<_> = std::env::split_paths(&path)
            .filter(|path| path.is_absolute())
            .collect();
        if let Some(roaming) = std::env::var_os("APPDATA") {
            directories.push(PathBuf::from(roaming).join("npm"));
        }
        directories
            .into_iter()
            .filter(|directory| directory != &self.project_root)
            .map(|directory| {
                directory.join("node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js")
            })
            .find(|entry| entry.is_file())
    }

    pub fn available_pi_cli(&self) -> Result<Option<PathBuf>, String> {
        Ok(self.pi_cli()?.or_else(|| self.system_pi_cli()))
    }

    pub fn dshmarket_manifest(&self) -> PathBuf {
        self.dshmarket_package().join("package.json")
    }

    pub fn dsh_profile(&self) -> PathBuf {
        self.dsh_home.join("profiles").join("web")
    }

    pub fn dshmarket_package(&self) -> PathBuf {
        self.dsh_profile().join("node_modules").join("dshmarket")
    }
}

#[cfg(test)]
mod tests {
    use super::AppPaths;

    #[test]
    fn creates_roaming_and_local_application_directories() {
        let root = std::env::temp_dir().join(format!("deeppi-path-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let paths = AppPaths::from_roots(
            root.join("roaming"),
            root.join("local"),
            root.join("project"),
        )
        .expect("application paths should initialize");

        assert!(paths
            .database
            .parent()
            .expect("database has a parent")
            .is_dir());
        assert!(paths.pi_home.is_dir());
        assert!(paths.dsh_home.is_dir());
        assert!(paths.runtimes.is_dir());
        assert!(paths.logs.is_dir());
        assert!(paths.backups.is_dir());

        let node = paths.runtimes.join("node").join("current").join("node.exe");
        let pi = paths
            .runtimes
            .join("pi")
            .join("current")
            .join("node_modules")
            .join("@earendil-works")
            .join("pi-coding-agent")
            .join("dist")
            .join("bundle")
            .join("cli.js");
        std::fs::create_dir_all(node.parent().expect("node has a parent"))
            .expect("node runtime directory should be created");
        std::fs::create_dir_all(pi.parent().expect("Pi has a parent"))
            .expect("Pi runtime directory should be created");
        std::fs::write(&node, "").expect("node marker should be written");
        std::fs::write(&pi, "").expect("Pi marker should be written");

        assert_eq!(paths.node_executable(), node);
        assert_eq!(paths.pi_cli().unwrap(), Some(pi));

        std::fs::remove_dir_all(root).expect("test paths should be removed");
    }
}
