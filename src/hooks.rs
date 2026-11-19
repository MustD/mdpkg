use crate::model::{HookScript, HookStage, PackageItem, PackageSource, ValidationIssue};
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn init_hooks_dir(hooks_dir: &Path) -> Result<PathBuf, anyhow::Error> {
    fs::create_dir_all(hooks_dir)?;
    let readme_path = hooks_dir.join("README.md");
    let content = r#"# `mdpkg` Lifecycle Hooks

This directory contains lifecycle hooks and custom package scripts executed by `mdpkg`.

## Instructions for Humans and AI Coding Agents

`mdpkg` discovers executable scripts grouped by package directory inside `hooks/` and invokes them automatically during lifecycle events.

### Directory & File Naming Convention
Hooks must be placed inside a package directory using the pattern:
`hooks/<packageName>/<stage>` or `hooks/<packageName>/<stage>.<ext>` (e.g. `hooks/ripgrep/postInstall.sh`, `hooks/docker/postInstall.sh`)

Where `<stage>` is one of:
- `preInstall`  — runs before the package is installed (e.g. `hooks/foo/preInstall`)
- `postInstall` — runs after the package is installed (e.g. `hooks/foo/postInstall.sh`)
- `preRemoval`  — runs before the package is uninstalled (e.g. `hooks/foo/preRemoval`)
- `postRemoval` — runs after the package is uninstalled (e.g. `hooks/foo/postRemoval.sh`)

For `c:` custom packages, scripts inside `hooks/<packageName>/` define the package lifecycle directly:
- `install`     — installs the custom package (e.g. `hooks/rustup/install.sh`)
- `remove`      — uninstalls the custom package (e.g. `hooks/rustup/remove.sh`)
- `update`      — updates the custom package during `mdpkg update` (optional, e.g. `hooks/rustup/update.sh`)

### Execution Rules
- Hooks execute as the **normal invoking user** (`$USER`, `$HOME`, user shell config).
- If root privileges are needed, the hook should invoke `sudo` inside the script. During `mdpkg sync`, `mdpkg add`, or `mdpkg update`, an active passwordless `sudo` session is maintained.
- Scripts may be directly executable (`chmod +x`), or have a `.sh` or `.py` extension. If Google Drive or another sync client strips the executable bit, `mdpkg` automatically invokes `.sh` hooks via `bash` and `.py` hooks via `python3`.

### Environment Variables Passed to Hooks
- `MDPKG_PACKAGE`   — name of the package (e.g. `ripgrep`)
- `MDPKG_SOURCE`    — source of the package (`repo`, `aur`, `apt`, `npm`, `custom`)
- `MDPKG_STAGE`     — lifecycle stage (`preInstall`, `postInstall`, `preRemoval`, `postRemoval`, `install`, `remove`, `update`)
- `MDPKG_HOST`      — hostname of the current machine
- `MDPKG_DATA_DIR`  — root data directory containing `packages.md`

### Recommended Hook Templates

#### 1. Shell Integrations (`fzf`, `zoxide`, `mise`, `ripgrep`)
Example: `hooks/ripgrep/postInstall.sh`
```bash
#!/usr/bin/env bash
set -euo pipefail
# Verify ripgrep is functional and configure default aliases if needed
rg --version
```

#### 2. System Service & Privileged Configuration
Example: `hooks/docker/postInstall.sh`
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

fn should_ignore_entry_name(name: &str) -> bool {
    name == "README.md" || name.starts_with('.')
}

fn parse_stage_filename(file_name: &str) -> Option<HookStage> {
    let stage_str = file_name.split('.').next()?;
    HookStage::from_stage_str(stage_str)
}

pub fn discover_hooks(hooks_dir: &Path) -> Result<Vec<HookScript>, anyhow::Error> {
    let mut hooks = Vec::new();
    if !hooks_dir.exists() {
        return Ok(hooks);
    }

    for entry in fs::read_dir(hooks_dir)? {
        let entry = entry?;
        let pkg_dir = entry.path();
        if !pkg_dir.is_dir() {
            continue;
        }

        let pkg_name = match pkg_dir.file_name().and_then(|n| n.to_str()) {
            Some(name) if !name.starts_with('.') => name,
            _ => continue,
        };

        for file_entry in fs::read_dir(&pkg_dir)? {
            let file_entry = file_entry?;
            let file_path = file_entry.path();
            if !file_path.is_file() {
                continue;
            }

            let file_name = match file_path.file_name().and_then(|n| n.to_str()) {
                Some(name) if !should_ignore_entry_name(name) => name,
                _ => continue,
            };

            if let Some(stage) = parse_stage_filename(file_name) {
                hooks.push(HookScript {
                    package: pkg_name.to_string(),
                    stage,
                    path: file_path,
                });
            }
        }
    }

    hooks.sort_by(|a, b| a.package.cmp(&b.package).then_with(|| a.path.cmp(&b.path)));
    Ok(hooks)
}

pub fn is_stub_script(path: &Path) -> Result<bool, anyhow::Error> {
    let content = fs::read_to_string(path)?;
    Ok(content.lines().all(|line| {
        let trimmed = line.trim();
        trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed == "set -euo pipefail"
            || trimmed == "set -e"
    }))
}

pub fn scaffold_custom_package_scripts(
    hooks_dir: &Path,
    package: &str,
) -> Result<(PathBuf, PathBuf), anyhow::Error> {
    let pkg_dir = hooks_dir.join(package);
    fs::create_dir_all(&pkg_dir)?;

    let install_path = pkg_dir.join("install.sh");
    let remove_path = pkg_dir.join("remove.sh");
    let stub = format!(
        "#!/usr/bin/env bash\nset -euo pipefail\n# TODO: Add commands for {}\n",
        package
    );

    for path in [&install_path, &remove_path] {
        if !path.exists() {
            fs::write(path, &stub)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        }
    }

    Ok((install_path, remove_path))
}

fn validate_custom_stage(
    pkg_dir: &Path,
    package: &str,
    stage: HookStage,
    issues: &mut Vec<ValidationIssue>,
) -> Result<(), anyhow::Error> {
    let mut matching_paths = Vec::new();
    if pkg_dir.is_dir() {
        for entry in fs::read_dir(pkg_dir)? {
            let entry = entry?;
            let file_path = entry.path();
            if !file_path.is_file() {
                continue;
            }
            let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if should_ignore_entry_name(file_name) {
                continue;
            }
            if parse_stage_filename(file_name) == Some(stage) {
                matching_paths.push(file_path);
            }
        }
    }

    if matching_paths.is_empty() {
        issues.push(ValidationIssue {
            file: pkg_dir.to_path_buf(),
            line: 0,
            message: format!(
                "Custom package '{}' is missing a '{}' script in hooks/{}/",
                package,
                stage.as_str(),
                package
            ),
        });
    } else {
        for script_path in matching_paths {
            if is_stub_script(&script_path)? {
                issues.push(ValidationIssue {
                    file: script_path,
                    line: 0,
                    message: format!(
                        "Custom package '{}' '{}' script is still an unedited stub",
                        package,
                        stage.as_str()
                    ),
                });
            }
        }
    }

    Ok(())
}

pub fn validate_hooks(
    hooks_dir: &Path,
    catalog_items: &[PackageItem],
) -> Result<Vec<ValidationIssue>, anyhow::Error> {
    let mut issues = Vec::new();
    let known_packages: HashSet<&str> = catalog_items.iter().map(|i| i.name.as_str()).collect();

    if hooks_dir.exists() {
        for entry in fs::read_dir(hooks_dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) if !should_ignore_entry_name(name) => name,
                _ => continue,
            };

            if path.is_file() {
                issues.push(ValidationIssue {
                    file: path,
                    line: 0,
                    message:
                        "Hook scripts must be placed in a package directory: hooks/<pkg>/<stage>.sh"
                            .to_string(),
                });
                continue;
            }

            if !path.is_dir() {
                continue;
            }

            let pkg_name = name;
            if !known_packages.contains(pkg_name) {
                issues.push(ValidationIssue {
                    file: path.clone(),
                    line: 0,
                    message: format!(
                        "Orphan hook directory '{}': package '{}' not in packages.md",
                        pkg_name, pkg_name
                    ),
                });
            }

            for file_entry in fs::read_dir(&path)? {
                let file_entry = file_entry?;
                let file_path = file_entry.path();
                if !file_path.is_file() {
                    continue;
                }

                let file_name = match file_path.file_name().and_then(|n| n.to_str()) {
                    Some(name) if !should_ignore_entry_name(name) => name,
                    _ => continue,
                };

                match parse_stage_filename(file_name) {
                    None => {
                        issues.push(ValidationIssue {
                            file: file_path.clone(),
                            line: 0,
                            message: format!(
                                "Invalid hook filename '{}' in hooks/{}/. Must use a valid <stage> name",
                                file_name, pkg_name
                            ),
                        });
                    }
                    Some(_stage) => {
                        let meta = file_entry.metadata()?;
                        let is_exec = meta.permissions().mode() & 0o111 != 0;
                        let has_fallback_ext =
                            file_name.ends_with(".sh") || file_name.ends_with(".py");

                        if !is_exec && !has_fallback_ext {
                            issues.push(ValidationIssue {
                                file: file_path.clone(),
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
        }
    }

    let mut checked_custom = HashSet::new();
    for item in catalog_items {
        if item.source == PackageSource::Custom && checked_custom.insert(item.name.as_str()) {
            let pkg_dir = hooks_dir.join(&item.name);
            validate_custom_stage(&pkg_dir, &item.name, HookStage::Install, &mut issues)?;
            validate_custom_stage(&pkg_dir, &item.name, HookStage::Remove, &mut issues)?;
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

    let source_str = pkg.source.short_label();

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
    use std::fs;
    use tempfile::tempdir;

    fn pkg_item(name: &str, source: PackageSource) -> PackageItem {
        PackageItem {
            checked: true,
            source,
            explicit_m_prefix: false,
            name: name.into(),
            description: "".into(),
            line_no: 1,
        }
    }

    #[test]
    fn discovers_grouped_hooks_with_dots_in_package_name() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");

        fs::create_dir_all(hooks_dir.join("dotnet-sdk-8.0")).unwrap();
        fs::write(hooks_dir.join("dotnet-sdk-8.0/postInstall.sh"), "echo ok\n").unwrap();

        fs::create_dir_all(hooks_dir.join("lib32-glibc-2.38")).unwrap();
        fs::write(hooks_dir.join("lib32-glibc-2.38/preInstall"), "echo ok\n").unwrap();

        let hooks = discover_hooks(&hooks_dir).unwrap();
        assert_eq!(hooks.len(), 2);
        assert!(hooks
            .iter()
            .any(|h| h.package == "dotnet-sdk-8.0" && h.stage == HookStage::PostInstall));
        assert!(hooks
            .iter()
            .any(|h| h.package == "lib32-glibc-2.38" && h.stage == HookStage::PreInstall));
    }

    #[test]
    fn hooks_init_creates_ai_readme_and_handles_non_executable_sh_and_orphan_validation() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        let readme = init_hooks_dir(&hooks_dir).unwrap();
        let readme_text = fs::read_to_string(&readme).unwrap();
        assert!(readme_text.contains("hooks/<packageName>/<stage>"));
        assert!(readme_text.contains("hooks/docker/postInstall.sh"));
        assert!(readme_text.contains("ripgrep"));
        assert!(readme_text.contains("install"));
        assert!(readme_text.contains("remove"));
        assert!(readme_text.contains("update"));

        // Non-executable .sh hook (simulating Google Drive stripping +x)
        let rg_dir = hooks_dir.join("ripgrep");
        fs::create_dir_all(&rg_dir).unwrap();
        let sh_hook = rg_dir.join("postInstall.sh");
        fs::write(
            &sh_hook,
            "#!/usr/bin/env bash\necho $MDPKG_PACKAGE > \"$MDPKG_DATA_DIR/hook_ran.txt\"\n",
        )
        .unwrap();

        // Extensionless non-executable hook -> should be flagged by validate_hooks
        let git_dir = hooks_dir.join("git");
        fs::create_dir_all(&git_dir).unwrap();
        let bad_hook = git_dir.join("preInstall");
        fs::write(&bad_hook, "echo hi").unwrap();

        // Orphan hook directory -> package not in catalog
        let orphan_dir = hooks_dir.join("unknownpkg");
        fs::create_dir_all(&orphan_dir).unwrap();
        let orphan_hook = orphan_dir.join("postInstall.sh");
        fs::write(&orphan_hook, "echo orphan").unwrap();

        let hooks = discover_hooks(&hooks_dir).unwrap();
        let rg_hook = hooks
            .iter()
            .find(|h| h.package == "ripgrep" && h.stage == HookStage::PostInstall)
            .unwrap();
        let (prog, args) = resolve_hook_invocation(rg_hook).unwrap();
        assert_eq!(prog, "bash");
        assert_eq!(args, vec![sh_hook.to_string_lossy().to_string()]);

        let item = pkg_item("ripgrep", PackageSource::Repo);
        run_hook(rg_hook, &item, "testhost", dir.path()).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("hook_ran.txt"))
                .unwrap()
                .trim(),
            "ripgrep"
        );

        let catalog_items = vec![
            pkg_item("ripgrep", PackageSource::Repo),
            pkg_item("git", PackageSource::Repo),
        ];
        let issues = validate_hooks(&hooks_dir, &catalog_items).unwrap();
        assert_eq!(issues.len(), 2, "Expected 2 issues, got: {:?}", issues); // bad_hook + orphan_dir
    }

    #[test]
    fn scaffold_does_not_overwrite_existing_and_flags_missing_custom_scripts() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        init_hooks_dir(&hooks_dir).unwrap();

        let custom_items = vec![pkg_item("mytool", PackageSource::Custom)];
        let missing_issues = validate_hooks(&hooks_dir, &custom_items).unwrap();
        assert_eq!(missing_issues.len(), 2);

        let (inst_path, rem_path) = scaffold_custom_package_scripts(&hooks_dir, "mytool").unwrap();
        fs::write(&inst_path, "#!/usr/bin/env bash\necho install\n").unwrap();
        let (inst_again, rem_again) =
            scaffold_custom_package_scripts(&hooks_dir, "mytool").unwrap();
        assert_eq!(inst_again, inst_path);
        assert_eq!(rem_again, rem_path);
        assert!(!is_stub_script(&inst_path).unwrap());
        assert!(is_stub_script(&rem_path).unwrap());
    }

    #[test]
    fn discovers_grouped_hooks_and_flags_flat_files_orphans_and_stubs() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        init_hooks_dir(&hooks_dir).unwrap();

        // Valid grouped hook: hooks/ripgrep/postInstall.sh
        let rg_dir = hooks_dir.join("ripgrep");
        fs::create_dir_all(&rg_dir).unwrap();
        fs::write(
            rg_dir.join("postInstall.sh"),
            "#!/usr/bin/env bash\necho $MDPKG_PACKAGE > \"$MDPKG_DATA_DIR/hook_ran.txt\"\n",
        )
        .unwrap();

        // Scaffold custom package scripts for rustup
        let (inst_path, rem_path) = scaffold_custom_package_scripts(&hooks_dir, "rustup").unwrap();
        assert!(inst_path.exists());
        assert!(rem_path.exists());
        assert!(is_stub_script(&inst_path).unwrap());

        // Flat file at top-level hooks/ must be flagged as invalid
        fs::write(hooks_dir.join("legacy_postInstall.sh"), "echo old\n").unwrap();

        let hooks = discover_hooks(&hooks_dir).unwrap();
        assert!(hooks
            .iter()
            .any(|h| h.package == "ripgrep" && h.stage == HookStage::PostInstall));
        assert!(hooks
            .iter()
            .any(|h| h.package == "rustup" && h.stage == HookStage::Install));
        assert!(hooks
            .iter()
            .any(|h| h.package == "rustup" && h.stage == HookStage::Remove));

        let catalog_items = vec![
            PackageItem {
                checked: true,
                source: PackageSource::Repo,
                explicit_m_prefix: false,
                name: "ripgrep".into(),
                description: "".into(),
                line_no: 1,
            },
            PackageItem {
                checked: true,
                source: PackageSource::Custom,
                explicit_m_prefix: false,
                name: "rustup".into(),
                description: "".into(),
                line_no: 2,
            },
        ];

        let issues = validate_hooks(&hooks_dir, &catalog_items).unwrap();
        // 1 issue for legacy_postInstall.sh at root + 2 issues for unedited stub install.sh and remove.sh
        assert_eq!(issues.len(), 3, "Expected 3 issues, got: {:?}", issues);

        // Once user fills in install.sh and remove.sh, stub issues disappear
        fs::write(
            &inst_path,
            "#!/usr/bin/env bash\ncurl -fsSL https://sh.rustup.rs | sh -s -- -y\n",
        )
        .unwrap();
        fs::write(&rem_path, "#!/usr/bin/env bash\nrustup self uninstall -y\n").unwrap();
        fs::remove_file(hooks_dir.join("legacy_postInstall.sh")).unwrap();

        assert!(!is_stub_script(&inst_path).unwrap());
        let clean_issues = validate_hooks(&hooks_dir, &catalog_items).unwrap();
        assert!(
            clean_issues.is_empty(),
            "Expected no issues, got: {:?}",
            clean_issues
        );
    }
}
