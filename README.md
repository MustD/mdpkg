# `mdpkg` — Markdown-Driven Multi-Host Package Manager for Manjaro Linux

`mdpkg` is an open-source, declarative package manager for Manjaro Linux. It replaces ad-hoc shell and Python installer scripts with human- and AI-editable Markdown checklists, automated lifecycle hooks, single-password system updates, and safe multi-host state reconciliation.

---

## Key Features

- **Declarative Markdown Checklists:** Manage official repository, AUR, and global `npm` packages in clean Markdown files (`packages.md` master catalog and `<hostname>.md` per-host checklists).
- **Safe Multi-Host Reconciliation:** Sync catalog additions and reorganizations across multiple machines without clobbering host-specific enabled/disabled choices.
- **Desired-State Enforcement:** Checking `- [x]` installs a package; unchecking `- [ ]` removes it. Disabling an entire category `### [ ]` disables all child items while preserving their checkboxes on disk for easy re-enabling.
- **Single-Password Elevation:** Runs `sudo -v` upfront and maintains credentials in the background alongside a temporary Polkit session rule so long unattended system updates and AUR builds never pause for repeated password prompts.
- **Lifecycle Hooks:** Execute custom scripts (`preInstall`, `postInstall`, `preRemoval`, `postRemoval`) for custom setups like system services, hardware drivers, and shell integrations, with automatic fallbacks for filesystems or sync clients (e.g. Nextcloud, Syncthing, Google Drive) that strip executable bits.
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
sudo pacman -U https://github.com/MustD/mdpkg/releases/download/v0.1.1/mdpkg-0.1.1-1-x86_64.pkg.tar.zst
```

### From Source

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
├── desktop.md                # Host checklist for 'desktop'
└── hooks/                    # Created by `mdpkg hooks init`
    ├── README.md             # Lifecycle hooks guide and templates
    ├── ripgrep_postInstall.sh
    └── docker_postInstall.sh
```

### Checklist Syntax (`packages.md` & `<hostname>.md`)

```markdown
# Manjaro packages

> **Format legend**
> - `- [x]` enabled on host · `- [ ]` disabled (uninstalled if present on host)
> - `### [x] category` category toggle · `## [x] SECTION` section toggle (gates default sync)
> - Repo package: no prefix or `m:` — `- [x] `git` — version control`
> - AUR package: `a:` prefix — `- [x] a:`google-chrome` — Chrome browser` (legacy `Ⓐ` also accepted)
> - Global npm package: `n:` prefix — `- [x] n:`typescript` — TypeScript compiler`

## [x] MAIN — Core system utilities

### [x] CLI tools
- [x] `git` — version control
- [x] m:`neovim` — modern text editor
- [x] `ripgrep` — fast grep replacement

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

### 2. Inspect Drift and Validate Markdown
Check syntax, pending catalog-to-host diffs, and package drift without making any changes:

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
Perform a clean, unattended update of repository packages, AUR builds, orphan cleanup, and global npm packages with only one password prompt:

```bash
mdpkg update
```

### 5. Add, Enable, and Disable Packages

```bash
# Add package non-interactively
mdpkg add ripgrep --source repo --section MAIN --category CLI --desc "Fast search"

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

## Lifecycle Hooks

Run `mdpkg hooks init` to set up `hooks/` with starter templates.

Hook scripts are named: `<packageName>_<stage>` (e.g. `ripgrep_postInstall.sh` or `docker_postInstall.sh`).

Stages:
- `preInstall`
- `postInstall`
- `preRemoval`
- `postRemoval`

Hooks receive environment variables:
- `MDPKG_PACKAGE`
- `MDPKG_SOURCE` (`repo`, `aur`, `npm`)
- `MDPKG_STAGE`
- `MDPKG_HOST`
- `MDPKG_DATA_DIR`

---

## AUR Packaging & Publishing

The AUR package specification lives in [`packaging/aur/PKGBUILD`](packaging/aur/PKGBUILD) and [`packaging/aur/.SRCINFO`](packaging/aur/.SRCINFO).

To build and publish a new release to the AUR:

```bash
# 1. Tag and create the GitHub release so the source tarball URL is available
gh release create v0.1.0 --title "v0.1.0" --generate-notes

# 2. Update sha256sums in packaging/aur/PKGBUILD and regenerate .SRCINFO
cd packaging/aur
updpkgsums
makepkg --printsrcinfo > .SRCINFO

# 3. Verify package build and tests
makepkg -sf

# 4. Push PKGBUILD and .SRCINFO to the AUR repository
git clone ssh://aur@aur.archlinux.org/mdpkg.git /tmp/aur-mdpkg
cp PKGBUILD .SRCINFO /tmp/aur-mdpkg/
git -C /tmp/aur-mdpkg commit -am "Update to v0.1.0-1"
git -C /tmp/aur-mdpkg push origin master
```

---

## License

MIT License. See [LICENSE](LICENSE) for details.
