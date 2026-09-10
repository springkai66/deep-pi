use std::{fs, path::PathBuf, process::Command, time::Duration};

pub struct TestRepo {
    pub base: PathBuf,
    pub root: PathBuf,
    pub executable: PathBuf,
}

impl TestRepo {
    pub fn new() -> Self {
        let base = std::env::temp_dir().join(format!("deeppi-git-{}", uuid::Uuid::new_v4()));
        let root = base.join("repo");
        fs::create_dir_all(&root).unwrap();
        let executable = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .filter(|path| path.is_absolute())
            .map(|path| path.join("git.exe"))
            .find(|path| path.is_file())
            .expect("Git is required for integration tests");
        let repo = Self {
            base,
            root,
            executable,
        };
        repo.git(&["init", "--template=", "--initial-branch=main"]);
        repo.git(&["config", "core.autocrlf", "false"]);
        repo.git(&["config", "user.name", "DeepPi Test"]);
        repo.git(&["config", "user.email", "test@localhost"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.executable);
        for (key, _) in std::env::vars_os() {
            if key
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("GIT_")
            {
                command.env_remove(key);
            }
        }
        command
            .current_dir(&self.root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "NUL")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(args);
        command
    }

    pub fn git(&self, args: &[&str]) {
        let output =
            crate::process_runner::run(&mut self.command(args), Duration::from_secs(10)).unwrap();
        assert!(output.status.success(), "Git {args:?}: {}", output.text());
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}
