use crate::config::ResolvedPaths;
use clap::{Args, Parser, Subcommand, ValueEnum, ValueHint};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "mdpkg",
    author,
    version,
    about = "Declarative multi-host package manager for Manjaro Linux via Markdown checklists",
    long_about = "mdpkg is a declarative, multi-host package manager for Manjaro Linux.\n\
It uses a human- and AI-editable Markdown catalog (packages.md) paired with per-host\n\
checklists (<hostname>.md) to install, uninstall, and synchronize packages across machines\n\
with lifecycle hooks, single-password elevation, and unattended system updates."
)]
pub struct Cli {
    #[arg(
        short = 'd',
        long = "dir",
        env = "MDPKG_DIR",
        value_hint = ValueHint::DirPath,
        help = "Path to mdpkg data directory (containing packages.md)"
    )]
    pub dir: Option<PathBuf>,

    #[arg(
        short = 'H',
        long = "host",
        help = "Hostname or host checklist file to use (defaults to system hostname)"
    )]
    pub host: Option<String>,

    #[arg(
        short = 'c',
        long = "config",
        value_hint = ValueHint::FilePath,
        help = "Custom path to config.toml (defaults to ~/.config/mdpkg/config.toml)"
    )]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliPackageSource {
    #[value(name = "m", alias = "repo", alias = "manjaro")]
    Repo,
    #[value(name = "a", alias = "aur")]
    Aur,
    #[value(name = "n", alias = "npm")]
    Npm,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    #[command(
        about = "Synchronize host system with host Markdown checklist",
        long_about = "Reconciles <hostname>.md against packages.md, determines packages that need to be\n\
installed or removed, executes lifecycle hooks, and applies package manager transactions."
    )]
    Sync(SyncArgs),

    #[command(
        about = "Perform unattended full system update with single password prompt",
        long_about = "Acquires sudo and Polkit credentials upfront, clears AUR build caches,\n\
upgrades official repos and AUR packages via pamac, cleans orphans, and updates global npm packages."
    )]
    Update(UpdateArgs),

    #[command(
        about = "Add a new package to the catalog and enable it on the current host",
        long_about = "Adds a package to packages.md and <hostname>.md, and installs it immediately\n\
unless --no-install is specified. If --source, --section, or --category are omitted in an interactive\n\
terminal, prompts interactively."
    )]
    Add(AddArgs),

    #[command(
        about = "Enable a package on the current host (marks [x] and installs if not present)",
        long_about = "Marks the package as [x] in <hostname>.md, runs pre/post install hooks, and installs it."
    )]
    Enable(PackageTargetArgs),

    #[command(
        about = "Disable a package on the current host (marks [ ] and uninstalls)",
        long_about = "Marks the package as [ ] in <hostname>.md, runs pre/post removal hooks, and uninstalls it."
    )]
    Disable(DisableArgs),

    #[command(
        about = "Alias for disable (or removes package from catalog with --purge-catalog)",
        long_about = "Marks package [ ] in <hostname>.md and uninstalls. If --purge-catalog is passed, also deletes it from packages.md."
    )]
    Remove(RemoveArgs),

    #[command(
        visible_aliases = ["status", "validate"],
        about = "Validate Markdown syntax, check catalog-to-host drift, and inspect system drift",
        long_about = "Strictly read-only inspection. Checks packages.md and <hostname>.md for syntax errors,\n\
reports pending catalog-to-host sync diffs, and lists uninstalled/unwanted packages."
    )]
    Check(CheckArgs),

    #[command(
        about = "List sections, categories, packages, and their status",
        long_about = "Read-only listing of packages in the catalog with status, source, descriptions, and hooks."
    )]
    List(ListArgs),

    #[command(about = "Manage lifecycle hooks")]
    Hooks(HooksCommand),

    #[command(about = "View or modify mdpkg configuration")]
    Config(ConfigCommand),

    #[command(
        name = "ai-help",
        about = "Output complete structured Markdown guide for AI coding agents",
        long_about = "Prints a comprehensive Markdown guide detailing resolved local paths, formatting rules,\n\
non-interactive CLI flags, and hook creation rules for automated agents."
    )]
    AiHelp,

    #[command(
        about = "Generate shell completion scripts to stdout",
        long_about = "Outputs a static completion script for the specified shell (bash, zsh, fish, elvish, powershell)."
    )]
    Completions(CompletionsArgs),
}

#[derive(Args, Debug, Clone)]
pub struct CompletionsArgs {
    #[arg(
        value_enum,
        help = "Target shell (bash, zsh, fish, elvish, powershell)"
    )]
    pub shell: Shell,
}

#[derive(Args, Debug, Clone)]
pub struct SyncArgs {
    #[arg(help = "Specific sections to sync (e.g. MAIN CLI). Defaults to all checked sections.")]
    pub sections: Vec<String>,

    #[arg(long, help = "Sync all checked sections (default behavior)")]
    pub all: bool,

    #[arg(
        long,
        help = "Force reinstall/rebuild of already-installed AUR packages"
    )]
    pub force_aur: bool,

    #[arg(
        long,
        help = "Simulate operations without making changes to disk or packages"
    )]
    pub dry_run: bool,

    #[arg(
        long,
        help = "Do not prompt for confirmation before package operations"
    )]
    pub no_confirm: bool,
}

#[derive(Args, Debug, Clone)]
pub struct UpdateArgs {
    #[arg(long, help = "Do not prompt for confirmation")]
    pub no_confirm: bool,

    #[arg(long, help = "Skip updating global npm packages")]
    pub skip_npm: bool,

    #[arg(long, help = "Skip removing orphan packages")]
    pub skip_orphans: bool,
}

#[derive(Args, Debug, Clone)]
pub struct AddArgs {
    #[arg(help = "Package name to add")]
    pub package: String,

    #[arg(
        short,
        long,
        value_enum,
        help = "Package source (m/repo, a/aur, n/npm)"
    )]
    pub source: Option<CliPackageSource>,

    #[arg(long, help = "Target section name (e.g. MAIN)")]
    pub section: Option<String>,

    #[arg(long, help = "Target category name (e.g. cli)")]
    pub category: Option<String>,

    #[arg(long, help = "Short package description")]
    pub desc: Option<String>,

    #[arg(
        long,
        help = "Add to Markdown without installing on the current machine"
    )]
    pub no_install: bool,
}

#[derive(Args, Debug, Clone)]
pub struct PackageTargetArgs {
    #[arg(help = "Package name")]
    pub package: String,
}

#[derive(Args, Debug, Clone)]
pub struct DisableArgs {
    #[arg(help = "Package name")]
    pub package: String,

    #[arg(long, help = "Do not prompt for confirmation")]
    pub no_confirm: bool,
}

#[derive(Args, Debug, Clone)]
pub struct RemoveArgs {
    #[arg(help = "Package name")]
    pub package: String,

    #[arg(
        long,
        help = "Also purge the package entry from packages.md master catalog"
    )]
    pub purge_catalog: bool,

    #[arg(long, help = "Do not prompt for confirmation")]
    pub no_confirm: bool,
}

#[derive(Args, Debug, Clone)]
pub struct CheckArgs {
    #[arg(
        long,
        help = "Check all <host>.md files in the data directory, not just the current host"
    )]
    pub all_hosts: bool,

    #[arg(
        long,
        help = "Exit with non-zero status if any pending actions or drifts are found"
    )]
    pub strict: bool,
}

#[derive(Args, Debug, Clone)]
pub struct ListArgs {
    #[arg(long, help = "List sections and categories only")]
    pub sections: bool,

    #[arg(long, help = "List attached lifecycle hooks for each package")]
    pub hooks: bool,

    #[arg(long, help = "List only packages with pending diffs/actions")]
    pub diff: bool,
}

#[derive(Args, Debug, Clone)]
pub struct HooksCommand {
    #[command(subcommand)]
    pub action: HooksSubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HooksSubcommand {
    #[command(about = "Initialize hooks/ directory with human- and AI-ready README and templates")]
    Init,

    #[command(about = "List all discovered lifecycle hooks")]
    List,

    #[command(about = "Run a specific hook script for testing")]
    Run {
        #[arg(help = "Package name")]
        package: String,
        #[arg(
            value_parser = ["preInstall", "postInstall", "preRemoval", "postRemoval"],
            help = "Hook stage (preInstall, postInstall, preRemoval, postRemoval)"
        )]
        stage: String,
    },
}

#[derive(Args, Debug, Clone)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub action: ConfigSubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigSubcommand {
    #[command(about = "Set default data directory path in config.toml")]
    SetDir {
        #[arg(value_hint = ValueHint::DirPath, help = "Directory path")]
        path: PathBuf,
    },

    #[command(about = "Show resolved configuration and active paths")]
    Show,
}

pub fn render_ai_help(paths: &ResolvedPaths) -> String {
    format!(
        r#"# `mdpkg` Agent & Automation Reference Manual

## Active Resolved Paths
- Data directory: `{}`
- Master catalog: `{}`
- Host checklist: `{}`
- Hooks directory: `{}`
- Config file:    `{}`

## Core Concepts & Rules
1. `packages.md` defines known packages, sections (`##`), categories (`###`), and descriptions.
2. `<hostname>.md` tracks host-specific enablement:
   - `- [x]` -> installed on this host
   - `- [ ]` -> uninstalled/removed from this host
   - `### [ ]` -> category disabled (all child packages queued for removal while preserving child checkboxes)
3. Source prefixes before the package backtick:
   - No prefix or `m:` -> Manjaro repository package
   - `a:` (or legacy `Ⓐ`) -> AUR package (normalized to `a:`)
   - `n:` -> Global npm package

## Non-Interactive CLI Commands for Agents
- Add package:
  `mdpkg add <pkg> --source repo|aur|npm --section <SEC> --category <CAT> --desc "<DESC>"`
- Enable package:
  `mdpkg enable <pkg>`
- Disable package:
  `mdpkg disable <pkg> --no-confirm`
- Sync host state:
  `mdpkg sync --no-confirm`
- Inspect status without mutations:
  `mdpkg check`
- Full system update:
  `mdpkg update --no-confirm`
- Generate shell completions:
  `mdpkg completions <bash|zsh|fish|elvish|powershell>`
"#,
        paths.data_dir.display(),
        paths.catalog_path.display(),
        paths.host_path.display(),
        paths.hooks_dir.display(),
        paths.config_file.display()
    )
}
