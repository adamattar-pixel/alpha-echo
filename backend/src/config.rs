use std::{
    env,
    net::IpAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub host: IpAddr,
    pub port: u16,
    pub research: ResearchRunnerConfig,
}

#[derive(Debug, Clone)]
pub struct ResearchRunnerConfig {
    pub enabled: bool,
    pub codex_binary: PathBuf,
    pub model: Option<String>,
    pub reasoning_effort: String,
    pub project_root: PathBuf,
    pub timeout: std::time::Duration,
    pub max_attempts: u8,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let host = env::var("APP_HOST")
            .unwrap_or_else(|_| "127.0.0.1".into())
            .parse()
            .context("APP_HOST must be an IP address")?;
        let port = env::var("APP_PORT")
            .unwrap_or_else(|_| "8088".into())
            .parse()
            .context("APP_PORT must be a valid port")?;
        let enabled = env::var("CODEX_RESEARCH_ENABLED")
            .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let model = env::var("CODEX_RESEARCH_MODEL")
            .ok()
            .filter(|value| !value.trim().is_empty());
        if enabled && model.is_none() {
            anyhow::bail!(
                "CODEX_RESEARCH_MODEL must be set when CODEX_RESEARCH_ENABLED=true so every run records the exact model"
            );
        }
        let reasoning_effort = env::var("CODEX_RESEARCH_REASONING_EFFORT")
            .unwrap_or_else(|_| "medium".into())
            .to_ascii_lowercase();
        if !matches!(
            reasoning_effort.as_str(),
            "minimal" | "low" | "medium" | "high" | "xhigh"
        ) {
            anyhow::bail!(
                "CODEX_RESEARCH_REASONING_EFFORT must be minimal, low, medium, high, or xhigh"
            );
        }
        let project_root = env::var_os("ALPHA_ECHO_PROJECT_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."));
        let timeout_seconds = env::var("CODEX_RESEARCH_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "900".into())
            .parse::<u64>()
            .context("CODEX_RESEARCH_TIMEOUT_SECONDS must be a positive integer")?;
        let max_attempts = env::var("CODEX_RESEARCH_MAX_ATTEMPTS")
            .unwrap_or_else(|_| "2".into())
            .parse::<u8>()
            .context("CODEX_RESEARCH_MAX_ATTEMPTS must be a positive integer")?;
        if timeout_seconds == 0 || max_attempts == 0 {
            anyhow::bail!("Codex research timeout and max attempts must be positive");
        }

        Ok(Self {
            database_url,
            host,
            port,
            research: ResearchRunnerConfig {
                enabled,
                codex_binary: resolve_codex_binary(
                    env::var_os("CODEX_BINARY")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from("codex")),
                ),
                model,
                reasoning_effort,
                project_root,
                timeout: std::time::Duration::from_secs(timeout_seconds),
                max_attempts,
            },
        })
    }
}

fn resolve_codex_binary(configured: PathBuf) -> PathBuf {
    if configured.components().count() == 1 {
        if let Some(path) = env::var_os("PATH") {
            for directory in env::split_paths(&path) {
                let candidate = directory.join(&configured);
                if executable_file(&candidate) {
                    return candidate;
                }
            }
        }
    }
    if executable_file(&configured) {
        return configured;
    }
    // Only migrate the known bundle layout; never replace a custom CLI path.
    if configured.ends_with("Contents/Resources/codex") {
        if let Some(resources) = configured.parent() {
            let relocated = resources.join("codex-cli/CodexCLI.app/Contents/MacOS/codex");
            if executable_file(&relocated) {
                return relocated;
            }
        }
    }
    configured
}

fn executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

impl ResearchRunnerConfig {
    pub fn preflight(&self) -> Result<(), String> {
        if !executable_file(&self.codex_binary) {
            return Err(format!(
                "Codex executable is missing or not executable: {}. Update CODEX_BINARY and restart the backend.",
                self.codex_binary.display()
            ));
        }
        if !self.project_root.is_dir() {
            return Err(format!(
                "research project directory is missing: {}",
                self.project_root.display()
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relocated_bundled_codex() {
        let root = env::temp_dir().join(format!("alpha-echo-cli-{}", uuid::Uuid::new_v4()));
        let old = root.join("Contents/Resources/codex");
        let current = root.join("Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex");
        std::fs::create_dir_all(current.parent().unwrap()).unwrap();
        std::fs::write(&current, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&current, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(resolve_codex_binary(old), current);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preserves_explicit_custom_binary() {
        let custom = PathBuf::from("/custom/missing/research-cli");
        assert_eq!(resolve_codex_binary(custom.clone()), custom);
    }

    #[test]
    fn preflight_rejects_missing_cli_before_report_creation() {
        let config = ResearchRunnerConfig {
            enabled: true,
            codex_binary: PathBuf::from("/custom/missing/research-cli"),
            model: Some("test-model".into()),
            reasoning_effort: "medium".into(),
            project_root: env::temp_dir(),
            timeout: std::time::Duration::from_secs(1),
            max_attempts: 1,
        };
        let error = config.preflight().unwrap_err();
        assert!(error.contains("CODEX_BINARY"));
        assert!(error.contains("/custom/missing/research-cli"));
    }

    #[test]
    fn directories_are_not_executable_files() {
        assert!(!executable_file(&env::temp_dir()));
    }
}
