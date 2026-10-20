# `mdpkg` — Markdown-Driven Package & Host Manager for Manjaro

**Date:** 2026-10-20  
**Status:** Draft (Pending Review & Approval)

---

## 1. Intent & Why

Users maintaining multiple Manjaro Linux machines often keep their personal package configuration in a synced folder (e.g., Nextcloud, Syncthing, Google Drive, or a Git repository at `~/sync/packages`). Previously, this workflow required maintaining separate update scripts, install scripts, checklists, and manual setup notes.

Two main pain points drove `mdpkg`:
1. **Unattended System & Package Updates:** Updating repo packages, resetting AUR build files, updating AUR packages, removing orphans, and updating global `npm` packages previously required typing root/polkit passwords multiple times mid-run.
2. **Multi-Host Declarative Sync via Markdown:** Manually copying package names between machines or editing a single list without per-host state or uninstall support was tedious. The user wants a human- and AI-editable Markdown catalog (`packages.md`) paired with per-host Markdown checklists (`<hostname>.md`) where checking `[x]` installs a package, unchecking `[ ]` removes it, and new packages added on one host automatically sync to other hosts with safe defaults.

`mdpkg` replaces those scripts with a single open-source Rust CLI tool, packaged for release on the AUR.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
1. **Master Known Package List (`packages.md`):** Maintain a human- and AI-editable Markdown catalog of personal packages with sections (`##`), categories (`###`), keyboard-friendly ASCII package source prefixes (Manjaro Repo default or `m:`, AUR `a:`, Global NPM `n:`, plus backwards-compatible parsing of legacy `Ⓐ`), comments, and descriptions.
2. **Multi-Host Checklists (`<hostname>.md`):** Support per-host Markdown files derived from `packages.md`. Each host file tracks enabled (`[x]`) and disabled (`[ ]`) packages for that machine.
3. **Safe Bidirectional Catalog ↔ Host Sync:**
   - Reconcile `<hostname>.md` with `packages.md` whenever syncing or inspecting.
   - Structural changes in `packages.md` (AI regrouping categories, edited descriptions, added/removed comments) propagate into `<hostname>.md` while preserving existing per-host checkbox states.
   - Any package newly present in `packages.md` (or all packages when initializing `<hostname>.md` on a host for the first time) defaults its checkbox state in `<hostname>.md` to the **actual installed state on the current machine** (`[x]` if currently installed, `[ ]` if not).
4. **Full Desired-State Enforcement (Install & Remove):**
   - Because `packages.md` only tracks personal packages (never base system packages), `<hostname>.md` defines the exact desired state for all catalog packages on that host:
     - Effective `[x]` (item is `[x]` and its parent category `###` is enabled) + not installed → **install**.
     - Effective `[ ]` (item is `[ ]` or its parent category `### [ ]` is disabled) + currently installed → **uninstall/remove**.
5. **Interactive/CLI Package Addition (`mdpkg add <pkg>`):**
   - Prompt for source (Manjaro repo, AUR, or global `npm`) if not passed via flag, prompt to select an existing section/category from `packages.md` (or create a new one), prompt for an optional description, install the package on the current host, add it to `packages.md`, and mark it `[x]` in `<hostname>.md`.
6. **Single-Password Full System Update (`mdpkg update`):**
   - Acquire and keep alive `sudo` credentials upfront so the user only enters their password once.
   - Clean/reset AUR build files, force-refresh databases and update repo + AUR packages, clean orphaned packages, and update global `npm` packages.
7. **Lifecycle Hooks & AI-Friendly Hook Initialization (`mdpkg hooks init`):**
   - Initialize a `hooks/` directory containing a comprehensive `README.md` (written for both humans and AI assistants) explaining how to create executable hooks named `<toolName>_preInstall`, `<toolName>_postInstall`, `<toolName>_preRemoval`, and `<toolName>_postRemoval`, with recommended examples (e.g., `ripgrep`/`fzf`/`zoxide`/`mise` shell integration, system service configuration).
   - Execute matching hooks automatically during package install/remove lifecycle events.
8. **Configuration, Syntax Validation, & Drift Inspection (`mdpkg check` / `mdpkg status` / `mdpkg list`):**
   - Validate `packages.md`, `<hostname>.md`, and `hooks/` for broken Markdown syntax, duplicate entries, or orphaned hooks with exact `file:line` error reporting (essential after human or AI edits).
   - Clearly report pending actions: catalog-to-host drift (whether `<hostname>.md` needs syncing from `packages.md`), host-to-system drift (packages to install/remove), and attached pre/post hooks.
9. **Rich AI-Agent-Friendly CLI Help (`--help` & `mdpkg ai-help`):**
   - Provide comprehensive, self-documenting `--help` output on the root command and every subcommand (including concrete examples, Markdown format specification, source prefixes `m:`/`a:`/`n:`, and file reconciliation rules) plus a dedicated `mdpkg ai-help` subcommand that prints a complete, structured Markdown reference manual with resolved local paths so any AI agent can immediately understand and operate `mdpkg` via CLI or file edits.
10. **Open-Source & AUR Ready:**
   - Licensed under MIT with a complete `PKGBUILD` and documentation for publishing to the AUR.

### Non-Goals (Deferred for Future Versions)
- **`mise` and `curl+bash` custom script package sources:** Deferred by the user for a later iteration. However, the internal Rust `PackageSource` trait/enum must be designed cleanly so adding `mise` and custom script sources later requires no architectural rework.
- **Base system management:** `mdpkg` does not manage base Manjaro OS packages that are not listed in `packages.md`.
- **Full TUI/GUI editor:** Checkbox toggling is done in standard Markdown editors (Obsidian, Zed, Neovim, GitHub) or by AI assistants, plus CLI commands (`mdpkg add`, `mdpkg enable`, `mdpkg disable` / `mdpkg remove`).

### Anti-Goals (What Would Make This a Failure)
- **Repeated password prompts during `mdpkg update` or `mdpkg sync`:** The tool must not force the user to sit and re-enter root/polkit passwords at multiple stages.
- **Uninstalling unlisted system packages:** `mdpkg` must *never* touch or remove packages installed on the system that are not explicitly listed in `packages.md` / `<hostname>.md`.
- **Clobbering AI/human formatting or host state:** Syncing `packages.md` into `<hostname>.md` must not lose a host's `[x]`/`[ ]` selections when an AI reorganizes categories or edits descriptions in `packages.md`.
- **Rebuilding already-installed AUR packages during normal sync:** Unless `--force-aur` is passed, `mdpkg` must skip AUR packages that are already installed during `sync`.

---

## 3. Markdown Format & File Layout

### 3.1 Data Directory Layout
```text
<data_dir>/                   # e.g. ~/sync/packages
├── packages.md               # Master known-packages catalog (sections, categories, descriptions)
├── <hostname>.md             # Per-host package checklist (e.g. laptop.md, desktop.md)
└── hooks/                    # Created by `mdpkg hooks init`
    ├── README.md             # Human + AI instructions and recommended hook templates
    ├── <pkg>_preInstall*     # Executable hook script (e.g. .sh, .py, or no extension)
    ├── <pkg>_postInstall*
    ├── <pkg>_preRemoval*
    └── <pkg>_postRemoval*
```

### 3.2 Markdown Syntax (`packages.md` & `<hostname>.md`)
Easy to type on any keyboard and backwards-compatible with the user's existing `packages.md`:

```markdown
# Manjaro packages

> **Format legend**
> - `- [x]` enabled on host · `- [ ]` disabled (uninstalled if present on host)
> - `### [x] category` category toggle · `## [x] SECTION` section toggle (gates `--all`)
> - Manjaro repo package: no prefix or `m:` — `` - [x] `git` — description `` or `` - [x] m:`git` — description ``
> - AUR package: `a:` prefix — `` - [x] a:`google-chrome` — description `` (legacy `Ⓐ` also accepted and normalized to `a:`)
> - Global npm package: `n:` prefix — `` - [x] n:`typescript` — description ``

## [x] MAIN — Basic packages — the core set for a fresh system (init).

### [x] core CLI / shell
- [x] `git` — version control
- [x] m:`neovim` — modern Vim-based text editor

### [x] browsers / sync
- [x] a:`google-chrome` — Chrome browser

### [x] web / node globals
- [x] n:`typescript` — TypeScript compiler
```

#### Parsing & Semantics Rules
1. **Headings:**
   - `# ...` — Document title (preserved).
   - `## [x| ] <SECTION_NAME> — <description>` — Section heading. Checkbox is optional (no checkbox = `[x]`). In `<hostname>.md`, the section checkbox gates `--all` (default sync scope), while explicitly naming a section (`mdpkg sync PRINTING`) processes that section regardless of its `## [ ]` box.
   - `### [x| ] <category name>` — Category heading. Checkbox is optional (no checkbox = `[x]`). If a category is `### [ ]` in `<hostname>.md`, all packages inside it have an **effective desired state of disabled (`[ ]`)**, while preserving their individual `- [x]` checkboxes on disk so re-enabling the category restores the selection.
2. **Package Items & Source Prefixes:**
   - Pattern: `^\s*-\s*(?:\[([ xX])\]\s+)?(?:([a-zA-Z0-9_-]+):|(Ⓐ))?\s*`([^`]+)`(?:\s*(?:—|--|-)\s*(.*))?$`
   - Recognized source prefixes before the opening backtick:
     - Omitted (no prefix) or `m:` (or `manjaro:` / `repo:`) → **Manjaro Repo** (`PackageSource::Repo`).
     - `a:` (or `aur:` or legacy `Ⓐ`) → **AUR** (`PackageSource::Aur`). When writing/formatting `<hostname>.md` (or updating `packages.md`), `mdpkg` normalizes AUR items to `a:` and Manjaro repo items to no prefix (or preserves `m:` if explicitly specified).
     - `n:` (or `npm:`) → **Global NPM** (`PackageSource::Npm`).
     - Any other `<prefix>:` is flagged by `mdpkg check` as an unknown/unsupported source prefix (making future extensions like `mise:` or `curl:` trivial to add).
   - Note: In `packages.md` (the master catalog), items may be written with `- [x]`, `- [ ]`, or plain `- ` bullets; `packages.md` defines the catalog of known packages, their source (`Repo`, `Aur`, `Npm`), their section/category placement, and their description. (**Designer's call:** Supporting both `- [x]` and `- ` in `packages.md` ensures existing `packages.md` works unmodified without confusing catalog membership with per-host state.)
   - Package identity key is `(source, pkg_name)` or `pkg_name` (unique across the file; duplicates within a file are deduplicated preserving first-seen order).
3. **Prose & Blockquotes:**
   - Lines starting with `>` (blockquotes, such as format legends or setup notes) and free-form comments under categories are preserved in position relative to their section/category when syncing `packages.md` → `<hostname>.md`.

---

## 4. Core Workflows & Behavior

### 4.1 Config & Data Directory Resolution (**Designer's call**)
`mdpkg` resolves the data directory in the following priority order:
1. `--dir <PATH>` CLI flag (or `MDPKG_DIR` environment variable).
2. `~/.config/mdpkg/config.toml` (`data_dir = "/path/to/packages"`), configurable via `mdpkg config set-dir <PATH>`.
3. Current working directory if `packages.md` exists in `./`.

Host file resolution:
1. `--host <NAME_OR_FILE>` CLI flag.
2. Default host name from `~/.config/mdpkg/config.toml` if configured, otherwise the system hostname (`gethostname()`, e.g., `<data_dir>/<hostname>.md`).

### 4.2 Catalog → Host File Reconciliation
Whenever `mdpkg sync`, `mdpkg add`, `mdpkg enable`, `mdpkg disable`/`remove`, or `mdpkg status` runs:
1. Parse `packages.md` as the structural template (sections, categories, blockquotes/notes, package items with source markers and descriptions).
2. Check if `<hostname>.md` exists:
   - If `<hostname>.md` exists, parse its recorded checkbox states for sections (`## [x| ]`), categories (`### [x| ]`), and individual packages (`- [x| ]`).
   - If `<hostname>.md` does not exist, initialize section checkboxes from `packages.md` and category checkboxes to `[x]`.
3. Query actual installed packages on the current machine:
   - Repo & AUR installed set: `pacman -Qq`
   - Global NPM installed set: `npm list -g --depth=0 --json` (if `npm` is in `PATH`; otherwise empty set).
4. For every package in `packages.md`:
   - If the package already existed in `<hostname>.md`, keep its recorded `[x]` or `[ ]` state from `<hostname>.md`.
   - If the package is **new** (present in `packages.md` but was not in `<hostname>.md`), set its checkbox to `[x]` if it is currently installed on the system, or `[ ]` if it is not installed.
5. **Write Policy (**Designer's call**):**
   - Mutating commands (`mdpkg sync` without `--dry-run`, `mdpkg add`, `mdpkg enable`, `mdpkg disable`/`remove`) render and write the reconciled `<hostname>.md` to disk, matching the structure, categories, comments, and descriptions of `packages.md`.
   - Inspection commands (`mdpkg status`, `mdpkg list`, and `mdpkg sync --dry-run`) are **strictly read-only**: they perform the reconciliation in-memory to report diffs and newly discovered catalog packages without modifying `<hostname>.md` on disk.
   - **Designer's call on packages present in `<hostname>.md` but deleted from `packages.md`:** If a package was in `<hostname>.md` and removed from `packages.md`, warn the user during sync and drop it from `<hostname>.md` (without auto-uninstalling unless the user unchecks it first, preventing accidental deletion if a line was typoed in `packages.md`).

### 4.3 Host File → System Sync (`mdpkg sync [SECTIONS...]`)
1. Determine sections in scope:
   - By default (or with `--all`): all sections marked `## [x]` in `<hostname>.md`.
   - If specific section names are passed (e.g. `mdpkg sync MAIN DEV` or `mdpkg sync PRINTING`): those named sections, even if their heading is `## [ ]`.
2. For each package in the in-scope sections:
   - **Effective state is enabled** (`section` in scope AND `### [x]` category enabled AND `- [x]` item enabled):
     - If not currently installed (or if `--force-aur` is passed for an AUR package): queue for **Installation**.
   - **Effective state is disabled** (`### [ ]` category disabled OR `- [ ]` item disabled):
     - If currently installed on the machine: queue for **Removal**.
3. Print a clear transaction summary (packages to install by source, packages to remove by source, and hooks that will run).
4. Execute in order:
   - **Pre-install hooks** for packages queued for installation (`<pkg>_preInstall*`). (**Designer's call:** If `<pkg>_preInstall` exits non-zero, log the error, remove `<pkg>` from the installation batch so broken pre-conditions don't install a half-configured package, continue remaining packages, and return a non-zero exit code at the end of the run.)
   - **Install packages:**
     - Repo packages: batch install via `pamac install --no-upgrade <flags> <pkgs...>`.
     - AUR packages: batch build via `pamac build <flags> <pkgs...>`.
     - Global NPM packages: `sudo npm install -g <pkgs...>`.
   - **Post-install hooks** for newly installed packages (`<pkg>_postInstall*`). (**Designer's call:** If a post-hook exits non-zero, log the error clearly, continue remaining hooks, and return a non-zero exit code at the end of the run.)
   - **Pre-removal hooks** for packages queued for removal (`<pkg>_preRemoval*`). (**Designer's call:** If `<pkg>_preRemoval` exits non-zero, skip removing `<pkg>`, log the error, and return a non-zero exit code at the end.)
   - **Remove packages:**
     - Repo & AUR packages: batch remove via `pamac remove <flags> <pkgs...>`.
     - Global NPM packages: `sudo npm uninstall -g <pkgs...>`.
   - **Post-removal hooks** for removed packages (`<pkg>_postRemoval*`).

### 4.4 Single-Password Elevation Strategy (`mdpkg update` & `mdpkg sync`)
Why `update.sh` previously asked for passwords multiple times:
- `pamac` uses Polkit (`org.manjaro.pamac.commit`) when run as a normal user (and `pamac build` refuses to run as `root`), which prompts separately from `sudo` and can time out across long AUR builds, while `sudo npm update -g` and root commands inside hooks use `sudo`.
- **Solution (**Designer's call**):**
  1. At the start of any privileged `mdpkg` command (`update`, `sync` with pending changes, `add`, `enable`, `disable`/`remove`), `mdpkg` runs `sudo -v` once to prompt for the user's password upfront, and spawns a background keep-alive thread that runs `sudo -n -v` every 50 seconds until `mdpkg` exits.
  2. To prevent Polkit from prompting a second time during `pamac` operations while keeping `pamac` running as the normal user: immediately after `sudo -v` succeeds, `mdpkg` uses `sudo` to write a temporary session Polkit rule `/etc/polkit-1/rules.d/90-mdpkg-session.rules` granting `polkit.Result.YES` for action `org.manjaro.pamac.commit` strictly to the invoking user's username (`subject.user == "<username>"`).
  3. An RAII guard (`Drop` implementation) plus `SIGINT`/`SIGTERM` signal handler automatically removes `/etc/polkit-1/rules.d/90-mdpkg-session.rules` as soon as the `mdpkg` command finishes or is interrupted.
  4. Specifically during `mdpkg update`:
     - Authenticate once via `sudo -v` + activate temporary Polkit session rule + start `sudo` keep-alive.
     - Step 1: `pamac clean --build-files --no-confirm` (reset AUR build files so updated fix scripts/PKGBUILDs are fetched fresh).
     - Step 2: `pamac update -a --force-refresh` (passes `--no-confirm` if requested).
     - Step 3: Clean orphans (`pamac remove --orphans --no-confirm`, ignoring "no orphans" exit status).
     - Step 4: If `npm` is installed, run `sudo npm update -g`.

### 4.5 Adding, Enabling, & Disabling Packages (`mdpkg add`, `enable`, `disable`/`remove`)
- **`mdpkg add <package> [--source m|a|n|repo|aur|npm] [--section <SEC>] [--category <CAT>] [--desc <TEXT>] [--no-install]`:**
  - If `--source` is omitted: prompt the user interactively to choose (`1. Manjaro Repo (m:)`, `2. AUR (a:)`, `3. Global npm (n:)`), hinting if `pacman -Si <pkg>` finds it in official repos.
  - If `--section` / `--category` is omitted: present an interactive selector listing existing `## SECTION` and `### category` groups parsed from `packages.md` (plus an option to create a new category).
  - If `--desc` is omitted: prompt for a short description (pre-filled with the package description from `pacman -Si` / AUR / `npm view` if available).
  - Installs the package on the current host (unless `--no-install` is passed), runs any `<pkg>_preInstall` / `<pkg>_postInstall` hooks, inserts the item into `packages.md` under the chosen section/category, and syncs `<hostname>.md` with the package marked `[x]`.
- **`mdpkg enable <package>` (**Designer's call**):**
  - Marks an existing catalog package as `[x]` in `<hostname>.md` and installs it (running pre/post install hooks) if not already installed.
- **`mdpkg disable <package>` / `mdpkg remove <package> [--purge-catalog]` (**Designer's call**):**
  - Marks the package as `[ ]` in `<hostname>.md` and uninstalls it from the current host (running pre/post removal hooks) if currently installed. If `--purge-catalog` is passed, also deletes the entry from `packages.md`.

### 4.6 Lifecycle Hooks (`mdpkg hooks`)
- **Execution Context (**Designer's call**):**
  - Hooks always execute as the **normal invoking user** (so `$HOME`, `~/.bashrc`, `~/.config`, etc. refer to the user's own environment). When a hook needs root privileges (such as enabling a systemd service or configuring system files), it calls `sudo` inside the script, which succeeds without prompting thanks to `mdpkg`'s active `sudo` keep-alive session.
- **`mdpkg hooks init`:**
  - Creates `<data_dir>/hooks/` and `<data_dir>/hooks/README.md`.
  - `hooks/README.md` contains:
    1. Clear instructions structured for both human users and AI coding agents on how `mdpkg` discovers and executes hooks.
    2. Naming convention: `<packageName>_<stage>` or `<packageName>_<stage>.<ext>` (where `<stage>` is one of `preInstall`, `postInstall`, `preRemoval`, `postRemoval`, e.g., `ripgrep_postInstall.sh`, `docker_postInstall.sh`).
    3. Execution rules: Hook runs as normal user (with passwordless `sudo` active during transactions). Hook may be directly executable (`chmod +x`) or have a `.sh`/`.py` extension (invoked via `bash` or `python3` automatically if a sync client strips executable bits).
    4. Environment variables passed to hooks: `MDPKG_PACKAGE`, `MDPKG_SOURCE`, `MDPKG_STAGE`, `MDPKG_HOST`, `MDPKG_DATA_DIR`.
    5. Ready-to-use recommended hook examples in the README:
       - Shell integration check/setup for `fzf`, `zoxide`, `mise`, `ripgrep`.
       - System service configuration (`docker_postInstall.sh`).
- **`mdpkg hooks list` / `mdpkg hooks run <pkg> <stage>`:**
  - Inspect all discovered hooks and manually test-run a specific hook.

### 4.7 Validation, Drift Check, & Inspection (`mdpkg check` / `mdpkg status` & `mdpkg list`)
- **`mdpkg check` (aliases: `mdpkg validate`, `mdpkg status`) — Read-Only Validation & Action Check:**
  1. **Markdown Syntax Validation (with exact `file:line` diagnostics):**
     - Validates `packages.md` and `<hostname>.md` (or all host files with `--all-hosts`).
     - Detects and reports:
       - Malformed `##` section headings or `###` category headings.
       - Package items appearing before any `##` section.
       - Malformed `- [...]` item lines (missing `` `package` `` backticks, invalid checkbox state, or unrecognized source prefix before the backtick).
       - Duplicate package entries within the same file (reporting both line numbers).
  2. **Catalog ↔ Host Sync Check:**
     - Reports if `<hostname>.md` is missing or out of sync with `packages.md`:
       - Newly added packages in `packages.md` not yet in `<hostname>.md` (showing what default `[x]`/`[ ]` state they will receive based on current installation).
       - Regrouped categories, moved packages, or updated descriptions/comments in `packages.md` waiting to be written to `<hostname>.md`.
       - Stale packages in `<hostname>.md` that were removed from `packages.md`.
  3. **Host ↔ System Action Check:**
     - Reports packages in `<hostname>.md` that need to be **installed** (`[x]` in file, not installed on machine) or **removed** (`[ ]` in file, currently installed on machine).
  4. **Hooks Validation:**
     - Checks `hooks/` for orphaned hook scripts (whose `<packageName>` does not match any package in `packages.md`) or non-executable hook files without a `.sh`/`.py` fallback extension, and lists which valid hooks are attached to each package.
  5. **Exit Codes:**
     - Returns non-zero immediately if any Markdown syntax error or invalid hook is found; with `--strict`, also returns non-zero if there are pending sync or install/remove actions.
- **`mdpkg list [--sections | --hooks | --installed | --diff]` (read-only):**
  - Lists sections, packages, and their current status + descriptions + attached hooks.

### 4.8 Rich AI-Agent-Friendly Help (`--help` & `mdpkg ai-help`)
- **Rich `clap` `--help` (`long_about` & `after_long_help`):**
  - Running `mdpkg --help` or `mdpkg <subcommand> --help` outputs detailed explanations, workflow examples, non-interactive flag combinations (for agent automation), and a concise summary of the Markdown checklist format (`## [x] SECTION`, `### [x] category`, `` - [x] `pkg` — desc ``, `m:` for Manjaro repo, `a:` for AUR, `n:` for global npm).
- **`mdpkg ai-help` Subcommand:**
  - Outputs a single, self-contained Markdown guide tailored for AI coding agents that includes:
    1. Resolved active paths on the current machine (`data_dir`, `packages.md`, `<hostname>.md`, `hooks/`).
    2. Exact rules for editing `packages.md` (how an AI can regroup, classify, add comments/descriptions, and add packages with source markers) vs. `<hostname>.md` (how checking `[x]` installs and unchecking `[ ]` uninstalls).
    3. How to create and test lifecycle hooks in `hooks/` (`<pkg>_preInstall`, `<pkg>_postInstall`, `<pkg>_preRemoval`, `<pkg>_postRemoval`).
    4. Copy-pasteable non-interactive CLI commands (using `--source`, `--section`, `--category`, `--desc`, `--no-confirm`, `--dry-run`) so an agent never gets stuck on an interactive prompt.

---

## 5. CLI Command Summary

```text
mdpkg sync [SECTIONS...] [--all] [--host <HOST>] [--force-aur] [--dry-run] [--no-confirm]
mdpkg update [--no-confirm] [--skip-npm] [--skip-orphans]
mdpkg add <PACKAGE> [--source repo|aur|npm] [--section <SEC>] [--category <CAT>] [--desc <DESC>] [--no-install]
mdpkg enable <PACKAGE> [--host <HOST>]
mdpkg disable <PACKAGE> [--host <HOST>]    # Alias: mdpkg remove <PACKAGE> [--purge-catalog]
mdpkg remove <PACKAGE> [--purge-catalog] [--host <HOST>]
mdpkg check [--host <HOST>] [--all-hosts] [--strict] # Validate MD syntax, catalog->host sync, & system drift (aliases: status, validate)
mdpkg list [--sections] [--hooks] [--diff] # Read-only listing of sections, packages, descriptions, hooks
mdpkg hooks init                           # Initialize hooks/ folder with AI-ready README & examples
mdpkg hooks list                           # Show all configured hooks matched to packages
mdpkg hooks run <PACKAGE> <STAGE>          # Manually trigger a hook for testing
mdpkg config set-dir <PATH>                # Save default data_dir in ~/.config/mdpkg/config.toml
mdpkg config show                          # Show resolved configuration
mdpkg ai-help                              # Print complete AI-agent Markdown guide & resolved paths
```

---

## 6. Open-Source & AUR Packaging

- **Repository structure:**
  - Standard Cargo workspace/package (`Cargo.toml`, `src/`, `tests/`, `README.md`, `LICENSE` (MIT)).
  - `packaging/aur/PKGBUILD` and `packaging/aur/.SRCINFO` building `mdpkg` from source using `cargo build --release --locked` and installing binary + shell completions + man/readme.
- **Dependencies & Rust Ecosystem ( Left to Builder ):**
  - CLI parsing: `clap` (with derive).
  - Interactive prompts: `dialoguer` or `inquire`.
  - Config serialization: `serde` + `toml` + `serde_json` (for parsing `npm list -g --json`).
  - Terminal styling: `console` / `owo-colors`.

---

## 7. What's Left to the Builder

- Internal module breakdown (`cli`, `config`, `markdown`, `sync`, `sources`, `hooks`, `privilege`).
- Exact formatting of terminal tables/colors in `mdpkg status` and `mdpkg list`.
- Exact regex/AST implementation for lossless Markdown round-tripping so blockquotes, blank lines, and descriptions are preserved identically when syncing `packages.md` → `<hostname>.md`.
- Unit and integration test suite covering Markdown parsing, catalog-to-host reconciliation (including new-package default state from mock installed sets), hook discovery (including non-executable `.sh`/`.py` fallback on Google Drive), and CLI argument handling.
