use crate::hooks::{discover_hooks, run_hook};
use crate::model::{HookStage, PackageItem, PackageSource};
use crate::reconcile::{InstalledPackages, SyncPlan};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

pub trait SystemBackend {
    fn query_installed(&self) -> Result<InstalledPackages, anyhow::Error>;
    fn ensure_elevated(&mut self) -> Result<(), anyhow::Error>;
    fn install_repo(&mut self, pkgs: &[String], no_confirm: bool, dry_run: bool) -> Result<(), anyhow::Error>;
    fn install_aur(&mut self, pkgs: &[String], no_confirm: bool, dry_run: bool) -> Result<(), anyhow::Error>;
    fn install_npm(&mut self, pkgs: &[String], dry_run: bool) -> Result<(), anyhow::Error>;
    fn remove_repo_or_aur(&mut self, pkgs: &[String], no_confirm: bool, dry_run: bool) -> Result<(), anyhow::Error>;
    fn remove_npm(&mut self, pkgs: &[String], dry_run: bool) -> Result<(), anyhow::Error>;
    fn run_system_update(&mut self, no_confirm: bool, skip_npm: bool, skip_orphans: bool) -> Result<(), anyhow::Error>;
}

#[derive(Default)]
pub struct MockSystemBackend {
    pub installed: InstalledPackages,
    pub elevated: bool,
    pub installed_repo: Vec<String>,
    pub installed_aur: Vec<String>,
    pub installed_npm: Vec<String>,
    pub removed_repo_or_aur: Vec<String>,
    pub removed_npm: Vec<String>,
    pub updated: bool,
}

impl SystemBackend for MockSystemBackend {
    fn query_installed(&self) -> Result<InstalledPackages, anyhow::Error> {
        Ok(self.installed.clone())
    }

    fn ensure_elevated(&mut self) -> Result<(), anyhow::Error> {
        self.elevated = true;
        Ok(())
    }

    fn install_repo(&mut self, pkgs: &[String], _no_confirm: bool, _dry_run: bool) -> Result<(), anyhow::Error> {
        self.installed_repo.extend_from_slice(pkgs);
        for p in pkgs {
            self.installed.repo_and_aur.insert(p.clone());
        }
        Ok(())
    }

    fn install_aur(&mut self, pkgs: &[String], _no_confirm: bool, _dry_run: bool) -> Result<(), anyhow::Error> {
        self.installed_aur.extend_from_slice(pkgs);
        for p in pkgs {
            self.installed.repo_and_aur.insert(p.clone());
        }
        Ok(())
    }

    fn install_npm(&mut self, pkgs: &[String], _dry_run: bool) -> Result<(), anyhow::Error> {
        self.installed_npm.extend_from_slice(pkgs);
        for p in pkgs {
            self.installed.npm.insert(p.clone());
        }
        Ok(())
    }

    fn remove_repo_or_aur(&mut self, pkgs: &[String], _no_confirm: bool, _dry_run: bool) -> Result<(), anyhow::Error> {
        self.removed_repo_or_aur.extend_from_slice(pkgs);
        for p in pkgs {
            self.installed.repo_and_aur.remove(p);
        }
        Ok(())
    }

    fn remove_npm(&mut self, pkgs: &[String], _dry_run: bool) -> Result<(), anyhow::Error> {
        self.removed_npm.extend_from_slice(pkgs);
        for p in pkgs {
            self.installed.npm.remove(p);
        }
        Ok(())
    }

    fn run_system_update(&mut self, _no_confirm: bool, _skip_npm: bool, _skip_orphans: bool) -> Result<(), anyhow::Error> {
        self.updated = true;
        Ok(())
    }
}

pub struct RealSystemBackend {
    _session: Option<crate::privilege::PrivilegeSession>,
}

impl RealSystemBackend {
    pub fn new() -> Self {
        Self { _session: None }
    }
}

impl Default for RealSystemBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemBackend for RealSystemBackend {
    fn ensure_elevated(&mut self) -> Result<(), anyhow::Error> {
        if self._session.is_none() {
            let session = crate::privilege::PrivilegeSession::acquire()?;
            self._session = Some(session);
        }
        Ok(())
    }

    fn query_installed(&self) -> Result<InstalledPackages, anyhow::Error> {
        let mut repo_and_aur = HashSet::new();
        if let Ok(output) = Command::new("pacman").arg("-Qq").output() {
            if output.status.success() {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        repo_and_aur.insert(trimmed.to_string());
                    }
                }
            }
        }

        let mut npm = HashSet::new();
        if let Ok(output) = Command::new("npm")
            .args(["list", "-g", "--depth=0", "--json"])
            .output()
        {
            if output.status.success() {
                let json_str = String::from_utf8_lossy(&output.stdout);
                npm = parse_npm_global_json(&json_str);
            }
        }

        Ok(InstalledPackages { repo_and_aur, npm })
    }

    fn install_repo(&mut self, pkgs: &[String], no_confirm: bool, dry_run: bool) -> Result<(), anyhow::Error> {
        if pkgs.is_empty() {
            return Ok(());
        }
        println!("Installing repository packages: {}", pkgs.join(" "));
        if dry_run {
            return Ok(());
        }
        let mut cmd = Command::new("pamac");
        cmd.arg("install").arg("--no-upgrade");
        if no_confirm {
            cmd.arg("--no-confirm");
        }
        cmd.args(pkgs);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("pamac install failed with status: {}", status);
        }
        Ok(())
    }

    fn install_aur(&mut self, pkgs: &[String], no_confirm: bool, dry_run: bool) -> Result<(), anyhow::Error> {
        if pkgs.is_empty() {
            return Ok(());
        }
        println!("Building AUR packages: {}", pkgs.join(" "));
        if dry_run {
            return Ok(());
        }
        let mut cmd = Command::new("pamac");
        cmd.arg("build");
        if no_confirm {
            cmd.arg("--no-confirm");
        }
        cmd.args(pkgs);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("pamac build failed with status: {}", status);
        }
        Ok(())
    }

    fn install_npm(&mut self, pkgs: &[String], dry_run: bool) -> Result<(), anyhow::Error> {
        if pkgs.is_empty() {
            return Ok(());
        }
        println!("Installing global npm packages: {}", pkgs.join(" "));
        if dry_run {
            return Ok(());
        }
        let mut cmd = Command::new("sudo");
        cmd.args(["npm", "install", "-g"]);
        cmd.args(pkgs);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("sudo npm install -g failed with status: {}", status);
        }
        Ok(())
    }

    fn remove_repo_or_aur(&mut self, pkgs: &[String], no_confirm: bool, dry_run: bool) -> Result<(), anyhow::Error> {
        if pkgs.is_empty() {
            return Ok(());
        }
        println!("Removing repo/AUR packages: {}", pkgs.join(" "));
        if dry_run {
            return Ok(());
        }
        let mut cmd = Command::new("pamac");
        cmd.arg("remove");
        if no_confirm {
            cmd.arg("--no-confirm");
        }
        cmd.args(pkgs);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("pamac remove failed with status: {}", status);
        }
        Ok(())
    }

    fn remove_npm(&mut self, pkgs: &[String], dry_run: bool) -> Result<(), anyhow::Error> {
        if pkgs.is_empty() {
            return Ok(());
        }
        println!("Removing global npm packages: {}", pkgs.join(" "));
        if dry_run {
            return Ok(());
        }
        let mut cmd = Command::new("sudo");
        cmd.args(["npm", "uninstall", "-g"]);
        cmd.args(pkgs);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("sudo npm uninstall -g failed with status: {}", status);
        }
        Ok(())
    }

    fn run_system_update(&mut self, no_confirm: bool, skip_npm: bool, skip_orphans: bool) -> Result<(), anyhow::Error> {
        // Step 1: Clean build files
        println!("==> Step 1: Cleaning AUR build files...");
        let _ = Command::new("pamac")
            .args(["clean", "--build-files", "--no-confirm"])
            .status();

        // Step 2: System update
        println!("==> Step 2: Refreshing databases and upgrading repo + AUR packages...");
        let mut cmd = Command::new("pamac");
        cmd.args(["update", "-a", "--force-refresh"]);
        if no_confirm {
            cmd.arg("--no-confirm");
        }
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("pamac update failed with status: {}", status);
        }

        // Step 3: Remove orphans
        if !skip_orphans {
            println!("==> Step 3: Removing orphan packages...");
            let _ = Command::new("pamac")
                .args(["remove", "--orphans", "--no-confirm"])
                .status();
        }

        // Step 4: Update global npm
        if !skip_npm {
            println!("==> Step 4: Updating global npm packages...");
            let npm_available = Command::new("npm")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if npm_available {
                let status = Command::new("sudo").args(["npm", "update", "-g"]).status()?;
                if !status.success() {
                    eprintln!("Warning: sudo npm update -g exited with status {}", status);
                }
            } else {
                println!("npm not found in PATH, skipping npm update.");
            }
        }

        Ok(())
    }
}

pub fn parse_npm_global_json(json_str: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(json_str) {
        if let Some(Value::Object(deps)) = map.get("dependencies") {
            for key in deps.keys() {
                result.insert(key.clone());
            }
        }
    }
    result
}

pub fn execute_sync_plan(
    plan: &SyncPlan,
    hooks_dir: &Path,
    host: &str,
    data_dir: &Path,
    backend: &mut dyn SystemBackend,
    no_confirm: bool,
    dry_run: bool,
) -> Result<(), anyhow::Error> {
    let hooks = discover_hooks(hooks_dir).unwrap_or_default();
    let mut has_hook_errors = false;

    // 1. Run Pre-install hooks and determine allowed install items
    let mut approved_install: Vec<&PackageItem> = Vec::new();
    for item in &plan.to_install {
        let pre_hook = hooks
            .iter()
            .find(|h| h.package == item.name && h.stage == HookStage::PreInstall);

        let mut hook_ok = true;
        if let Some(hook) = pre_hook {
            if dry_run {
                println!("Would run preInstall hook for {}", item.name);
            } else if let Err(e) = run_hook(hook, item, host, data_dir) {
                eprintln!("Error in preInstall hook for {}: {}", item.name, e);
                has_hook_errors = true;
                hook_ok = false;
            }
        }

        if hook_ok {
            approved_install.push(item);
        }
    }

    // 2. Batch install approved packages
    let mut repo_pkgs = Vec::new();
    let mut aur_pkgs = Vec::new();
    let mut npm_pkgs = Vec::new();

    for item in &approved_install {
        match item.source {
            PackageSource::Repo => repo_pkgs.push(item.name.clone()),
            PackageSource::Aur => aur_pkgs.push(item.name.clone()),
            PackageSource::Npm => npm_pkgs.push(item.name.clone()),
        }
    }

    backend.install_repo(&repo_pkgs, no_confirm, dry_run)?;
    backend.install_aur(&aur_pkgs, no_confirm, dry_run)?;
    backend.install_npm(&npm_pkgs, dry_run)?;

    // 3. Run Post-install hooks
    for item in &approved_install {
        let post_hook = hooks
            .iter()
            .find(|h| h.package == item.name && h.stage == HookStage::PostInstall);

        if let Some(hook) = post_hook {
            if dry_run {
                println!("Would run postInstall hook for {}", item.name);
            } else if let Err(e) = run_hook(hook, item, host, data_dir) {
                eprintln!("Error in postInstall hook for {}: {}", item.name, e);
                has_hook_errors = true;
            }
        }
    }

    // 4. Run Pre-removal hooks and determine allowed removal items
    let mut approved_remove: Vec<&PackageItem> = Vec::new();
    for item in &plan.to_remove {
        let pre_hook = hooks
            .iter()
            .find(|h| h.package == item.name && h.stage == HookStage::PreRemoval);

        let mut hook_ok = true;
        if let Some(hook) = pre_hook {
            if dry_run {
                println!("Would run preRemoval hook for {}", item.name);
            } else if let Err(e) = run_hook(hook, item, host, data_dir) {
                eprintln!("Error in preRemoval hook for {}: {}", item.name, e);
                has_hook_errors = true;
                hook_ok = false;
            }
        }

        if hook_ok {
            approved_remove.push(item);
        }
    }

    // 5. Batch remove approved packages
    let mut remove_repo_or_aur = Vec::new();
    let mut remove_npm = Vec::new();

    for item in &approved_remove {
        match item.source {
            PackageSource::Repo | PackageSource::Aur => remove_repo_or_aur.push(item.name.clone()),
            PackageSource::Npm => remove_npm.push(item.name.clone()),
        }
    }

    backend.remove_repo_or_aur(&remove_repo_or_aur, no_confirm, dry_run)?;
    backend.remove_npm(&remove_npm, dry_run)?;

    // 6. Run Post-removal hooks
    for item in &approved_remove {
        let post_hook = hooks
            .iter()
            .find(|h| h.package == item.name && h.stage == HookStage::PostRemoval);

        if let Some(hook) = post_hook {
            if dry_run {
                println!("Would run postRemoval hook for {}", item.name);
            } else if let Err(e) = run_hook(hook, item, host, data_dir) {
                eprintln!("Error in postRemoval hook for {}: {}", item.name, e);
                has_hook_errors = true;
            }
        }
    }

    if has_hook_errors {
        anyhow::bail!("One or more lifecycle hooks failed during execution");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PackageItem, PackageSource};
    use crate::reconcile::SyncPlan;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn pre_install_hook_failure_skips_only_failing_package_and_installs_others() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        fs::write(hooks_dir.join("badpkg_preInstall.sh"), "#!/usr/bin/env bash\nexit 1\n").unwrap();
        fs::write(hooks_dir.join("goodpkg_postInstall.sh"), "#!/usr/bin/env bash\necho ok > \"$MDPKG_DATA_DIR/good_post.txt\"\n").unwrap();

        let plan = SyncPlan {
            to_install: vec![
                PackageItem { checked: true, source: PackageSource::Repo, explicit_m_prefix: false, name: "badpkg".into(), description: "".into(), line_no: 1 },
                PackageItem { checked: true, source: PackageSource::Repo, explicit_m_prefix: false, name: "goodpkg".into(), description: "".into(), line_no: 2 },
            ],
            to_remove: vec![],
            skipped_installed_aur: vec![],
        };

        let mut backend = MockSystemBackend::default();
        let res = execute_sync_plan(&plan, &hooks_dir, "testhost", dir.path(), &mut backend, true, false);
        assert!(res.is_err()); // overall run reports failure due to badpkg preInstall
        assert_eq!(backend.installed_repo, vec!["goodpkg".to_string()]);
        assert!(dir.path().join("good_post.txt").exists());
    }

    #[test]
    fn parse_npm_json_extracts_top_level_dependency_keys() {
        let sample = r#"{"dependencies":{"typescript":{"version":"5.6.2"},"eslint":{"version":"9.0.0"}}}"#;
        let pkgs = parse_npm_global_json(sample);
        assert!(pkgs.contains("typescript"));
        assert!(pkgs.contains("eslint"));
        assert_eq!(pkgs.len(), 2);
    }
}
