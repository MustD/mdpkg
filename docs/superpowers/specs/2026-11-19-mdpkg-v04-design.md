# `mdpkg` v0.4 — Interactive Add, Package-Grouped Hooks, Custom Script Packages, Ignore Updates, & APT/Docker Support

**Date:** 2026-11-19  
**Status:** Draft (Pending Review & Approval)

---

## 1. Intent & Why

Building on `mdpkg` v0.3, six items from `todo.md` were evaluated to streamline multi-host package management across Manjaro and Debian/Ubuntu systems:
1. **Interactive Section & Category Selection in `mdpkg add`:** Removing the `--section` and `--category` CLI flags in favor of an always-guided interactive selection from existing sections/categories in `packages.md` (with an option to create a new section or category on the fly).
2. **Ignoring Problematic Packages During Updates (`mdpkg update --ignore <pkg>`):** Allowing users to bypass broken package upgrades via `--ignore <pkg>`, and automatically hinting this command whenever `mdpkg update` terminates with an error.
3. **Research on `update --no-confirm`:** Verified that `pamac update -a --force-refresh --no-confirm` natively bypasses all confirmation messages for both Manjaro official repository packages and AUR builds. This requirement is already fulfilled by `--no-confirm` and requires no new flag.
4. **Package-Grouped Hook Directories (`hooks/<pkg>/<stage>.sh`):** Replacing the flat `hooks/<pkg>_<stage>.sh` layout with per-package subdirectories `hooks/<pkg>/<stage>` so all scripts for a package live cleanly in one folder.
5. **Custom Script / Curl Package Support (`c:` prefix):** Supporting packages installed and updated via custom bash/python scripts stored directly in `hooks/<pkg>/` (`install.sh`, `remove.sh`, and optional `update.sh`), with full support for `preInstall`, `postInstall`, `preRemoval`, and `postRemoval` lifecycle hooks. When adding a custom package via `mdpkg add`, `mdpkg` scaffolds `install.sh` and `remove.sh` in `hooks/<pkg>/` and prompts the user to fill them in.
6. **APT (`d:` prefix for Debian/Ubuntu) Support & Docker Integration Tests:** Adding an explicit `d:` (`PackageSource::Apt`) package source so all 5 sources are clearly differentiable (`m:` Manjaro repos, `a:` AUR, `d:` APT Debian/Ubuntu, `n:` npm, `c:` custom script), executing `apt` directly (instead of `apt-get`), and verifying with automated Docker test suites on Ubuntu and Manjaro/Arch containers.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
1. **Interactive Section & Category Picker in `mdpkg add`:**
   - Remove `--section` and `--category` flags from `AddArgs`.
   - In an interactive terminal, prompt the user to pick a section from existing sections in `packages.md` or `+ Create new section...`, then pick a category from existing categories in that section or `+ Create new category...`.
2. **Grouped Hooks Directory Structure (`hooks/<pkg>/<stage>`):**
   - Switch completely from flat `hooks/<pkg>_<stage>.*` files to per-package directories: `hooks/<pkg>/<stage>` or `hooks/<pkg>/<stage>.<ext>` (where `.sh` and `.py` remain supported even if non-executable).
3. **Custom Script Packages (`PackageSource::Custom`, prefix `c:`):**
   - Parse and render `c:` (aliases `custom:`) in `packages.md` and `<hostname>.md`.
   - Support lifecycle & action scripts inside `hooks/<pkg>/`:
     - `install.sh` (or `install`, `install.py`) — installs the custom package.
     - `remove.sh` (or `remove`, `remove.py`) — uninstalls the custom package.
     - `update.sh` (or `update`, `update.py`) — optional script run during `mdpkg update` for installed custom packages; if absent, `install.sh` is re-run during `mdpkg update` (unless skipped or ignored).
     - `preInstall`, `postInstall`, `preRemoval`, `postRemoval` — run around custom package installation and removal identically to repo/AUR/npm packages.
   - During `mdpkg add <pkg>` when Custom source (`c`) is selected: create `hooks/<pkg>/install.sh` and `hooks/<pkg>/remove.sh` with `#!/usr/bin/env bash` boilerplate, skip immediate execution of the empty script, and print clear instructions prompting the user to populate `install.sh` and `remove.sh`.
4. **Update Ignore Flag & Failure Hint (`mdpkg update --ignore <pkg>`):**
   - Accept `--ignore <PACKAGE>` (comma-separated or repeatable) on `mdpkg update` (and add `updates` as a visible subcommand alias for `update`).
   - Pass ignored packages to `pamac update --ignore <pkgs>`, hold/exclude them on `apt`, and skip them during custom package updates.
   - Whenever `mdpkg update` fails, append a clear hint to the error message suggesting: `Tip: You can ignore a failing package with: mdpkg update --ignore <package_name>`.
5. **Explicit 5-Source Model (`m:`, `a:`, `d:`, `n:`, `c:`) & Docker Tests:**
   - Clearly differentiate package sources in Markdown and CLI:
     - `m:` (or unprefixed default on Manjaro) — Manjaro repositories (`PackageSource::Repo`)
     - `a:` — AUR (`PackageSource::Aur`)
     - `d:` — APT for Ubuntu / Debian (`PackageSource::Apt`)
     - `n:` — Global npm (`PackageSource::Npm`)
     - `c:` — Custom script (`PackageSource::Custom`)
   - Use the `apt` CLI command (`sudo apt update`, `sudo apt install`, `sudo apt remove`, `sudo apt upgrade`, `sudo apt autoremove`) instead of `apt-get`.
   - Provide a Docker test suite testing `mdpkg` end-to-end inside `ubuntu:24.04` and `manjarolinux/base` (or `archlinux:latest`).

### Non-Goals
- **Backwards compatibility with flat `hooks/<pkg>_<stage>.sh`:** Explicitly dropped in favor of a clean cutover to `hooks/<pkg>/<stage>.sh`.
- **Adding a separate `--confirm` flag to `mdpkg update`:** Research confirmed `--no-confirm` already suppresses both Manjaro repo and AUR prompts in `pamac update -a`.
- **AUR builds on Ubuntu/APT hosts:** If a host checklist enables an `a:` (AUR) package on an `apt`-based system, `mdpkg` logs a clear warning and skips AUR operations rather than attempting to emulate `makepkg` on Debian/Ubuntu.

### Anti-Goals
- **Executing empty scaffolded custom scripts immediately on `mdpkg add`:** When a user adds a `c:` package, `install.sh` starts as a template stub; `mdpkg add` must prompt the user to fill it in rather than falsely marking it installed by running a no-op script.
- **Writing Manjaro Polkit rules on Ubuntu/Debian systems:** On `apt` systems, `apt-get` runs directly under `sudo`; `PrivilegeSession` must not fail if `/etc/polkit-1/rules.d` is absent or unused.

---

## 3. Detailed Feature Specifications

### 3.1 `mdpkg add` Interactive Section, Category, & 5-Source Selection
- **Explicit 5 Package Sources (`PackageSource` & `CliPackageSource`):**
  - `Repo` (`m:` or no prefix in Markdown; CLI `--source m`, aliases `repo`, `manjaro`) — Manjaro repositories
  - `Aur` (`a:` in Markdown; CLI `--source a`, alias `aur`) — Arch User Repository
  - `Apt` (`d:` or `apt:` in Markdown, normalized to `d:`; CLI `--source d`, aliases `apt`, `debian`, `ubuntu`) — Ubuntu / Debian `apt` repositories
  - `Npm` (`n:` in Markdown; CLI `--source n`, alias `npm`) — Global npm packages
  - `Custom` (`c:` or `custom:` in Markdown, normalized to `c:`; CLI `--source c`, alias `custom`) — Custom script packages
- **CLI Changes:**
  - Remove `--section` and `--category` from `AddArgs` in `src/cli.rs`.
- **Interactive Flow (when `atty_check()` is true):**
  1. **Source Prompt (if `--source` omitted):**
     - `1. Manjaro Repository (m:)`
     - `2. AUR (a:)`
     - `3. APT Ubuntu/Debian (d:)`
     - `4. Global npm (n:)`
     - `5. Custom script (c:)`
  2. **Section Prompt:**
     - List all existing section names from `packages.md` (e.g., `MAIN — Basic packages`, `DEV — Development`), followed by `+ Create new section...`.
     - If `+ Create new section...` is chosen (or if `packages.md` has no sections yet), prompt with `Input::<String>` for the new section name (and optional section description).
  3. **Category Prompt:**
     - For the selected section, list all existing named categories (`### <name>`), followed by `+ Create new category...`.
     - If `+ Create new category...` is chosen (or if the section has no named categories), prompt with `Input::<String>` for the new category name.
  4. **Description Prompt (if `--desc` omitted):**
     - `Input::<String>` for optional short description.
- **Non-Interactive Fallback (**Builder's call**):**
  - When `atty_check()` is false (e.g. automated tests or `MDPKG_NON_INTERACTIVE=1`), default to the first existing section in `packages.md` (or `"MAIN"`) and its first existing category (or `"general"`).

### 3.2 Grouped Hooks Directory Layout (`hooks/<pkg>/<stage>`)
- **Directory Structure:**
  ```text
  <data_dir>/hooks/
  ├── README.md
  └── <package_name>/
      ├── preInstall.sh
      ├── postInstall.sh
      ├── preRemoval.sh
      ├── postRemoval.sh
      ├── install.sh       # Used by c:<package_name>
      ├── remove.sh        # Used by c:<package_name>
      └── update.sh        # Optional custom update script for c:<package_name>
  ```
- **Stage Names (`HookStage`):**
  - Standard lifecycle stages (valid for all package sources): `preInstall`, `postInstall`, `preRemoval`, `postRemoval`.
  - Custom package action stages (valid for `c:` custom packages): `install`, `remove`, `update`.
- **Discovery Rules (`discover_hooks`):**
  - Scan subdirectories of `<data_dir>/hooks/`. Each subdirectory name is `<package_name>` (ignoring hidden directories starting with `.`).
  - Inside `hooks/<package_name>/`, match files whose stem (filename before `.` extension, or full filename if extensionless) is a valid stage (`preInstall`, `postInstall`, `preRemoval`, `postRemoval`, `install`, `remove`, `update`).
  - Ignore `README.md` and hidden dotfiles.
- **Validation Rules (`validate_hooks`):**
  - Any regular file placed directly at top-level `hooks/<file>` (other than `hooks/README.md` or dotfiles) is reported as a validation issue: `"Hook scripts must be placed in a package directory: hooks/<pkg>/<stage>.sh"`.
  - If `hooks/<pkg>/` does not match any package in `packages.md`, report an orphan hook directory/script validation issue.
  - Inside `hooks/<pkg>/`, any file whose stem is not one of the recognized stages (`preInstall`, `postInstall`, `preRemoval`, `postRemoval`, `install`, `remove`, `update`) or that is neither executable (`chmod +x`) nor `.sh`/`.py` is reported as a validation issue.
  - For every package with source `PackageSource::Custom` (`c:`) in `packages.md`, `validate_hooks` verifies that `hooks/<pkg>/install*` and `hooks/<pkg>/remove*` exist and are non-empty (contain at least one non-comment, non-whitespace command line beyond the stub marker). (**Builder's call:** `mdpkg check` warns or flags if a `c:` package still has an unedited stub script.)

### 3.3 Custom Script Package Support (`c:<pkg>`)
- **Markdown Syntax:**
  - Prefix `c:` (or `custom:` normalized to `c:` on render):
    ```markdown
    - [x] c:`rustup` — Rust toolchain installer
    ```
- **Scaffolding on `mdpkg add <pkg> --source c` (or interactive option 4):**
  - Creates directory `<data_dir>/hooks/<pkg>/`.
  - If `install.sh` does not already exist, writes `<data_dir>/hooks/<pkg>/install.sh` (with mode `0o755`):
    ```bash
    #!/usr/bin/env bash
    set -euo pipefail
    # TODO: Add installation commands for <pkg> (e.g. curl -fsSL ... | sh)
    ```
  - If `remove.sh` does not already exist, writes `<data_dir>/hooks/<pkg>/remove.sh` (with mode `0o755`):
    ```bash
    #!/usr/bin/env bash
    set -euo pipefail
    # TODO: Add removal commands for <pkg>
    ```
  - Adds `- c:`<pkg>`` to `packages.md` and `- [x] c:`<pkg>`` to `<hostname>.md`.
  - Does **not** execute `install.sh` during `mdpkg add` when newly scaffolded; instead prints:
    ```text
    Created custom script templates in <data_dir>/hooks/<pkg>/:
      - install: <data_dir>/hooks/<pkg>/install.sh
      - remove:  <data_dir>/hooks/<pkg>/remove.sh
    Please fill in install.sh and remove.sh, then run `mdpkg sync` to install.
    ```
- **Installed State Tracking for Custom Packages (**Builder's call**):**
  - Because a custom script (`c:my-tool`) might install a binary with a different name, a library, or dotfiles, relying solely on `which <pkg>` can be inaccurate.
  - `mdpkg` tracks installed custom packages via a state file `<data_dir>/.mdpkg/installed_custom_<hostname>.json` (or `.txt` list of installed `c:` package names per host), combined with checking `command -v <pkg>` when reconciling a brand-new host for the first time:
    - When `hooks/<pkg>/install*` succeeds on `<hostname>`, `<pkg>` is recorded as installed for `<hostname>`.
    - When `hooks/<pkg>/remove*` succeeds on `<hostname>`, `<pkg>` is removed from the installed record for `<hostname>`.
- **Lifecycle Execution during `mdpkg sync`, `enable`, `disable`, `remove`:**
  - **Installing `c:<pkg>`:**
    1. Run `hooks/<pkg>/preInstall*` (if present).
    2. Run `hooks/<pkg>/install*` (required; fails if missing or if it only contains comments/stub `# TODO:`).
    3. Mark `<pkg>` installed in custom state tracker.
    4. Run `hooks/<pkg>/postInstall*` (if present).
  - **Removing `c:<pkg>`:**
    1. Run `hooks/<pkg>/preRemoval*` (if present).
    2. Run `hooks/<pkg>/remove*` (required; fails if missing).
    3. Unmark `<pkg>` from custom state tracker.
    4. Run `hooks/<pkg>/postRemoval*` (if present).
- **Updating Custom Packages during `mdpkg update`:**
  - After system repo/AUR and npm updates, `mdpkg update` checks all enabled & installed `c:` packages for the active host (excluding any passed via `--ignore`):
    - For each `c:<pkg>`:
      1. Run `hooks/<pkg>/preInstall*` (if present).
      2. If `hooks/<pkg>/update*` exists, execute `update*`; otherwise if `hooks/<pkg>/install*` exists, re-execute `install*` (since most `curl | sh` installers are idempotent self-updaters).
      3. Run `hooks/<pkg>/postInstall*` (if present).

### 3.4 `mdpkg update --ignore <pkg>` & Error Suggestion
- **CLI Changes:**
  - On `Commands::Update`, add `#[command(visible_alias = "updates")]`.
  - In `UpdateArgs`:
    ```rust
    #[arg(
        long,
        value_delimiter = ',',
        help = "Ignore specific package(s) during update (comma-separated or repeatable)"
    )]
    pub ignore: Vec<String>,
    ```
- **Execution Behavior:**
  - **On Manjaro (`pamac`):** If `ignore` is non-empty, pass `--ignore <pkg1,pkg2,...>` to `pamac update -a --force-refresh`.
  - **On Ubuntu/Debian (`apt`):** If `ignore` is non-empty, temporarily mark ignored packages on hold (`sudo apt-mark hold <pkgs...>`) before `sudo apt-get upgrade` and unhold them afterward (`sudo apt-mark unhold <pkgs...>`) via a drop guard, or filter them out.
  - **On Custom scripts (`c:`):** Skip running update/install scripts for any package in `ignore`.
- **Error Hint:**
  - In `handle_update`, if the update returns `Err(e)`, format/wrap the error with:
    ```text
    Tip: If a specific package caused the update to fail, try running:
      mdpkg update --ignore <package_name>
    ```

### 3.5 APT (`d:`) Support & Docker Integration Tests
- **Source Separation & Execution:**
  - `InstalledPackages` tracks installed sets per source domain:
    - `repo_and_aur: HashSet<String>` (queried via `pacman -Qq` when `pacman` is available)
    - `apt: HashSet<String>` (queried via `dpkg-query -W -f='${Package}\t${db:Status-Abbrev}\n'` filtering for `ii ` when `dpkg-query` is available)
    - `npm: HashSet<String>` (queried via `npm list -g --depth=0 --json`)
    - `custom: HashSet<String>` (loaded from `<data_dir>/.mdpkg/installed_custom_<hostname>.json`, or `command -v <pkg>` fallback for new host discovery)
  - **`m:` (`PackageSource::Repo`)** runs via `pamac install --no-upgrade` / `pamac remove` (or `sudo pacman -S --needed` / `sudo pacman -Rns` if `pamac` is not installed).
  - **`a:` (`PackageSource::Aur`)** runs via `pamac build` / `pamac remove`.
  - **`d:` (`PackageSource::Apt`)** runs via `apt`:
    - **Install (`install_apt`):** `sudo DEBIAN_FRONTEND=noninteractive apt install [-y] <pkgs...>` (`-y` passed when `no_confirm` is true).
    - **Remove (`remove_apt`):** `sudo DEBIAN_FRONTEND=noninteractive apt remove [-y] <pkgs...>`.
  - **System Update (`mdpkg update`):**
    - On Manjaro/Arch (`pamac` / `pacman` present):
      - Step 1: `pamac clean --build-files --no-confirm`
      - Step 2: `pamac update -a --force-refresh [--no-confirm] [--ignore <pkg1,pkg2>]`
      - Step 3 (if `!skip_orphans`): `pamac remove --orphans --no-confirm`
    - On Ubuntu/Debian (`apt` present):
      - Step 1: `sudo apt update`
      - Step 2: If `--ignore` is non-empty, hold ignored packages via `sudo apt-mark hold <pkgs...>`, run `sudo DEBIAN_FRONTEND=noninteractive apt upgrade [-y]`, then unhold via `sudo apt-mark unhold <pkgs...>`.
      - Step 3 (if `!skip_orphans`): `sudo DEBIAN_FRONTEND=noninteractive apt autoremove -y`
    - Step 4 (all systems, if `!skip_npm`): `sudo npm update -g`
    - Step 5 (all systems): Run `update` (or `install`) script for all installed `c:` custom packages (except those listed in `--ignore`).
- **Privilege Session on Non-Polkit / Container Environments:**
  - If running as `root` already (`geteuid() == 0`, common in Docker containers), `PrivilegeSession::acquire()` succeeds immediately without calling `sudo -v` or writing Polkit rules.
  - On Ubuntu/Debian (or when `/etc/polkit-1/rules.d` is absent or `pamac` is not used), skip writing the Manjaro Polkit rule.
- **Docker Integration Test Suite (`tests/docker/`):**
  - Provide Dockerfiles and an automated test runner script (`tests/docker/run_docker_tests.sh` plus an ignored Cargo test `cargo test --test docker_integration -- --ignored`):
    1. **Ubuntu (`ubuntu:24.04`):** Tests `mdpkg add --source d`, `mdpkg sync` (installing and removing real `d:` `apt` packages like `jq` or `tree`, plus a custom `c:` script package and `hooks/<pkg>/postInstall.sh`), `mdpkg check`, and `mdpkg update --ignore ... --no-confirm`.
    2. **Manjaro / Arch (`manjarolinux/base` with fallback to `archlinux:latest`):** Tests `m:` repo package sync, `hooks/<pkg>/`, custom `c:` scripts, and `mdpkg update --ignore ... --no-confirm`.

---

## 4. Updated CLI Summary

```text
mdpkg sync [SECTIONS...] [--all] [--host <HOST>] [--force-aur] [--dry-run] [--no-confirm]
mdpkg update [--no-confirm] [--skip-npm] [--skip-orphans] [--ignore <PKG>...]   # Alias: mdpkg updates
mdpkg add <PACKAGE> [--source m|a|d|n|c|repo|aur|apt|npm|custom] [--desc <DESC>] [--no-install]
mdpkg enable <PACKAGE> [--host <HOST>]
mdpkg disable <PACKAGE> [--host <HOST>] [--no-confirm]
mdpkg remove <PACKAGE> [--purge-catalog] [--host <HOST>] [--no-confirm]
mdpkg check [--host <HOST>] [--all-hosts] [--strict]
mdpkg list [--sections] [--hooks] [--diff]
mdpkg hooks init
mdpkg hooks list
mdpkg hooks run <PACKAGE> <STAGE>
mdpkg config set-dir <PATH>
mdpkg config show
mdpkg ai-help
```

---

## 5. What's Left to the Builder

- Internal refactor of `SystemBackend` to pass `paths: &ResolvedPaths` (or custom package update hooks) into `handle_update` so custom `c:` packages are updated alongside system packages.
- Exact JSON/text schema for `.mdpkg/custom_installed_<host>.json` tracking installed custom script packages.
- Exact `dialoguer::Select` and `dialoguer::Input` prompts and unit-testable helper functions for section/category selection.
- Docker test harness script and container caching so local Docker runs complete quickly.
