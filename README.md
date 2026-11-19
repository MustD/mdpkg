# `mdpkg` — Markdown-Driven Multi-Host Package Manager for Linux

`mdpkg` is an open-source, declarative multi-host package manager for Linux (Manjaro/Arch and Ubuntu/Debian). It replaces ad-hoc shell and Python installer scripts with human- and AI-editable Markdown checklists, package-grouped lifecycle hooks, custom script-managed tools, single-password system updates, and safe multi-host state reconciliation.

---

## Key Features

- **Declarative Markdown Checklists:** Manage packages across 5 explicit sources in clean Markdown files (`packages.md` master catalog and `<hostname>.md` per-host checklists):
  - `m:` (or unprefixed) — Manjaro / Arch official repositories (`pamac` / `pacman`)
  - `a:` (or legacy `Ⓐ`) — Arch User Repository (`AUR`)
  - `d:` (or `apt:`) — Ubuntu / Debian `apt` packages
  - `n:` (or `npm:`) — Global `npm` packages
  - `c:` (or `custom:`) — Custom script-managed packages (`hooks/<pkg>/install.sh`, `remove.sh`, optional `update.sh`)
- **Safe Multi-Host Reconciliation:** Sync catalog additions and reorganizations across multiple machines (including mixed Manjaro and Ubuntu hosts) without clobbering host-specific enabled/disabled choices.
- **Desired-State Enforcement:** Checking `- [x]` installs a package; unchecking `- [ ]`, its parent category `### [ ]`, or its parent section `## [ ]` removes it while preserving child checkboxes on disk for easy re-enabling.
- **Single-Password Elevation:** Runs `sudo -v` upfront and maintains credentials in the background alongside a temporary Polkit session rule (when Polkit is present) so long unattended system updates and AUR builds never pause for repeated password prompts.
- **Package-Grouped Lifecycle Hooks & Custom Packages:** Organize hooks cleanly under `hooks/<packageName>/<stage>[.sh|.py]` (`preInstall`, `postInstall`, `preRemoval`, `postRemoval`, plus `install`, `remove`, and `update` for `c:` custom packages), with automatic fallbacks for filesystems or sync clients (e.g. Nextcloud, Syncthing, Google Drive) that strip executable bits.
- **Resilient System Updates (`mdpkg update` / `mdpkg updates`):** Upgrade official repositories, AUR, APT, global `npm`, and custom `c:` packages in one pass, with `--ignore <pkg1>,<pkg2>` support (automatically holding/unholding APT packages or passing `--ignore` to `pamac`/`pacman` and skipping ignored custom scripts).
- **AI Agent Native:** Built-in `mdpkg ai-help` command and rich `--help` documentation providing structured instructions and non-interactive command flags for coding agents.

---

## Installation

### From AUR (Arch User Repository)

[`mdpkg` is available on the AUR](https://aur.archlinux.org/packages/mdpkg):

```bash
# Using pamac (Manjaro)
pamac build mdpkg

# Or using yay / paru
yay -S mdpkg
```

### From GitHub Releases (Prebuilt Package)

Download the prebuilt `.pkg.tar.zst` package from the [latest GitHub Release](https://github.com/MustD/mdpkg/releases/latest) and install with `pacman`:

```bash
sudo pacman -U https://github.com/MustD/mdpkg/releases/download/v1.0.0/mdpkg-1.0.0-1-x86_64.pkg.tar.zst
```

### From Source (Manjaro / Arch / Ubuntu / Debian)

```bash
git clone https://github.com/MustD/mdpkg.git
cd mdpkg
cargo install --path .
```

---

## Markdown Syntax & Data Layout

### Directory Structure

```text
~/sync/packages/               # Configured via `mdpkg config set-dir <PATH>` or `--dir`
├── packages.md               # Master catalog of known packages and categories
├── laptop.md                 # Host checklist for 'laptop'
├── ubuntu-srv.md             # Host checklist for 'ubuntu-srv'
├── .mdpkg/                   # Per-host state tracking for custom (c:) packages
│   └── installed_custom_laptop.json
└── hooks/                    # Created by `mdpkg hooks init`
    ├── README.md             # Lifecycle hooks guide and templates
    ├── ripgrep/
    │   └── postInstall.sh
    ├── docker/
    │   └── postInstall.sh
    └── rustup/               # Scaffolded automatically by `mdpkg add rustup --source c`
        ├── install.sh
        ├── remove.sh
        └── update.sh
```

### Checklist Syntax (`packages.md` & `<hostname>.md`)

```markdown
# System packages

> **Format legend**
> - `- [x]` enabled on host · `- [ ]` disabled (uninstalled if present on host)
> - `### [x] category` category toggle · `## [x] SECTION` section toggle (unchecking `## [ ]` or `### [ ]` disables and removes all child packages)
> - Manjaro/Arch repo package: no prefix or `m:` — `- [x] `git` — version control`
> - AUR package: `a:` prefix — `- [x] a:`google-chrome` — Chrome browser` (legacy `Ⓐ` also accepted)
> - Ubuntu/Debian APT package: `d:` prefix — `- [x] d:`jq` — JSON processor`
> - Global npm package: `n:` prefix — `- [x] n:`typescript` — TypeScript compiler`
> - Custom script package: `c:` prefix — `- [x] c:`rustup` — Rust toolchain installer`

## [x] MAIN — Core system utilities

### [x] CLI tools
- [x] `git` — version control
- [x] m:`neovim` — modern text editor
- [x] `ripgrep` — fast grep replacement
- [x] d:`jq` — command-line JSON processor
- [x] c:`rustup` — Rust toolchain installer

### [x] browsers
- [x] a:`google-chrome` — Chrome web browser

## [ ] CONTAINERS — Optional container & virtualization tools

> Note: Enable and start docker service via post-install hook

### [x] runtime
- [x] `docker` — container engine
```

---

## CLI Quickstart

### 1. Configure Data Directory
Set your persistent synced directory (e.g. Nextcloud, Syncthing, Google Drive, or Git repository):

```bash
mdpkg config set-dir ~/sync/packages
```

### 2. Inspect Drift and Validate Markdown & Hooks
Check Markdown syntax, hook directory structure (including unedited custom script stubs), pending catalog-to-host diffs, and package drift without making any changes:

```bash
mdpkg check
```

### 3. Synchronize Packages
Reconcile `<hostname>.md` with `packages.md` and apply changes:

```bash
mdpkg sync
```

Or preview changes first:
```bash
mdpkg sync --dry-run
```

### 4. Single-Password System Update
Perform a clean, unattended update of repository packages, AUR builds, APT packages, custom `c:` packages, global npm packages, and orphan cleanup with only one password prompt:

```bash
mdpkg update
# Alias:
mdpkg updates
```

To skip problematic packages during an update:
```bash
mdpkg update --ignore broken-pkg,another-pkg
```

### 5. Add, Enable, and Disable Packages

```bash
# Add package interactively (prompts for source, existing/new section, category, and description)
mdpkg add ripgrep

# Add package non-interactively (m = Manjaro repo, a = AUR, d = APT, n = npm, c = custom)
mdpkg add jq --source d --desc "JSON processor"

# Add a custom script-managed package (scaffolds hooks/<pkg>/install.sh and remove.sh)
mdpkg add rustup --source c --desc "Rust toolchain"

# Enable an existing package on current host
mdpkg enable google-chrome

# Disable and uninstall from current host
mdpkg disable git
```

### 6. AI Coding Agent Help

Print a self-contained automation manual with resolved machine paths:

```bash
mdpkg ai-help
```

### 7. Shell Completions

Generate completion scripts for `bash`, `zsh`, `fish`, `elvish`, or `powershell`:

```bash
# Bash (~/.bashrc)
source <(mdpkg completions bash)

# Zsh (~/.zshrc)
source <(mdpkg completions zsh)

# Fish (~/.config/fish/config.fish)
mdpkg completions fish | source
```

---

## Lifecycle Hooks & Custom (`c:`) Packages

Run `mdpkg hooks init` to set up `hooks/` with starter templates.

Hook scripts are grouped by package directory: `hooks/<packageName>/<stage>[.sh|.py]` (e.g. `hooks/ripgrep/postInstall.sh` or `hooks/docker/postInstall.sh`).

### Standard Lifecycle Stages
- `preInstall`  — runs before the package is installed
- `postInstall` — runs after the package is installed
- `preRemoval`  — runs before the package is uninstalled
- `postRemoval` — runs after the package is uninstalled

### Custom Package (`c:`) Stages
When you run `mdpkg add <pkg> --source c`, `mdpkg` creates stub templates at `hooks/<pkg>/install.sh` and `hooks/<pkg>/remove.sh` and prompts you to fill them in before syncing (`mdpkg check` and `mdpkg sync` detect unedited stubs so empty templates are never marked as installed):
- `install`     — required for `c:` packages; installs the custom tool
- `remove`      — required for `c:` packages; uninstalls the custom tool
- `update`      — optional for `c:` packages; runs during `mdpkg update` (falls back to `install` if omitted)

### Hook Environment Variables
Hooks receive:
- `MDPKG_PACKAGE`
- `MDPKG_SOURCE` (`repo`, `aur`, `apt`, `npm`, `custom`)
- `MDPKG_STAGE` (`preInstall`, `postInstall`, `preRemoval`, `postRemoval`, `install`, `remove`, `update`)
- `MDPKG_HOST`
- `MDPKG_DATA_DIR`

---

## AUR Packaging & Publishing

The AUR package specification lives in [`packaging/aur/PKGBUILD`](packaging/aur/PKGBUILD) and [`packaging/aur/.SRCINFO`](packaging/aur/.SRCINFO).

To build and publish a new release to the AUR:

```bash
# 1. Tag and create the GitHub release so the source tarball URL is available
gh release create v1.0.0 --title "v1.0.0" --generate-notes

# 2. Update sha256sums in packaging/aur/PKGBUILD and regenerate .SRCINFO
cd packaging/aur
updpkgsums
makepkg --printsrcinfo > .SRCINFO

# 3. Build the package, run tests, and upload the artifact to the GitHub release
makepkg -sfc
gh release upload v1.0.0 mdpkg-1.0.0-1-x86_64.pkg.tar.zst

# 4. Push PKGBUILD and .SRCINFO to the AUR repository
git clone ssh://aur@aur.archlinux.org/mdpkg.git /tmp/aur-mdpkg
cp PKGBUILD .SRCINFO /tmp/aur-mdpkg/
git -C /tmp/aur-mdpkg commit -am "Update to v1.0.0-1"
git -C /tmp/aur-mdpkg push origin master
```

---

## License

MIT License. See [LICENSE](LICENSE) for details.
