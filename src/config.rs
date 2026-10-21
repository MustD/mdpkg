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

pub fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s == "~" {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home);
        }
    } else if let Some(rest) = s.strip_prefix("~/") {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

/// Normalizes a path by expanding `~`, resolving against `cwd` if relative,
/// canonicalizing if it exists on disk, or syntactically resolving `.` and `..` components.
pub fn normalize_data_dir(path: &Path, cwd: &Path) -> Result<PathBuf, anyhow::Error> {
    let expanded = expand_tilde(path);
    let candidate = if expanded.is_relative() {
        cwd.join(&expanded)
    } else {
        expanded
    };
    if let Ok(canon) = fs::canonicalize(&candidate) {
        return Ok(canon);
    }
    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            c => normalized.push(c.as_os_str()),
        }
    }
    Ok(normalized)
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
        expand_tilde(dir)
    } else if let Ok(env_dir) = env::var("MDPKG_DIR") {
        expand_tilde(Path::new(&env_dir))
    } else if let Some(cfg_dir) = &app_config.data_dir {
        let expanded = expand_tilde(cfg_dir);
        if expanded.is_relative() {
            if let Some(config_parent) = config_file.parent() {
                normalize_data_dir(&expanded, config_parent)?
            } else {
                expanded
            }
        } else {
            expanded
        }
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

pub fn save_data_dir(config_file: &Path, data_dir: &Path) -> Result<PathBuf, anyhow::Error> {
    let cwd = env::current_dir()?;
    save_data_dir_at(config_file, data_dir, &cwd)
}

pub fn save_data_dir_at(
    config_file: &Path,
    data_dir: &Path,
    cwd: &Path,
) -> Result<PathBuf, anyhow::Error> {
    if let Some(parent) = config_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut app_config: AppConfig = if config_file.exists() {
        let content = fs::read_to_string(config_file)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        AppConfig::default()
    };

    let resolved_dir = normalize_data_dir(data_dir, cwd)?;
    app_config.data_dir = Some(resolved_dir.clone());
    let toml_str = toml::to_string_pretty(&app_config)?;
    fs::write(config_file, toml_str)?;
    Ok(resolved_dir)
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

    #[test]
    fn save_data_dir_normalizes_relative_path_against_cwd_so_later_calls_from_other_dirs_use_config(
    ) {
        let dir = tempdir().unwrap();
        let config_file = dir.path().join("config.toml");
        let target_dir = dir.path().join("my_packages");
        let other_callsite = dir.path().join("other_dir");
        fs::create_dir_all(target_dir.join("sub")).unwrap();
        fs::create_dir_all(&other_callsite).unwrap();

        // Save a path containing relative segments ("sub/..") or relative to current dir
        let unnormalized = target_dir.join("sub").join("..");
        save_data_dir(&config_file, &unnormalized).unwrap();

        let saved_str = fs::read_to_string(&config_file).unwrap();
        assert!(
            !saved_str.contains(".."),
            "Expected saved config to canonicalize path without '..', got: {}",
            saved_str
        );

        let raw_config: AppConfig = toml::from_str(&saved_str).unwrap();
        assert_eq!(
            raw_config.data_dir.as_deref(),
            Some(target_dir.canonicalize().unwrap().as_path())
        );

        // Resolving from a completely different callsite without --dir must resolve to target_dir, not other_callsite
        let resolved =
            resolve_paths(None, None, &config_file, &other_callsite, "testhost").unwrap();
        assert_eq!(resolved.data_dir, target_dir.canonicalize().unwrap());

        // Also test pure relative path "." or "my_packages" resolved against an explicit cwd
        save_data_dir_at(&config_file, Path::new("."), &target_dir).unwrap();
        let resolved_dot =
            resolve_paths(None, None, &config_file, &other_callsite, "testhost").unwrap();
        assert_eq!(resolved_dot.data_dir, target_dir.canonicalize().unwrap());

        save_data_dir_at(&config_file, Path::new("my_packages"), dir.path()).unwrap();
        let resolved_sub =
            resolve_paths(None, None, &config_file, &other_callsite, "testhost").unwrap();
        assert_eq!(resolved_sub.data_dir, target_dir.canonicalize().unwrap());
    }

    #[test]
    fn resolve_paths_expands_tilde_in_config_data_dir() {
        let dir = tempdir().unwrap();
        let config_file = dir.path().join("config.toml");
        let callsite = dir.path().join("callsite");
        fs::create_dir_all(&callsite).unwrap();

        fs::write(&config_file, "data_dir = \"~/sync/packages\"\n").unwrap();

        let home = env::var("HOME").unwrap();
        let resolved = resolve_paths(None, None, &config_file, &callsite, "testhost").unwrap();
        assert_eq!(resolved.data_dir, PathBuf::from(home).join("sync/packages"));
    }

    #[test]
    fn resolve_paths_does_not_resolve_relative_config_data_dir_against_callsite() {
        let dir = tempdir().unwrap();
        let config_dir = dir.path().join("cfg");
        fs::create_dir_all(&config_dir).unwrap();
        let config_file = config_dir.join("config.toml");
        let callsite = dir.path().join("random_callsite");
        fs::create_dir_all(&callsite).unwrap();

        // If user manually writes a relative path in config.toml, it should resolve relative to config_file's directory, never callsite
        fs::write(&config_file, "data_dir = \"packages_repo\"\n").unwrap();

        let resolved = resolve_paths(None, None, &config_file, &callsite, "testhost").unwrap();
        assert_eq!(resolved.data_dir, config_dir.join("packages_repo"));
        assert!(!resolved.data_dir.is_relative());
    }

    #[test]
    fn normalize_data_dir_syntactically_collapses_relative_segments() {
        let cwd = Path::new("/var/workspace");
        let path = Path::new("sub/../foo/./bar");
        let norm = normalize_data_dir(path, cwd).unwrap();
        assert_eq!(norm, PathBuf::from("/var/workspace/foo/bar"));
    }
}
