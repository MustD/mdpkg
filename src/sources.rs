use crate::hooks::{discover_hooks, is_stub_script, run_hook};
use crate::model::{Document, HookStage, PackageItem, PackageSource};
use crate::reconcile::{InstalledPackages, SyncPlan};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub trait SystemBackend {
    fn query_installed(&self) -> Result<InstalledPackages, anyhow::Error>;
    fn ensure_elevated(&mut self) -> Result<(), anyhow::Error>;
    fn install_repo(
        &mut self,
        pkgs: &[String],
        no_confirm: bool,
        dry_run: bool,
    ) -> Result<(), anyhow::Error>;
    fn install_aur(
        &mut self,
        pkgs: &[String],
        no_confirm: bool,
        dry_run: bool,
    ) -> Result<(), anyhow::Error>;
    fn install_apt(
        &mut self,
        pkgs: &[String],
        no_confirm: bool,
        dry_run: bool,
    ) -> Result<(), anyhow::Error>;
    fn install_npm(&mut self, pkgs: &[String], dry_run: bool) -> Result<(), anyhow::Error>;
    fn remove_repo_or_aur(
        &mut self,
        pkgs: &[String],
        no_confirm: bool,
        dry_run: bool,
    ) -> Result<(), anyhow::Error>;
    fn remove_apt(
        &mut self,
        pkgs: &[String],
        no_confirm: bool,
        dry_run: bool,
    ) -> Result<(), anyhow::Error>;
    fn remove_npm(&mut self, pkgs: &[String], dry_run: bool) -> Result<(), anyhow::Error>;
    fn run_system_update(
        &mut self,
        no_confirm: bool,
        skip_npm: bool,
        skip_orphans: bool,
        ignore: &[String],
    ) -> Result<(), anyhow::Error>;
}

#[derive(Default)]
pub struct MockSystemBackend {
    pub installed: InstalledPackages,
    pub elevated: bool,
    pub installed_repo: Vec<String>,
    pub installed_aur: Vec<String>,
    pub installed_apt: Vec<String>,
    pub installed_npm: Vec<String>,
    pub removed_repo_or_aur: Vec<String>,
    pub removed_apt: Vec<String>,
    pub removed_npm: Vec<String>,
    pub updated: bool,
    pub ignored_updates: Vec<String>,
    pub fail_update: bool,
}

impl SystemBackend for MockSystemBackend {
    fn query_installed(&self) -> Result<InstalledPackages, anyhow::Error> {
        Ok(self.installed.clone())
    }

    fn ensure_elevated(&mut self) -> Result<(), anyhow::Error> {
        self.elevated = true;
        Ok(())
    }

    fn install_repo(
        &mut self,
        pkgs: &[String],
        _no_confirm: bool,
        _dry_run: bool,
    ) -> Result<(), anyhow::Error> {
        self.installed_repo.extend_from_slice(pkgs);
        for p in pkgs {
            self.installed.repo_and_aur.insert(p.clone());
        }
        Ok(())
    }

    fn install_aur(
        &mut self,
        pkgs: &[String],
        _no_confirm: bool,
        _dry_run: bool,
    ) -> Result<(), anyhow::Error> {
        self.installed_aur.extend_from_slice(pkgs);
        for p in pkgs {
            self.installed.repo_and_aur.insert(p.clone());
        }
        Ok(())
    }

    fn install_apt(
        &mut self,
        pkgs: &[String],
        _no_confirm: bool,
        _dry_run: bool,
    ) -> Result<(), anyhow::Error> {
        self.installed_apt.extend_from_slice(pkgs);
        for p in pkgs {
            self.installed.apt.insert(p.clone());
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

    fn remove_repo_or_aur(
        &mut self,
        pkgs: &[String],
        _no_confirm: bool,
        _dry_run: bool,
    ) -> Result<(), anyhow::Error> {
        self.removed_repo_or_aur.extend_from_slice(pkgs);
        for p in pkgs {
            self.installed.repo_and_aur.remove(p);
        }
        Ok(())
    }

    fn remove_apt(
        &mut self,
        pkgs: &[String],
        _no_confirm: bool,
        _dry_run: bool,
    ) -> Result<(), anyhow::Error> {
        self.removed_apt.extend_from_slice(pkgs);
        for p in pkgs {
            self.installed.apt.remove(p);
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

    fn run_system_update(
        &mut self,
        _no_confirm: bool,
        _skip_npm: bool,
        _skip_orphans: bool,
        ignore: &[String],
    ) -> Result<(), anyhow::Error> {
        self.ignored_updates = ignore.to_vec();
        if self.fail_update {
            anyhow::bail!("simulated update failure");
        }
        self.updated = true;
        Ok(())
    }
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

fn command_exists(bin: &str) -> bool {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            if dir.join(bin).is_file() {
                return true;
            }
        }
    }
    false
}

fn privileged_command(prog: &str) -> Command {
    if is_root() {
        Command::new(prog)
    } else {
        let mut cmd = Command::new("sudo");
        cmd.arg(prog);
        cmd
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

        let mut apt = HashSet::new();
        if let Ok(output) = Command::new("dpkg-query")
            .args(["-W", "-f=${Package}\t${db:Status-Abbrev}\n"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                apt = parse_dpkg_query_output(&stdout);
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

        Ok(InstalledPackages {
            repo_and_aur,
            apt,
            npm,
            custom: HashSet::new(),
        })
    }

    fn install_repo(
        &mut self,
        pkgs: &[String],
        no_confirm: bool,
        dry_run: bool,
    ) -> Result<(), anyhow::Error> {
        if pkgs.is_empty() {
            return Ok(());
        }
        println!("Installing repository packages: {}", pkgs.join(" "));
        if dry_run {
            return Ok(());
        }
        let forced = std::env::var("MDPKG_BACKEND").ok();
        let use_pacman = forced.as_deref() == Some("pacman")
            || (forced.as_deref() != Some("pamac") && !command_exists("pamac"));

        if use_pacman {
            let mut cmd = privileged_command("pacman");
            cmd.args(["-S", "--needed"]);
            if no_confirm {
                cmd.arg("--noconfirm");
            }
            cmd.args(pkgs);
            let status = cmd.status()?;
            if !status.success() {
                anyhow::bail!("pacman -S failed with status: {}", status);
            }
        } else {
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
        }
        Ok(())
    }

    fn install_aur(
        &mut self,
        pkgs: &[String],
        no_confirm: bool,
        dry_run: bool,
    ) -> Result<(), anyhow::Error> {
        if pkgs.is_empty() {
            return Ok(());
        }
        println!("Building AUR packages: {}", pkgs.join(" "));
        if dry_run {
            return Ok(());
        }
        if !command_exists("pamac") {
            eprintln!(
                "Warning: pamac is not available on this system; skipping AUR package(s): {}",
                pkgs.join(" ")
            );
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

    fn install_apt(
        &mut self,
        pkgs: &[String],
        no_confirm: bool,
        dry_run: bool,
    ) -> Result<(), anyhow::Error> {
        if pkgs.is_empty() {
            return Ok(());
        }
        println!("Installing APT packages: {}", pkgs.join(" "));
        if dry_run {
            return Ok(());
        }
        let mut cmd = if is_root() {
            let mut c = Command::new("apt");
            c.env("DEBIAN_FRONTEND", "noninteractive");
            c.arg("install");
            c
        } else {
            let mut c = Command::new("sudo");
            c.env("DEBIAN_FRONTEND", "noninteractive");
            c.args(["DEBIAN_FRONTEND=noninteractive", "apt", "install"]);
            c
        };
        if no_confirm {
            cmd.arg("-y");
        }
        cmd.args(pkgs);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("apt install failed with status: {}", status);
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
        let mut cmd = privileged_command("npm");
        cmd.args(["install", "-g"]);
        cmd.args(pkgs);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("npm install -g failed with status: {}", status);
        }
        Ok(())
    }

    fn remove_repo_or_aur(
        &mut self,
        pkgs: &[String],
        no_confirm: bool,
        dry_run: bool,
    ) -> Result<(), anyhow::Error> {
        if pkgs.is_empty() {
            return Ok(());
        }
        println!("Removing repo/AUR packages: {}", pkgs.join(" "));
        if dry_run {
            return Ok(());
        }
        if !command_exists("pamac") && !command_exists("pacman") {
            eprintln!(
                "Warning: neither pamac nor pacman is available on this system; skipping repo/AUR removal for: {}",
                pkgs.join(" ")
            );
            return Ok(());
        }
        let forced = std::env::var("MDPKG_BACKEND").ok();
        let use_pacman = forced.as_deref() == Some("pacman")
            || (forced.as_deref() != Some("pamac") && !command_exists("pamac"));

        if use_pacman {
            let mut cmd = privileged_command("pacman");
            cmd.arg("-Rns");
            if no_confirm {
                cmd.arg("--noconfirm");
            }
            cmd.args(pkgs);
            let status = cmd.status()?;
            if !status.success() {
                anyhow::bail!("pacman -Rns failed with status: {}", status);
            }
        } else {
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
        }
        Ok(())
    }

    fn remove_apt(
        &mut self,
        pkgs: &[String],
        no_confirm: bool,
        dry_run: bool,
    ) -> Result<(), anyhow::Error> {
        if pkgs.is_empty() {
            return Ok(());
        }
        println!("Removing APT packages: {}", pkgs.join(" "));
        if dry_run {
            return Ok(());
        }
        let mut cmd = if is_root() {
            let mut c = Command::new("apt");
            c.env("DEBIAN_FRONTEND", "noninteractive");
            c.arg("remove");
            c
        } else {
            let mut c = Command::new("sudo");
            c.env("DEBIAN_FRONTEND", "noninteractive");
            c.args(["DEBIAN_FRONTEND=noninteractive", "apt", "remove"]);
            c
        };
        if no_confirm {
            cmd.arg("-y");
        }
        cmd.args(pkgs);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("apt remove failed with status: {}", status);
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
        let mut cmd = privileged_command("npm");
        cmd.args(["uninstall", "-g"]);
        cmd.args(pkgs);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("npm uninstall -g failed with status: {}", status);
        }
        Ok(())
    }

    fn run_system_update(
        &mut self,
        no_confirm: bool,
        skip_npm: bool,
        skip_orphans: bool,
        ignore: &[String],
    ) -> Result<(), anyhow::Error> {
        let forced = std::env::var("MDPKG_BACKEND").ok();
        let backend_choice = match forced.as_deref() {
            Some("pamac") => "pamac",
            Some("pacman") => "pacman",
            Some("apt") => "apt",
            _ if command_exists("pamac") => "pamac",
            _ if command_exists("pacman") => "pacman",
            _ if command_exists("apt") => "apt",
            _ => "pamac",
        };

        let non_empty_ignore: Vec<&str> = ignore
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        match backend_choice {
            "pamac" => {
                println!("==> Step 1: Cleaning AUR build files...");
                let _ = Command::new("pamac")
                    .args(["clean", "--build-files", "--no-confirm"])
                    .status();

                println!("==> Step 2: Refreshing databases and upgrading repo + AUR packages...");
                let mut cmd = Command::new("pamac");
                cmd.args(["update", "-a", "--force-refresh"]);
                if no_confirm {
                    cmd.arg("--no-confirm");
                }
                if !non_empty_ignore.is_empty() {
                    cmd.arg("--ignore").arg(non_empty_ignore.join(","));
                }
                let status = cmd.status()?;
                if !status.success() {
                    anyhow::bail!("pamac update failed with status: {}", status);
                }

                if !skip_orphans {
                    println!("==> Step 3: Removing orphan packages...");
                    let _ = Command::new("pamac")
                        .args(["remove", "--orphans", "--no-confirm"])
                        .status();
                }
            }
            "pacman" => {
                println!("==> Upgrading system packages with pacman...");
                let mut cmd = privileged_command("pacman");
                cmd.arg("-Syu");
                if no_confirm {
                    cmd.arg("--noconfirm");
                }
                if !non_empty_ignore.is_empty() {
                    cmd.arg("--ignore").arg(non_empty_ignore.join(","));
                }
                let status = cmd.status()?;
                if !status.success() {
                    anyhow::bail!("pacman -Syu failed with status: {}", status);
                }
            }
            "apt" => {
                println!("==> Step 1: Updating APT package lists...");
                let status = privileged_command("apt").arg("update").status()?;
                if !status.success() {
                    anyhow::bail!("apt update failed with status: {}", status);
                }

                let pkgs_to_hold = if !non_empty_ignore.is_empty() {
                    let installed_apt = Command::new("dpkg-query")
                        .args(["-W", "-f=${Package}\t${db:Status-Abbrev}\n"])
                        .output()
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| parse_dpkg_query_output(&String::from_utf8_lossy(&o.stdout)))
                        .unwrap_or_default();

                    let already_held: HashSet<String> = Command::new("apt-mark")
                        .arg("showhold")
                        .output()
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| {
                            String::from_utf8_lossy(&o.stdout)
                                .lines()
                                .map(|l| l.trim())
                                .filter(|l| !l.is_empty())
                                .map(|l| l.to_string())
                                .collect()
                        })
                        .unwrap_or_default();

                    filter_apt_packages_to_hold(ignore, &installed_apt, &already_held)
                } else {
                    Vec::new()
                };

                if !pkgs_to_hold.is_empty() {
                    let hold_status = privileged_command("apt-mark")
                        .arg("hold")
                        .args(&pkgs_to_hold)
                        .status()?;
                    if !hold_status.success() {
                        anyhow::bail!("apt-mark hold failed with status: {}", hold_status);
                    }
                }

                println!("==> Step 2: Upgrading APT packages...");
                let mut upgrade_cmd = if is_root() {
                    let mut c = Command::new("apt");
                    c.env("DEBIAN_FRONTEND", "noninteractive");
                    c.arg("upgrade");
                    c
                } else {
                    let mut c = Command::new("sudo");
                    c.env("DEBIAN_FRONTEND", "noninteractive");
                    c.args(["DEBIAN_FRONTEND=noninteractive", "apt", "upgrade"]);
                    c
                };
                if no_confirm {
                    upgrade_cmd.arg("-y");
                }
                let upgrade_res = upgrade_cmd.status();

                if !pkgs_to_hold.is_empty() {
                    let _ = privileged_command("apt-mark")
                        .arg("unhold")
                        .args(&pkgs_to_hold)
                        .status();
                }

                let upgrade_status = upgrade_res?;
                if !upgrade_status.success() {
                    anyhow::bail!("apt upgrade failed with status: {}", upgrade_status);
                }

                if !skip_orphans {
                    println!("==> Step 3: Removing orphan APT packages...");
                    let mut autoremove_cmd = if is_root() {
                        let mut c = Command::new("apt");
                        c.env("DEBIAN_FRONTEND", "noninteractive");
                        c.args(["autoremove", "-y"]);
                        c
                    } else {
                        let mut c = Command::new("sudo");
                        c.env("DEBIAN_FRONTEND", "noninteractive");
                        c.args(["DEBIAN_FRONTEND=noninteractive", "apt", "autoremove", "-y"]);
                        c
                    };
                    let _ = autoremove_cmd.status();
                }
            }
            _ => unreachable!(),
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
                let mut cmd = privileged_command("npm");
                cmd.args(["update", "-g"]);
                let status = cmd.status()?;
                if !status.success() {
                    eprintln!("Warning: npm update -g exited with status {}", status);
                }
            } else {
                println!("npm not found in PATH, skipping npm update.");
            }
        }

        Ok(())
    }
}

pub fn filter_apt_packages_to_hold(
    ignore: &[String],
    installed_apt: &HashSet<String>,
    already_held: &HashSet<String>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for pkg in ignore {
        let trimmed = pkg.trim();
        if !trimmed.is_empty()
            && installed_apt.contains(trimmed)
            && !already_held.contains(trimmed)
            && seen.insert(trimmed.to_string())
        {
            result.push(trimmed.to_string());
        }
    }
    result
}

pub fn parse_dpkg_query_output(output: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    for line in output.lines() {
        if let Some((pkg, status)) = line.split_once('\t') {
            let pkg = pkg.trim();
            let status = status.trim();
            if !pkg.is_empty() && status.starts_with("ii") {
                result.insert(pkg.to_string());
            }
        }
    }
    result
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

fn custom_installed_state_path(data_dir: &Path, host: &str) -> PathBuf {
    data_dir
        .join(".mdpkg")
        .join(format!("installed_custom_{}.json", host))
}

pub fn load_custom_installed(data_dir: &Path, host: &str) -> HashSet<String> {
    let path = custom_installed_state_path(data_dir, host);
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(&content) {
            return list.into_iter().collect();
        }
    }
    HashSet::new()
}

pub fn save_custom_installed(
    data_dir: &Path,
    host: &str,
    set: &HashSet<String>,
) -> Result<(), anyhow::Error> {
    let path = custom_installed_state_path(data_dir, host);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut sorted: Vec<&String> = set.iter().collect();
    sorted.sort();
    let json = serde_json::to_string_pretty(&sorted)?;
    fs::write(&path, json)?;
    Ok(())
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

    // 2. Batch install approved packages and run custom install scripts
    let mut repo_pkgs = Vec::new();
    let mut aur_pkgs = Vec::new();
    let mut apt_pkgs = Vec::new();
    let mut npm_pkgs = Vec::new();
    let mut custom_install_items = Vec::new();

    for item in &approved_install {
        match item.source {
            PackageSource::Repo => repo_pkgs.push(item.name.clone()),
            PackageSource::Aur => aur_pkgs.push(item.name.clone()),
            PackageSource::Apt => apt_pkgs.push(item.name.clone()),
            PackageSource::Npm => npm_pkgs.push(item.name.clone()),
            PackageSource::Custom => custom_install_items.push(*item),
        }
    }

    backend.install_repo(&repo_pkgs, no_confirm, dry_run)?;
    backend.install_aur(&aur_pkgs, no_confirm, dry_run)?;
    backend.install_apt(&apt_pkgs, no_confirm, dry_run)?;
    backend.install_npm(&npm_pkgs, dry_run)?;

    let mut failed_custom_installs: HashSet<&str> = HashSet::new();
    for item in custom_install_items {
        let install_hook = hooks
            .iter()
            .find(|h| h.package == item.name && h.stage == HookStage::Install);

        match install_hook {
            Some(hook) if !is_stub_script(&hook.path).unwrap_or(true) => {
                if dry_run {
                    println!("Would run custom install script for {}", item.name);
                } else if let Err(e) = run_hook(hook, item, host, data_dir) {
                    eprintln!("Error in custom install script for {}: {}", item.name, e);
                    has_hook_errors = true;
                    failed_custom_installs.insert(item.name.as_str());
                } else {
                    let mut custom_set = load_custom_installed(data_dir, host);
                    custom_set.insert(item.name.clone());
                    save_custom_installed(data_dir, host, &custom_set)?;
                }
            }
            _ => {
                eprintln!(
                    "Error: custom package '{}' is missing a valid install script in hooks/{}/",
                    item.name, item.name
                );
                has_hook_errors = true;
                failed_custom_installs.insert(item.name.as_str());
            }
        }
    }

    // 3. Run Post-install hooks
    for item in &approved_install {
        if failed_custom_installs.contains(item.name.as_str()) {
            continue;
        }
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

    // 5. Batch remove approved packages and run custom remove scripts
    let mut remove_repo_or_aur = Vec::new();
    let mut remove_apt = Vec::new();
    let mut remove_npm = Vec::new();
    let mut custom_remove_items = Vec::new();

    for item in &approved_remove {
        match item.source {
            PackageSource::Repo | PackageSource::Aur => remove_repo_or_aur.push(item.name.clone()),
            PackageSource::Apt => remove_apt.push(item.name.clone()),
            PackageSource::Npm => remove_npm.push(item.name.clone()),
            PackageSource::Custom => custom_remove_items.push(*item),
        }
    }

    backend.remove_repo_or_aur(&remove_repo_or_aur, no_confirm, dry_run)?;
    backend.remove_apt(&remove_apt, no_confirm, dry_run)?;
    backend.remove_npm(&remove_npm, dry_run)?;

    let mut failed_custom_removes: HashSet<&str> = HashSet::new();
    for item in custom_remove_items {
        let remove_hook = hooks
            .iter()
            .find(|h| h.package == item.name && h.stage == HookStage::Remove);

        match remove_hook {
            Some(hook) if !is_stub_script(&hook.path).unwrap_or(true) => {
                if dry_run {
                    println!("Would run custom remove script for {}", item.name);
                } else if let Err(e) = run_hook(hook, item, host, data_dir) {
                    eprintln!("Error in custom remove script for {}: {}", item.name, e);
                    has_hook_errors = true;
                    failed_custom_removes.insert(item.name.as_str());
                } else {
                    let mut custom_set = load_custom_installed(data_dir, host);
                    custom_set.remove(&item.name);
                    save_custom_installed(data_dir, host, &custom_set)?;
                }
            }
            _ => {
                eprintln!(
                    "Error: custom package '{}' is missing a valid remove script in hooks/{}/",
                    item.name, item.name
                );
                has_hook_errors = true;
                failed_custom_removes.insert(item.name.as_str());
            }
        }
    }

    // 6. Run Post-removal hooks
    for item in &approved_remove {
        if failed_custom_removes.contains(item.name.as_str()) {
            continue;
        }
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

pub fn update_custom_packages(
    host_doc: &Document,
    installed: &InstalledPackages,
    hooks_dir: &Path,
    host: &str,
    data_dir: &Path,
    ignore: &[String],
) -> Result<(), anyhow::Error> {
    let hooks = discover_hooks(hooks_dir).unwrap_or_default();
    let mut seen = HashSet::new();

    for sec in &host_doc.sections {
        if !sec.checked {
            continue;
        }
        for cat in &sec.categories {
            if !cat.checked {
                continue;
            }
            for item in &cat.items {
                if !item.checked || item.source != PackageSource::Custom {
                    continue;
                }
                if !installed.is_installed(PackageSource::Custom, &item.name) {
                    continue;
                }
                if ignore.iter().any(|ig| ig == &item.name) {
                    continue;
                }
                if !seen.insert(item.name.as_str()) {
                    continue;
                }

                if let Some(pre_hook) = hooks
                    .iter()
                    .find(|h| h.package == item.name && h.stage == HookStage::PreInstall)
                {
                    run_hook(pre_hook, item, host, data_dir)?;
                }

                let update_or_install = hooks
                    .iter()
                    .find(|h| h.package == item.name && h.stage == HookStage::Update)
                    .or_else(|| {
                        hooks
                            .iter()
                            .find(|h| h.package == item.name && h.stage == HookStage::Install)
                    });

                match update_or_install {
                    Some(hook) if !is_stub_script(&hook.path)? => {
                        run_hook(hook, item, host, data_dir)?;
                    }
                    _ => {
                        anyhow::bail!(
                            "Custom package '{}' is missing a valid update or install script in hooks/{}/",
                            item.name,
                            item.name
                        );
                    }
                }

                if let Some(post_hook) = hooks
                    .iter()
                    .find(|h| h.package == item.name && h.stage == HookStage::PostInstall)
                {
                    run_hook(post_hook, item, host, data_dir)?;
                }
            }
        }
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
        fs::create_dir_all(hooks_dir.join("badpkg")).unwrap();
        fs::create_dir_all(hooks_dir.join("goodpkg")).unwrap();
        fs::write(
            hooks_dir.join("badpkg/preInstall.sh"),
            "#!/usr/bin/env bash\nexit 1\n",
        )
        .unwrap();
        fs::write(
            hooks_dir.join("goodpkg/postInstall.sh"),
            "#!/usr/bin/env bash\necho ok > \"$MDPKG_DATA_DIR/good_post.txt\"\n",
        )
        .unwrap();

        let plan = SyncPlan {
            to_install: vec![
                PackageItem {
                    checked: true,
                    source: PackageSource::Repo,
                    explicit_m_prefix: false,
                    name: "badpkg".into(),
                    description: "".into(),
                    line_no: 1,
                },
                PackageItem {
                    checked: true,
                    source: PackageSource::Repo,
                    explicit_m_prefix: false,
                    name: "goodpkg".into(),
                    description: "".into(),
                    line_no: 2,
                },
            ],
            to_remove: vec![],
            skipped_installed_aur: vec![],
        };

        let mut backend = MockSystemBackend::default();
        let res = execute_sync_plan(
            &plan,
            &hooks_dir,
            "testhost",
            dir.path(),
            &mut backend,
            true,
            false,
        );
        assert!(res.is_err()); // overall run reports failure due to badpkg preInstall
        assert_eq!(backend.installed_repo, vec!["goodpkg".to_string()]);
        assert!(dir.path().join("good_post.txt").exists());
    }

    #[test]
    fn parse_npm_json_extracts_top_level_dependency_keys() {
        let sample =
            r#"{"dependencies":{"typescript":{"version":"5.6.2"},"eslint":{"version":"9.0.0"}}}"#;
        let pkgs = parse_npm_global_json(sample);
        assert!(pkgs.contains("typescript"));
        assert!(pkgs.contains("eslint"));
        assert_eq!(pkgs.len(), 2);
    }

    #[test]
    fn parse_dpkg_query_extracts_only_installed_packages() {
        let sample = "jq\tii \ntree\trc \ncurl\tii \n";
        let parsed = parse_dpkg_query_output(sample);
        assert!(parsed.contains("jq"));
        assert!(parsed.contains("curl"));
        assert!(!parsed.contains("tree"));
    }

    #[test]
    fn filter_apt_packages_to_hold_excludes_non_apt_already_held_and_empty() {
        let installed_apt: HashSet<String> =
            ["jq", "curl", "docker-ce"].into_iter().map(String::from).collect();
        let already_held: HashSet<String> = ["docker-ce"].into_iter().map(String::from).collect();
        let ignore = vec![
            "".to_string(),
            "   ".to_string(),
            "rustup".to_string(),     // custom / not in installed_apt
            "docker-ce".to_string(),  // already held by user
            "jq".to_string(),         // installed unheld APT package
            "jq".to_string(),         // duplicate
            "curl".to_string(),       // installed unheld APT package
        ];

        let to_hold = filter_apt_packages_to_hold(&ignore, &installed_apt, &already_held);
        assert_eq!(to_hold, vec!["jq".to_string(), "curl".to_string()]);
    }

    #[test]
    fn execute_sync_plan_runs_custom_install_and_remove_and_updates_custom_state() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        let pkg_dir = hooks_dir.join("mytool");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(
            pkg_dir.join("preInstall.sh"),
            "#!/usr/bin/env bash\necho pre >> \"$MDPKG_DATA_DIR/log.txt\"\n",
        )
        .unwrap();
        fs::write(
            pkg_dir.join("install.sh"),
            "#!/usr/bin/env bash\necho install >> \"$MDPKG_DATA_DIR/log.txt\"\n",
        )
        .unwrap();
        fs::write(
            pkg_dir.join("postInstall.sh"),
            "#!/usr/bin/env bash\necho post >> \"$MDPKG_DATA_DIR/log.txt\"\n",
        )
        .unwrap();
        fs::write(
            pkg_dir.join("remove.sh"),
            "#!/usr/bin/env bash\necho remove >> \"$MDPKG_DATA_DIR/log.txt\"\n",
        )
        .unwrap();

        let item = PackageItem {
            checked: true,
            source: PackageSource::Custom,
            explicit_m_prefix: false,
            name: "mytool".into(),
            description: "".into(),
            line_no: 1,
        };

        let install_plan = SyncPlan {
            to_install: vec![item.clone()],
            to_remove: vec![],
            skipped_installed_aur: vec![],
        };

        let mut backend = MockSystemBackend::default();
        execute_sync_plan(
            &install_plan,
            &hooks_dir,
            "testhost",
            dir.path(),
            &mut backend,
            true,
            false,
        )
        .unwrap();

        let custom_set = load_custom_installed(dir.path(), "testhost");
        assert!(custom_set.contains("mytool"));
        assert_eq!(
            fs::read_to_string(dir.path().join("log.txt")).unwrap(),
            "pre\ninstall\npost\n"
        );

        let remove_plan = SyncPlan {
            to_install: vec![],
            to_remove: vec![item],
            skipped_installed_aur: vec![],
        };
        execute_sync_plan(
            &remove_plan,
            &hooks_dir,
            "testhost",
            dir.path(),
            &mut backend,
            true,
            false,
        )
        .unwrap();
        let custom_set_after = load_custom_installed(dir.path(), "testhost");
        assert!(!custom_set_after.contains("mytool"));
    }

    #[test]
    fn update_custom_packages_respects_ignore_and_runs_update_or_install_fallback() {
        use crate::markdown::parse_document;

        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");

        // tool1 has update.sh; tool2 only has install.sh (fallback); tool3 is ignored
        for (pkg, stage, msg) in [
            ("tool1", "install.sh", "tool1-install"),
            ("tool1", "update.sh", "tool1-update"),
            ("tool2", "install.sh", "tool2-install"),
            ("tool3", "update.sh", "tool3-update"),
        ] {
            let pdir = hooks_dir.join(pkg);
            fs::create_dir_all(&pdir).unwrap();
            fs::write(
                pdir.join(stage),
                format!(
                    "#!/usr/bin/env bash\necho {} >> \"$MDPKG_DATA_DIR/update_log.txt\"\n",
                    msg
                ),
            )
            .unwrap();
        }

        let host_md = "\
## [x] MAIN
### [x] tools
- [x] c:`tool1` — tool 1
- [x] c:`tool2` — tool 2
- [x] c:`tool3` — tool 3
";
        let (host_doc, _) = parse_document(Path::new("testhost.md"), host_md);
        let mut installed = InstalledPackages::default();
        installed.custom.insert("tool1".to_string());
        installed.custom.insert("tool2".to_string());
        installed.custom.insert("tool3".to_string());

        let ignore = vec!["tool3".to_string()];
        update_custom_packages(
            &host_doc,
            &installed,
            &hooks_dir,
            "testhost",
            dir.path(),
            &ignore,
        )
        .unwrap();

        let log = fs::read_to_string(dir.path().join("update_log.txt")).unwrap();
        assert_eq!(log, "tool1-update\ntool2-install\n");
    }
}
