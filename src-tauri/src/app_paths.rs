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

    pub fn managed_pi_runtime(&self) -> PathBuf {
        self.runtimes.join("pi").join("current")
    }

    pub fn pi_cli(&self) -> Option<PathBuf> {
        let cli = self
            .managed_pi_runtime()
            .join("node_modules")
            .join("@earendil-works")
            .join("pi-coding-agent")
            .join("dist")
            .join("bundle")
            .join("cli.js");
        cli.is_file().then_some(cli)
    }

    pub fn managed_dsh_runtime(&self) -> PathBuf {
        self.runtimes.join("dsh").join("current")
    }

    pub fn development_dsh_runtime(&self) -> PathBuf {
        self.project_root.join(".deeppi-runtime").join("dsh")
    }

    pub fn dsh_runtime(&self) -> PathBuf {
        let managed = self.managed_dsh_runtime();
        if Self::dsh_cli_path(&managed).is_file() {
            managed
        } else {
            self.development_dsh_runtime()
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

    pub fn native_pi_home(&self) -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("PI_CODING_AGENT_DIR") {
            return Some(path.into());
        }
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|home| PathBuf::from(home).join(".pi").join("agent"))
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
        assert_eq!(paths.pi_cli(), Some(pi));

        std::fs::remove_dir_all(root).expect("test paths should be removed");
    }
}
