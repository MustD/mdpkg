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
    fs::write(&catalog, "# Packages\n\n## [x] MAIN — Core\n\n### [x] cli\n- [x] `git` — vcs\n").unwrap();

    let mut backend = MockSystemBackend::default();
    backend.installed.repo_and_aur.insert("git".to_string());

    // Adding git again should not create a duplicate in packages.md
    let cli = Cli::try_parse_from([
        "mdpkg", "--dir", dir.path().to_str().unwrap(), "--host", "laptop",
        "add", "git", "--source", "repo", "--section", "MAIN", "--category", "cli", "--desc", "vcs",
    ]).unwrap();
    execute_command(cli, &mut backend).unwrap();

    let content = fs::read_to_string(&catalog).unwrap();
    assert_eq!(content.matches("`git`").count(), 1);
}

#[test]
fn test_privileged_commands_trigger_backend_elevation() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("packages.md");
    fs::write(&catalog, "# Packages\n\n## [x] MAIN — Core\n\n### [x] cli\n- [x] `git` — vcs\n").unwrap();

    let mut backend = MockSystemBackend::default();

    // mdpkg update should trigger ensure_elevated
    let cli = Cli::try_parse_from([
        "mdpkg", "--dir", dir.path().to_str().unwrap(), "update", "--no-confirm",
    ]).unwrap();
    execute_command(cli, &mut backend).unwrap();
    assert!(backend.elevated);
}
