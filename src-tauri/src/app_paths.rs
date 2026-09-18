use std::{fs, path::PathBuf};

use crate::message::{msg, msg_with};

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub database: PathBuf,
    pub checklist: PathBuf,
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
    /// 兼容入口（测试与旧调用方）：runtimes 落在 local 目录下。
    /// 生产启动使用 [`Self::from_roots_with_runtimes`]，把运行时放到安装目录下。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn from_roots(
        roaming: PathBuf,
        local: PathBuf,
        project_root: PathBuf,
    ) -> Result<Self, String> {
        let runtimes = local.join("runtimes");
        Self::from_roots_with_runtimes(roaming, local, runtimes, project_root)
    }

    /// `runtimes` 由调用方决定：发布构建放在 DeepPi 安装目录下的 `runtimes` 子文件夹
    /// （跟随安装盘符，不写 C 盘 AppData），开发构建放在仓库 `.deeppi-runtime/runtimes`。
    pub fn from_roots_with_runtimes(
        roaming: PathBuf,
        local: PathBuf,
        runtimes: PathBuf,
        project_root: PathBuf,
    ) -> Result<Self, String> {
        let paths = Self {
            database: roaming.join("deeppi.db"),
            checklist: roaming.join("checklist.db"),
            settings: roaming.join("settings.json"),
            backups: roaming.join("backups"),
            pi_home: roaming.join("agents").join("pi"),
            dsh_home: roaming.join("agents").join("dsh"),
            runtimes,
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
            &paths.cache,
            &paths.logs,
            &paths.temp,
        ] {
            fs::create_dir_all(directory)
                .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
        }
        // 运行时安装在 DeepPi 安装目录的子文件夹里；目录创建失败通常意味着
        // DeepPi 被装进了受保护的目录（如 Program Files），需要给出可操作的提示。
        fs::create_dir_all(&paths.runtimes).map_err(|error| {
            msg_with(
                "managed.runtime_dir_create_failed",
                &[
                    ("path", &paths.runtimes.display().to_string()),
                    ("error", &error.to_string()),
                ],
            )
        })?;
        Ok(paths)
    }

    /// 当前激活的托管 Node 目录（可能是 `current` 或 pointer 指向的 `versions/<id>`）。
    pub fn managed_node_runtime(&self) -> Result<PathBuf, String> {
        crate::runtime_pointer::current(&self.runtimes.join("node"))
    }

    /// 托管 Node 可执行文件。DeepPi 的所有运行时（Pi、DSH、dshmarket）只允许由它执行，
    /// 不回落系统 PATH 上的 node。
    pub fn node_runtime(&self) -> Result<PathBuf, String> {
        let node = self.managed_node_runtime()?.join("node.exe");
        if node.is_file() {
            Ok(node)
        } else {
            Err(msg("managed.node_missing"))
        }
    }

    /// 托管 Node 自带的 npm。安装/升级 Pi、DSH、dshmarket 只用它，
    /// 不调用电脑上全局安装的 npm。
    pub fn npm_runtime(&self) -> Result<PathBuf, String> {
        let npm = self.managed_node_runtime()?.join("npm.cmd");
        if npm.is_file() {
            Ok(npm)
        } else {
            Err(msg("managed.npm_missing"))
        }
    }

    pub fn managed_pi_runtime(&self) -> Result<PathBuf, String> {
        crate::runtime_pointer::current(&self.runtimes.join("pi"))
    }

    /// 托管 Pi CLI。不存在时返回 `None`，从不回落到电脑上安装的 Pi。
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

    pub fn required_pi_cli(&self) -> Result<PathBuf, String> {
        self.pi_cli()?.ok_or_else(|| msg("managed.pi_missing"))
    }

    pub fn managed_dsh_runtime(&self) -> Result<PathBuf, String> {
        crate::runtime_pointer::current(&self.runtimes.join("dsh"))
    }

    #[cfg(debug_assertions)]
    pub fn development_dsh_runtime(&self) -> PathBuf {
        self.project_root.join(".deeppi-runtime").join("dsh")
    }

    /// 托管 DSH 运行时路径。仅开发构建允许回落到仓库内的 `.deeppi-runtime/dsh`，
    /// 发布构建必须使用托管目录，不借用电脑上安装的 DSH。
    pub fn dsh_runtime(&self) -> Result<PathBuf, String> {
        let managed = self.managed_dsh_runtime()?;
        if Self::dsh_cli_path(&managed).is_file() {
            return Ok(managed);
        }
        if self.runtimes.join("dsh/active.json").exists() {
            return Err("active DSH runtime has no executable entry".into());
        }
        #[cfg(debug_assertions)]
        {
            let development = self.development_dsh_runtime();
            if Self::dsh_cli_path(&development).is_file() {
                return Ok(development);
            }
        }
        Err(msg("managed.dsh_missing"))
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
            return Err(msg("managed.native_task_unsupported"));
        }
        let home = record
            .pi_agent_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.pi_home.clone());
        crate::snapshot::reject_link(&home)?;
        if !home.is_absolute() || !home.is_dir() {
            return Err(msg("managed.pi_config_unavailable"));
        }
        Ok(home)
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

/// 托管运行时（Node/Pi/DSH/dshmarket）的安装根目录。
/// 发布构建：DeepPi 安装目录下的 `runtimes` 子文件夹，跟随安装位置，不写 C 盘 AppData。
/// 开发构建：仓库内 `.deeppi-runtime/runtimes`，与“用 DeepPi 开发 DeepPi”的开发剖面放在一起。
pub fn managed_runtimes_root(
    install_dir: &std::path::Path,
    project_root: &std::path::Path,
) -> PathBuf {
    if cfg!(debug_assertions) {
        project_root.join(".deeppi-runtime").join("runtimes")
    } else {
        install_dir.join("runtimes")
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
        assert_eq!(
            paths.checklist,
            root.join("roaming").join("checklist.db"),
            "checklist stays in DeepPi application data",
        );
        assert!(paths.pi_home.is_dir());
        assert!(paths.dsh_home.is_dir());
        assert!(paths.runtimes.is_dir());
        assert!(paths.logs.is_dir());
        assert!(paths.backups.is_dir());

        assert_eq!(
            paths.runtimes,
            root.join("local").join("runtimes"),
            "from_roots 保持旧行为：runtimes 位于 local 目录下"
        );

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

        assert_eq!(paths.node_runtime().unwrap(), node);
        assert_eq!(paths.pi_cli().unwrap(), Some(pi));
        // 托管缺失时必须报错，而不是回落到本机环境。
        std::fs::remove_file(&node).unwrap();
        assert!(paths.node_runtime().is_err());

        std::fs::remove_dir_all(root).expect("test paths should be removed");
    }

    #[test]
    fn runtimes_root_follows_the_install_directory() {
        let root = std::env::temp_dir().join(format!("deeppi-path-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let install_dir = root.join("install");
        let project_root = root.join("project");
        let runtimes = super::managed_runtimes_root(&install_dir, &project_root);
        let expected = if cfg!(debug_assertions) {
            project_root.join(".deeppi-runtime").join("runtimes")
        } else {
            install_dir.join("runtimes")
        };
        assert_eq!(runtimes, expected);

        let paths = AppPaths::from_roots_with_runtimes(
            root.join("roaming"),
            root.join("local"),
            runtimes.clone(),
            project_root,
        )
        .expect("application paths should initialize");
        assert_eq!(paths.runtimes, runtimes);
        assert!(paths.runtimes.is_dir());
        // roaming 侧的目录不受影响。
        assert!(paths.pi_home.is_dir());

        std::fs::remove_dir_all(root).expect("test paths should be removed");
    }
}
