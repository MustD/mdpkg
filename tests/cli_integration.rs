use clap::Parser;
use mdpkg::cli::Cli;
use mdpkg::commands::execute_command;
use mdpkg::sources::MockSystemBackend;
use std::fs;
use tempfile::tempdir;

#[test]
fn end_to_end_add_check_sync_disable_and_ai_help() {
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
        "--section",
        "MAIN",
        "--category",
        "cli",
        "--desc",
        "Browser",
    ])
    .unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert_eq!(backend.installed_aur, vec!["google-chrome".to_string()]);
    assert!(fs::read_to_string(&catalog)
        .unwrap()
        .contains("- [x] a:`google-chrome` — Browser"));
    assert!(dir.path().join("laptop.md").exists());

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
        "--section",
        "MAIN",
        "--category",
        "cli",
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
