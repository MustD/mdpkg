use crate::model::{HookScript, HookStage, PackageItem, ValidationIssue};
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn init_hooks_dir(hooks_dir: &Path) -> Result<PathBuf, anyhow::Error> {
    fs::create_dir_all(hooks_dir)?;
    let readme_path = hooks_dir.join("README.md");
    let content = r#"# `mdpkg` Lifecycle Hooks

This directory contains lifecycle hooks executed by `mdpkg` during package installation and removal.

## Instructions for Humans and AI Coding Agents

`mdpkg` discovers executable scripts in this directory and invokes them automatically during lifecycle events.

### File Naming Convention
Hooks must be named using the pattern:
`<packageName>_<stage>` or `<packageName>_<stage>.<ext>` (e.g. `<pkg>_preInstall`, `<pkg>_postInstall.sh`, `<pkg>_preRemoval`, `<pkg>_postRemoval`)

Where `<stage>` is one of:
- `preInstall`  — runs before the package is installed (e.g. `foo_preInstall`)
- `postInstall` — runs after the package is installed (e.g. `foo_postInstall.sh`)
- `preRemoval`  — runs before the package is uninstalled (e.g. `foo_preRemoval`)
- `postRemoval` — runs after the package is uninstalled (e.g. `foo_postRemoval.sh`)

### Execution Rules
- Hooks execute as the **normal invoking user** (`$USER`, `$HOME`, user shell config).
- If root privileges are needed, the hook should invoke `sudo` inside the script. During `mdpkg sync`, `mdpkg add`, or `mdpkg update`, an active passwordless `sudo` session is maintained.
- Scripts may be directly executable (`chmod +x`), or have a `.sh` or `.py` extension. If Google Drive or another sync client strips the executable bit, `mdpkg` automatically invokes `.sh` hooks via `bash` and `.py` hooks via `python3`.

### Environment Variables Passed to Hooks
- `MDPKG_PACKAGE`   — name of the package (e.g. `ripgrep`)
- `MDPKG_SOURCE`    — source of the package (`repo`, `aur`, `npm`)
- `MDPKG_STAGE`     — lifecycle stage (`preInstall`, `postInstall`, `preRemoval`, `postRemoval`)
- `MDPKG_HOST`      — hostname of the current machine
- `MDPKG_DATA_DIR`  — root data directory containing `packages.md`

### Recommended Hook Templates

#### 1. Shell Integrations (`fzf`, `zoxide`, `mise`, `ripgrep`)
Example: `ripgrep_postInstall.sh`
```bash
#!/usr/bin/env bash
set -euo pipefail
# Verify ripgrep is functional and configure default aliases if needed
rg --version
```

#### 2. System Service & Privileged Configuration
Example: `docker_postInstall.sh`
```bash
#!/usr/bin/env bash
set -euo pipefail
# Enable and start system service using the active sudo session
sudo systemctl enable --now docker.service || true
```
"#;
    fs::write(&readme_path, content)?;
    Ok(readme_path)
}

pub fn discover_hooks(hooks_dir: &Path) -> Result<Vec<HookScript>, anyhow::Error> {
    let mut hooks = Vec::new();
    if !hooks_dir.exists() {
        return Ok(hooks);
    }

    for entry in fs::read_dir(hooks_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        if file_name == "README.md" || file_name.starts_with('.') {
            continue;
        }

        if let Some((pkg, stage)) = parse_hook_filename(file_name) {
            hooks.push(HookScript {
                package: pkg,
                stage,
                path,
            });
        }
    }

    hooks.sort_by(|a, b| a.package.cmp(&b.package).then_with(|| a.path.cmp(&b.path)));
    Ok(hooks)
}

fn parse_hook_filename(file_name: &str) -> Option<(String, HookStage)> {
    // e.g. ripgrep_postInstall.sh, dotnet-sdk-8.0_postInstall.sh, git_preInstall
    let (pkg, stage_and_ext) = file_name.rsplit_once('_')?;
    let stage_str = stage_and_ext.split('.').next()?;
    let stage = HookStage::from_stage_str(stage_str)?;
    Some((pkg.to_string(), stage))
}

pub fn validate_hooks(
    hooks_dir: &Path,
    known_packages: &HashSet<String>,
) -> Result<Vec<ValidationIssue>, anyhow::Error> {
    let mut issues = Vec::new();
    if !hooks_dir.exists() {
        return Ok(issues);
    }

    for entry in fs::read_dir(hooks_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        if file_name == "README.md" || file_name.starts_with('.') {
            continue;
        }

        let parsed = parse_hook_filename(file_name);
        match parsed {
            None => {
                issues.push(ValidationIssue {
                    file: path.clone(),
                    line: 0,
                    message: format!(
                        "Invalid hook filename '{}'. Must follow <pkg>_<stage> pattern",
                        file_name
                    ),
                });
            }
            Some((pkg, _stage)) => {
                if !known_packages.contains(&pkg) {
                    issues.push(ValidationIssue {
                        file: path.clone(),
                        line: 0,
                        message: format!(
                            "Orphan hook script '{}': package '{}' not in packages.md",
                            file_name, pkg
                        ),
                    });
                }

                let meta = entry.metadata()?;
                let is_exec = meta.permissions().mode() & 0o111 != 0;
                let has_fallback_ext = file_name.ends_with(".sh") || file_name.ends_with(".py");

                if !is_exec && !has_fallback_ext {
                    issues.push(ValidationIssue {
                        file: path.clone(),
                        line: 0,
                        message: format!(
                            "Hook '{}' is not executable and lacks a .sh or .py extension",
                            file_name
                        ),
                    });
                }
            }
        }
    }

    issues.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.message.cmp(&b.message)));
    Ok(issues)
}

pub fn resolve_hook_invocation(script: &HookScript) -> Result<(String, Vec<String>), String> {
    let meta = fs::metadata(&script.path).map_err(|e| {
        format!(
            "Cannot read metadata for hook {}: {}",
            script.path.display(),
            e
        )
    })?;
    let is_exec = meta.permissions().mode() & 0o111 != 0;

    let path_str = script.path.to_string_lossy().to_string();

    if is_exec {
        Ok((path_str, Vec::new()))
    } else {
        let ext = script
            .path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        match ext {
            "sh" => Ok(("bash".to_string(), vec![path_str])),
            "py" => Ok(("python3".to_string(), vec![path_str])),
            _ => Err(format!(
                "Hook '{}' is not executable and has no .sh or .py extension",
                script.path.display()
            )),
        }
    }
}

pub fn run_hook(
    script: &HookScript,
    pkg: &PackageItem,
    host: &str,
    data_dir: &Path,
) -> Result<(), anyhow::Error> {
    let (prog, args) = resolve_hook_invocation(script).map_err(|e| anyhow::anyhow!("{}", e))?;

    let source_str = match pkg.source {
        crate::model::PackageSource::Repo => "repo",
        crate::model::PackageSource::Aur => "aur",
        crate::model::PackageSource::Npm => "npm",
    };

    let status = Command::new(&prog)
        .args(&args)
        .env("MDPKG_PACKAGE", &pkg.name)
        .env("MDPKG_SOURCE", source_str)
        .env("MDPKG_STAGE", script.stage.as_str())
        .env("MDPKG_HOST", host)
        .env("MDPKG_DATA_DIR", data_dir)
        .status()?;

    if !status.success() {
        anyhow::bail!(
            "Hook '{}' exited with status: {}",
            script.path.display(),
            status
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HookStage, PackageItem, PackageSource};
    use std::collections::HashSet;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_hook_filename_with_dots_in_package_name() {
        let parsed = parse_hook_filename("dotnet-sdk-8.0_postInstall.sh");
        assert_eq!(
            parsed,
            Some(("dotnet-sdk-8.0".to_string(), HookStage::PostInstall))
        );

        let parsed_no_ext = parse_hook_filename("lib32-glibc-2.38_preInstall");
        assert_eq!(
            parsed_no_ext,
            Some(("lib32-glibc-2.38".to_string(), HookStage::PreInstall))
        );
    }

    #[test]
    fn hooks_init_creates_ai_readme_and_handles_non_executable_sh_and_orphan_validation() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        let readme = init_hooks_dir(&hooks_dir).unwrap();
        let readme_text = fs::read_to_string(&readme).unwrap();
        assert!(readme_text.contains("_preInstall"));
        assert!(readme_text.contains("docker_postInstall.sh"));
        assert!(readme_text.contains("ripgrep"));

        // Non-executable .sh hook (simulating Google Drive stripping +x)
        let sh_hook = hooks_dir.join("ripgrep_postInstall.sh");
        fs::write(
            &sh_hook,
            "#!/usr/bin/env bash\necho $MDPKG_PACKAGE > \"$MDPKG_DATA_DIR/hook_ran.txt\"\n",
        )
        .unwrap();

        // Extensionless non-executable hook -> should be flagged by validate_hooks
        let bad_hook = hooks_dir.join("git_preInstall");
        fs::write(&bad_hook, "echo hi").unwrap();

        // Orphan hook -> package not in catalog
        let orphan_hook = hooks_dir.join("unknownpkg_postInstall.sh");
        fs::write(&orphan_hook, "echo orphan").unwrap();

        let hooks = discover_hooks(&hooks_dir).unwrap();
        let rg_hook = hooks
            .iter()
            .find(|h| h.package == "ripgrep" && h.stage == HookStage::PostInstall)
            .unwrap();
        let (prog, args) = resolve_hook_invocation(rg_hook).unwrap();
        assert_eq!(prog, "bash");
        assert_eq!(args, vec![sh_hook.to_string_lossy().to_string()]);

        let item = PackageItem {
            checked: true,
            source: PackageSource::Repo,
            explicit_m_prefix: false,
            name: "ripgrep".into(),
            description: "search".into(),
            line_no: 1,
        };
        run_hook(rg_hook, &item, "testhost", dir.path()).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("hook_ran.txt"))
                .unwrap()
                .trim(),
            "ripgrep"
        );

        let known = HashSet::from(["ripgrep".to_string(), "git".to_string()]);
        let issues = validate_hooks(&hooks_dir, &known).unwrap();
        assert_eq!(issues.len(), 2); // bad_hook (not executable & no .sh/.py) + orphan_hook
    }
}
