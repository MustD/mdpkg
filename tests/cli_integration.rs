use clap::Parser;
use mdpkg::cli::Cli;
use mdpkg::commands::execute_command;
use mdpkg::sources::MockSystemBackend;
use std::fs;
use tempfile::tempdir;

#[test]
fn end_to_end_add_check_sync_disable_and_ai_help() {
    std::env::set_var("MDPKG_NON_INTERACTIVE", "1");
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("packages.md");
    fs::write(
        &catalog,
        "# Packages\n\n## [x] MAIN — Core\n\n### [x] cli\n- [x] `git` — vcs\n",
    )
    .unwrap();

    let mut backend = MockSystemBackend::default();
    backend.installed.repo_and_aur.insert("git".to_string());

    // 1. Add an AUR package non-interactively
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "laptop",
        "add",
        "google-chrome",
        "--source",
        "a",
        "--desc",
        "Browser",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert_eq!(backend.installed_aur, vec!["google-chrome".to_string()]);
    let catalog_after_add = fs::read_to_string(&catalog).unwrap();
    assert!(catalog_after_add.contains("- a:`google-chrome` — Browser"));
    assert!(!catalog_after_add.contains("[x]"));
    assert!(!catalog_after_add.contains("[ ]"));
    assert!(dir.path().join("laptop.md").exists());
    assert!(fs::read_to_string(dir.path().join("laptop.md"))
        .unwrap()
        .contains("- [x] a:`google-chrome` — Browser"));

    // 2. Check command is read-only and succeeds on valid files
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "laptop",
        "check",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();

    // 3. Disable git on laptop -> unchecks in laptop.md and removes from system
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "laptop",
        "disable",
        "git",
        "--no-confirm",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert_eq!(backend.removed_repo_or_aur, vec!["git".to_string()]);
    assert!(fs::read_to_string(dir.path().join("laptop.md"))
        .unwrap()
        .contains("- [ ] `git` — vcs"));
}

#[test]
fn real_world_multi_section_legacy_aur_blockquotes_and_sync() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("packages.md");
    let real_world_md = "\
# Manjaro packages

> **Format legend**
> - format notes

## [x] MAIN — Core

### [x] CLI
- [x] `git` — vcs
- [x] Ⓐ`google-chrome` - browser
- [ ] n:`typescript` — ts

## [ ] CONTAINERS — Optional container setup

> Note: Enable docker service after installation

### [x] runtime
- [x] `docker` — container runtime
";
    fs::write(&catalog, real_world_md).unwrap();

    let mut backend = MockSystemBackend::default();
    backend.installed.repo_and_aur.insert("git".to_string());

    // Check command runs and succeeds
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "myhost",
        "check",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();

    // Sync dry-run doesn't create host file
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "myhost",
        "sync",
        "--dry-run",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert!(!dir.path().join("myhost.md").exists());

    // Sync creates host file, skips PRINTING (since ## [ ]), normalizes Ⓐ to a:
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "myhost",
        "sync",
        "--no-confirm",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert!(dir.path().join("myhost.md").exists());

    // Enable google-chrome on myhost -> installs via AUR and marks [x]
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "myhost",
        "enable",
        "google-chrome",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert_eq!(backend.installed_aur, vec!["google-chrome".to_string()]);

    let host_content = fs::read_to_string(dir.path().join("myhost.md")).unwrap();
    assert!(host_content.contains("- [x] a:`google-chrome` — browser"));
    assert!(!host_content.contains('Ⓐ'));
    assert!(host_content.contains("## [ ] CONTAINERS — Optional container setup"));
    assert!(host_content.contains("> Note: Enable docker service after installation"));
}

#[test]
fn test_add_existing_package_does_not_duplicate_in_catalog() {
    std::env::set_var("MDPKG_NON_INTERACTIVE", "1");
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("packages.md");
    fs::write(
        &catalog,
        "# Packages\n\n## [x] MAIN — Core\n\n### [x] cli\n- [x] `git` — vcs\n",
    )
    .unwrap();

    let mut backend = MockSystemBackend::default();
    backend.installed.repo_and_aur.insert("git".to_string());

    // Adding git again should not create a duplicate in packages.md
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "laptop",
        "add",
        "git",
        "--source",
        "repo",
        "--desc",
        "vcs",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();

    let content = fs::read_to_string(&catalog).unwrap();
    assert_eq!(content.matches("`git`").count(), 1);
}

#[test]
fn test_privileged_commands_trigger_backend_elevation() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("packages.md");
    fs::write(
        &catalog,
        "# Packages\n\n## [x] MAIN — Core\n\n### [x] cli\n- [x] `git` — vcs\n",
    )
    .unwrap();

    let mut backend = MockSystemBackend::default();

    // mdpkg update should trigger ensure_elevated
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "update",
        "--no-confirm",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert!(backend.elevated);
}

#[test]
fn test_config_set_dir_relative_resolves_to_absolute_and_works_without_dir_flag_from_other_callsite(
) {
    let dir = tempdir().unwrap();
    let config_file = dir.path().join("config.toml");
    let pkg_dir = dir.path().join("repo").join("mdpkg");
    let other_dir = dir.path().join("elsewhere");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::create_dir_all(&other_dir).unwrap();

    let catalog = pkg_dir.join("packages.md");
    fs::write(
        &catalog,
        "# Packages\n\n## [x] MAIN — Core\n\n### [x] cli\n- [x] `git` — vcs\n",
    )
    .unwrap();

    let mut backend = MockSystemBackend::default();
    backend.installed.repo_and_aur.insert("git".to_string());

    // Pass a relative path "." when running in pkg_dir, or a path with ".." segments
    let sub_dir = pkg_dir.join("sub");
    fs::create_dir_all(&sub_dir).unwrap();
    let uncanonical_path = sub_dir.join("..");
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--config",
        config_file.to_str().unwrap(),
        "config",
        "set-dir",
        uncanonical_path.to_str().unwrap(),
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();

    let saved_toml = fs::read_to_string(&config_file).unwrap();
    assert!(
        !saved_toml.contains(".."),
        "Expected canonicalized path without '..', got: {}",
        saved_toml
    );

    // Now invoke `mdpkg check` without `--dir` while our current process cwd is the crate root (not pkg_dir!)
    let cli_check = Cli::try_parse_from([
        "mdpkg",
        "--config",
        config_file.to_str().unwrap(),
        "--host",
        "laptop",
        "check",
    ])
    .unwrap();
    execute_command(cli_check, &mut backend).unwrap();
}

#[test]
fn test_missing_catalog_error_guides_user_to_config_set_dir() {
    let dir = tempdir().unwrap();
    let config_file = dir.path().join("nonexistent_config.toml");
    let empty_dir = dir.path().join("empty");
    fs::create_dir_all(&empty_dir).unwrap();

    let mut backend = MockSystemBackend::default();
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--config",
        config_file.to_str().unwrap(),
        "--dir",
        empty_dir.to_str().unwrap(),
        "check",
    ])
    .unwrap();

    let err = execute_command(cli, &mut backend).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("mdpkg config set-dir"),
        "Expected error message to hint `mdpkg config set-dir`, got: {}",
        msg
    );
}

#[test]
fn test_completions_subcommand_parses_and_generates_scripts_for_all_shells() {
    use clap_complete::Shell;
    use mdpkg::commands::generate_completions;

    for (shell_arg, shell_enum) in [
        ("bash", Shell::Bash),
        ("zsh", Shell::Zsh),
        ("fish", Shell::Fish),
        ("elvish", Shell::Elvish),
        ("powershell", Shell::PowerShell),
    ] {
        let cli = Cli::try_parse_from(["mdpkg", "completions", shell_arg]).unwrap();
        let mut backend = MockSystemBackend::default();
        execute_command(cli, &mut backend).unwrap();

        let mut buf = Vec::new();
        generate_completions(shell_enum, &mut buf);
        let script = String::from_utf8(buf).unwrap();
        assert!(
            !script.is_empty(),
            "Expected non-empty script for {}",
            shell_arg
        );
        assert!(
            script.contains("mdpkg") && script.contains("sync") && script.contains("completions"),
            "Expected subcommands in {} script, got: {}",
            shell_arg,
            script
        );
    }
}

#[test]
fn test_hooks_run_stage_validates_possible_values_for_completions() {
    assert!(Cli::try_parse_from(["mdpkg", "hooks", "run", "git", "postInstall"]).is_ok());
    assert!(Cli::try_parse_from(["mdpkg", "hooks", "run", "git", "invalidStage"]).is_err());
}

#[test]
fn test_section_checkbox_hierarchy_triggers_removal_and_install() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("packages.md");
    let host_file = dir.path().join("laptop.md");

    fs::write(
        &catalog,
        "# Packages\n\n## MAIN — Core\n\n### cli\n- `git` — vcs\n\n## PRINTING — Printers\n\n### printing stack\n- `manjaro-printer` — cups\n- `system-config-printer` — gtk\n",
    )
    .unwrap();

    // Unchecked ## [ ] PRINTING section in host file while child categories and items are [x]
    fs::write(
        &host_file,
        "# Packages\n\n## [x] MAIN — Core\n\n### [x] cli\n- [x] `git` — vcs\n\n## [ ] PRINTING — Printers\n\n### [x] printing stack\n- [x] `manjaro-printer` — cups\n- [x] `system-config-printer` — gtk\n",
    )
    .unwrap();

    let mut backend = MockSystemBackend::default();
    backend.installed.repo_and_aur.insert("git".to_string());
    backend
        .installed
        .repo_and_aur
        .insert("manjaro-printer".to_string());
    backend
        .installed
        .repo_and_aur
        .insert("system-config-printer".to_string());

    // 1. Sync with ## [ ] PRINTING unchecked -> removes installed printing packages
    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "laptop",
        "sync",
        "--no-confirm",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert_eq!(
        backend.removed_repo_or_aur,
        vec![
            "manjaro-printer".to_string(),
            "system-config-printer".to_string()
        ]
    );

    // 2. Now check ## [x] PRINTING in host file -> installs printing packages
    let updated_host = fs::read_to_string(&host_file)
        .unwrap()
        .replace("## [ ] PRINTING", "## [x] PRINTING");
    fs::write(&host_file, updated_host).unwrap();

    let cli = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "laptop",
        "sync",
        "--no-confirm",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert_eq!(
        backend.installed_repo,
        vec![
            "manjaro-printer".to_string(),
            "system-config-printer".to_string()
        ]
    );
}

#[test]
fn test_add_rejects_removed_section_and_category_flags_and_supports_apt_and_custom() {
    std::env::set_var("MDPKG_NON_INTERACTIVE", "1");
    // --section and --category must no longer exist on `mdpkg add`
    assert!(Cli::try_parse_from(["mdpkg", "add", "jq", "--section", "MAIN"]).is_err());
    assert!(Cli::try_parse_from(["mdpkg", "add", "jq", "--category", "cli"]).is_err());

    let dir = tempdir().unwrap();
    let catalog = dir.path().join("packages.md");
    fs::write(&catalog, "# Packages\n\n## MAIN — Core\n\n### cli\n- `git` — vcs\n").unwrap();

    let mut backend = MockSystemBackend::default();
    backend.installed.repo_and_aur.insert("git".to_string());

    // 1. Add an APT (d:) package
    let cli_apt = Cli::try_parse_from([
        "mdpkg", "--dir", dir.path().to_str().unwrap(), "--host", "ubuntu-box",
        "add", "jq", "--source", "d", "--desc", "JSON processor",
    ]).unwrap();
    execute_command(cli_apt, &mut backend).unwrap();
    assert_eq!(backend.installed_apt, vec!["jq".to_string()]);
    assert!(fs::read_to_string(&catalog).unwrap().contains("- d:`jq` — JSON processor"));

    // 2. Add a Custom (c:) package -> scaffolds install.sh and remove.sh in hooks/rustup/
    let cli_custom = Cli::try_parse_from([
        "mdpkg", "--dir", dir.path().to_str().unwrap(), "--host", "ubuntu-box",
        "add", "rustup", "--source", "c", "--desc", "Rust toolchain",
    ]).unwrap();
    execute_command(cli_custom, &mut backend).unwrap();
    assert!(dir.path().join("hooks/rustup/install.sh").exists());
    assert!(dir.path().join("hooks/rustup/remove.sh").exists());
    assert!(fs::read_to_string(&catalog).unwrap().contains("- c:`rustup` — Rust toolchain"));
}

#[test]
fn test_updates_alias_and_ignore_flag_and_failure_hint() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("packages.md");
    fs::write(&catalog, "# Packages\n\n## MAIN\n\n### cli\n- `git`\n").unwrap();

    let mut backend = MockSystemBackend::default();
    let cli = Cli::try_parse_from([
        "mdpkg", "--dir", dir.path().to_str().unwrap(),
        "updates", "--no-confirm", "--ignore", "broken-pkg,another-pkg",
    ]).unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert_eq!(backend.ignored_updates, vec!["broken-pkg".to_string(), "another-pkg".to_string()]);

    backend.fail_update = true;
    let cli_fail = Cli::try_parse_from([
        "mdpkg", "--dir", dir.path().to_str().unwrap(),
        "update", "--no-confirm",
    ]).unwrap();
    let err = execute_command(cli_fail, &mut backend).unwrap_err();
    assert!(err.to_string().contains("--ignore"), "Expected --ignore hint, got: {}", err);
}

#[test]
fn test_first_time_host_creation_persists_discovered_custom_packages_and_update_respects_ignore() {
    use mdpkg::sources::load_custom_installed;

    let dir = tempdir().unwrap();
    let catalog = dir.path().join("packages.md");
    fs::write(
        &catalog,
        "# Packages\n\n## MAIN — Core\n\n### cli\n- c:`sh` — POSIX shell\n",
    )
    .unwrap();

    let sh_hooks = dir.path().join("hooks/sh");
    fs::create_dir_all(&sh_hooks).unwrap();
    fs::write(
        sh_hooks.join("install.sh"),
        "#!/usr/bin/env bash\necho install >> \"$MDPKG_DATA_DIR/sh_hook.log\"\n",
    )
    .unwrap();
    fs::write(
        sh_hooks.join("remove.sh"),
        "#!/usr/bin/env bash\necho remove >> \"$MDPKG_DATA_DIR/sh_hook.log\"\n",
    )
    .unwrap();
    fs::write(
        sh_hooks.join("update.sh"),
        "#!/usr/bin/env bash\necho \"update:$MDPKG_SOURCE\" >> \"$MDPKG_DATA_DIR/sh_hook.log\"\n",
    )
    .unwrap();

    let mut backend = MockSystemBackend::default();

    // 1. First-time `mdpkg sync` discovers `sh` via `command -v`, creates `box1.md` with `- [x] c:`sh``,
    //    and persists `sh` in `.mdpkg/installed_custom_box1.json`.
    let cli_sync = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "box1",
        "sync",
        "--no-confirm",
    ])
    .unwrap();
    execute_command(cli_sync, &mut backend).unwrap();

    let host_content = fs::read_to_string(dir.path().join("box1.md")).unwrap();
    assert!(host_content.contains("- [x] c:`sh` — POSIX shell"));
    let persisted = load_custom_installed(dir.path(), "box1");
    assert!(
        persisted.contains("sh"),
        "Expected 'sh' to be persisted in installed_custom_box1.json"
    );

    // 2. Subsequent `mdpkg check --strict` succeeds with 0 drift!
    let cli_check = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "box1",
        "check",
        "--strict",
    ])
    .unwrap();
    execute_command(cli_check, &mut backend).unwrap();

    // 3. `mdpkg update --no-confirm` executes `hooks/sh/update.sh`
    let log_path = dir.path().join("sh_hook.log");
    assert!(!log_path.exists());
    let cli_update = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "box1",
        "update",
        "--no-confirm",
    ])
    .unwrap();
    execute_command(cli_update, &mut backend).unwrap();
    assert_eq!(fs::read_to_string(&log_path).unwrap(), "update:custom\n");

    // 4. `mdpkg update --no-confirm --ignore sh` skips `hooks/sh/update.sh`
    let cli_update_ignore = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "box1",
        "update",
        "--no-confirm",
        "--ignore",
        "sh",
    ])
    .unwrap();
    execute_command(cli_update_ignore, &mut backend).unwrap();
    assert_eq!(fs::read_to_string(&log_path).unwrap(), "update:custom\n");

    // 5. `mdpkg hooks run sh update` resolves source `custom` from `packages.md`
    let cli_hooks_run = Cli::try_parse_from([
        "mdpkg",
        "--dir",
        dir.path().to_str().unwrap(),
        "--host",
        "box1",
        "hooks",
        "run",
        "sh",
        "update",
    ])
    .unwrap();
    execute_command(cli_hooks_run, &mut backend).unwrap();
    assert_eq!(
        fs::read_to_string(&log_path).unwrap(),
        "update:custom\nupdate:custom\n"
    );
}


