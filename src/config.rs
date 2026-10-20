use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub data_dir: Option<PathBuf>,
    pub default_host: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaths {
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
    pub catalog_path: PathBuf,
    pub host_name: String,
    pub host_path: PathBuf,
    pub hooks_dir: PathBuf,
}

pub fn resolve_paths(
    cli_dir: Option<&Path>,
    cli_host: Option<&str>,
    config_file: &Path,
    cwd: &Path,
    sys_hostname: &str,
) -> Result<ResolvedPaths, anyhow::Error> {
    let app_config: AppConfig = if config_file.exists() {
        let content = fs::read_to_string(config_file)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        AppConfig::default()
    };

    let data_dir = if let Some(dir) = cli_dir {
        dir.to_path_buf()
    } else if let Ok(env_dir) = env::var("MDPKG_DIR") {
        PathBuf::from(env_dir)
    } else if let Some(cfg_dir) = &app_config.data_dir {
        cfg_dir.clone()
    } else if cwd.join("packages.md").exists() {
        cwd.to_path_buf()
    } else {
        cwd.to_path_buf()
    };

    let host_name = if let Some(host) = cli_host {
        let clean = host.strip_suffix(".md").unwrap_or(host);
        clean.to_string()
    } else if let Some(cfg_host) = &app_config.default_host {
        cfg_host.clone()
    } else if !sys_hostname.is_empty() {
        sys_hostname.to_string()
    } else {
        "localhost".to_string()
    };

    let catalog_path = data_dir.join("packages.md");
    let host_path = data_dir.join(format!("{}.md", host_name));
    let hooks_dir = data_dir.join("hooks");

    Ok(ResolvedPaths {
        config_file: config_file.to_path_buf(),
        data_dir,
        catalog_path,
        host_name,
        host_path,
        hooks_dir,
    })
}

pub fn save_data_dir(config_file: &Path, data_dir: &Path) -> Result<(), anyhow::Error> {
    if let Some(parent) = config_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut app_config: AppConfig = if config_file.exists() {
        let content = fs::read_to_string(config_file)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        AppConfig::default()
    };

    app_config.data_dir = Some(data_dir.to_path_buf());
    let toml_str = toml::to_string_pretty(&app_config)?;
    fs::write(config_file, toml_str)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn resolve_paths_respects_priority_cli_then_config_then_cwd() {
        let dir = tempdir().unwrap();
        let config_file = dir.path().join("config.toml");
        let cwd = dir.path().join("cwd");
        fs::create_dir_all(&cwd).unwrap();

        // 1. When cwd has packages.md
        fs::write(cwd.join("packages.md"), "# Packages\n").unwrap();
        let paths = resolve_paths(None, None, &config_file, &cwd, "testhost").unwrap();
        assert_eq!(paths.data_dir, cwd);
        assert_eq!(paths.host_name, "testhost");
        assert_eq!(paths.host_path, cwd.join("testhost.md"));

        // 2. When config.toml sets data_dir
        let config_dir = dir.path().join("configured");
        fs::create_dir_all(&config_dir).unwrap();
        save_data_dir(&config_file, &config_dir).unwrap();
        let paths = resolve_paths(None, None, &config_file, &cwd, "testhost").unwrap();
        assert_eq!(paths.data_dir, config_dir);

        // 3. When cli_dir is passed, it overrides config
        let cli_override = dir.path().join("clioverride");
        let paths = resolve_paths(
            Some(&cli_override),
            Some("customhost"),
            &config_file,
            &cwd,
            "testhost",
        )
        .unwrap();
        assert_eq!(paths.data_dir, cli_override);
        assert_eq!(paths.host_name, "customhost");
        assert_eq!(paths.host_path, cli_override.join("customhost.md"));
    }
}
