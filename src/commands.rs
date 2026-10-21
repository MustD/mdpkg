use crate::cli::{
    AddArgs, CheckArgs, Cli, CliPackageSource, Commands, CompletionsArgs, ConfigCommand,
    ConfigSubcommand, DisableArgs, HooksCommand, HooksSubcommand, ListArgs, PackageTargetArgs,
    RemoveArgs, SyncArgs, UpdateArgs, render_ai_help,
};
use clap::CommandFactory;
use clap_complete::{Shell, generate};
use crate::config::{ResolvedPaths, resolve_paths, save_data_dir};
use crate::hooks::{discover_hooks, init_hooks_dir, run_hook, validate_hooks};
use crate::markdown::{
    insert_package_item, parse_document, remove_package_item, render_document, set_package_checked,
    validate_document,
};
use crate::model::{HookStage, PackageItem, PackageSource};
use crate::reconcile::{compute_sync_plan, reconcile_catalog_to_host};
use crate::sources::{SystemBackend, execute_sync_plan};
use dialoguer::{Input, Select};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;

fn get_system_hostname() -> String {
    if let Ok(name) = fs::read_to_string("/etc/hostname") {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Ok(name) = env::var("HOSTNAME") {
        if !name.trim().is_empty() {
            return name.trim().to_string();
        }
    }
    "localhost".to_string()
}

fn get_default_config_path() -> PathBuf {
    if let Ok(home) = env::var("HOME") {
        PathBuf::from(home).join(".config/mdpkg/config.toml")
    } else {
        PathBuf::from(".config/mdpkg/config.toml")
    }
}

pub fn generate_completions(shell: Shell, buf: &mut dyn std::io::Write) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "mdpkg", buf);
}

pub fn execute_command(cli: Cli, backend: &mut dyn SystemBackend) -> Result<(), anyhow::Error> {
    if let Some(Commands::Completions(CompletionsArgs { shell })) = cli.command {
        generate_completions(shell, &mut std::io::stdout());
        return Ok(());
    }

    let config_file = cli.config.clone().unwrap_or_else(get_default_config_path);
    let cwd = env::current_dir()?;
    let sys_hostname = get_system_hostname();

    let paths = resolve_paths(
        cli.dir.as_deref(),
        cli.host.as_deref(),
        &config_file,
        &cwd,
        &sys_hostname,
    )?;

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            // Default to running check
            Commands::Check(CheckArgs {
                all_hosts: false,
                strict: false,
            })
        }
    };

    match command {
        Commands::AiHelp => {
            println!("{}", render_ai_help(&paths));
            Ok(())
        }
        Commands::Completions(_) => unreachable!("Handled before path resolution"),
        Commands::Config(cmd) => handle_config(cmd, &paths),
        Commands::Hooks(cmd) => handle_hooks(cmd, &paths),
        Commands::Check(args) => handle_check(args, &paths, backend),
        Commands::Sync(args) => handle_sync(args, &paths, backend),
        Commands::Update(args) => handle_update(args, backend),
        Commands::Add(args) => handle_add(args, &paths, backend),
        Commands::Enable(args) => handle_enable(args, &paths, backend),
        Commands::Disable(args) => handle_disable(args, &paths, backend),
        Commands::Remove(args) => handle_remove(args, &paths, backend),
        Commands::List(args) => handle_list(args, &paths, backend),
    }
}

fn handle_config(cmd: ConfigCommand, paths: &ResolvedPaths) -> Result<(), anyhow::Error> {
    match cmd.action {
        ConfigSubcommand::SetDir { path } => {
            let saved = save_data_dir(&paths.config_file, &path)?;
            println!("Saved default data_dir = {}", saved.display());
            Ok(())
        }
        ConfigSubcommand::Show => {
            let cfg_status = if paths.config_file.exists() {
                "present"
            } else {
                "not found (run `mdpkg config set-dir <PATH>` to configure default data_dir)"
            };
            println!(
                "Configuration File: {} ({})",
                paths.config_file.display(),
                cfg_status
            );
            println!("Active Data Dir:    {}", paths.data_dir.display());
            println!("Master Catalog:     {}", paths.catalog_path.display());
            println!("Host Checklist:     {}", paths.host_path.display());
            println!("Hooks Directory:    {}", paths.hooks_dir.display());
            Ok(())
        }
    }
}

fn ensure_catalog_exists(paths: &ResolvedPaths) -> Result<(), anyhow::Error> {
    if !paths.catalog_path.exists() {
        anyhow::bail!(
            "Catalog file does not exist at {} (use `mdpkg config set-dir <PATH>` to set a persistent default data directory or pass `--dir <PATH>`)",
            paths.catalog_path.display()
        );
    }
    Ok(())
}

fn handle_hooks(cmd: HooksCommand, paths: &ResolvedPaths) -> Result<(), anyhow::Error> {
    match cmd.action {
        HooksSubcommand::Init => {
            let readme = init_hooks_dir(&paths.hooks_dir)?;
            println!("Initialized hooks directory: {}", paths.hooks_dir.display());
            println!("Created documentation at: {}", readme.display());
            Ok(())
        }
        HooksSubcommand::List => {
            let hooks = discover_hooks(&paths.hooks_dir)?;
            if hooks.is_empty() {
                println!("No hooks found in {}", paths.hooks_dir.display());
            } else {
                println!("Discovered lifecycle hooks ({}):", hooks.len());
                for h in hooks {
                    println!("  - {:<12} {:<20} -> {}", h.stage.as_str(), h.package, h.path.display());
                }
            }
            Ok(())
        }
        HooksSubcommand::Run { package, stage } => {
            let hook_stage = HookStage::from_stage_str(&stage)
                .ok_or_else(|| anyhow::anyhow!("Invalid hook stage '{}'. Use preInstall, postInstall, preRemoval, or postRemoval", stage))?;
            let hooks = discover_hooks(&paths.hooks_dir)?;
            let hook = hooks
                .iter()
                .find(|h| h.package == package && h.stage == hook_stage)
                .ok_or_else(|| anyhow::anyhow!("No hook found for package '{}' and stage '{}'", package, stage))?;

            let dummy_item = PackageItem {
                checked: true,
                source: PackageSource::Repo,
                explicit_m_prefix: false,
                name: package.clone(),
                description: String::new(),
                line_no: 0,
            };

            println!("Running hook {}...", hook.path.display());
            run_hook(hook, &dummy_item, &paths.host_name, &paths.data_dir)?;
            println!("Hook completed successfully.");
            Ok(())
        }
    }
}

fn handle_check(
    args: CheckArgs,
    paths: &ResolvedPaths,
    backend: &mut dyn SystemBackend,
) -> Result<(), anyhow::Error> {
    ensure_catalog_exists(paths)?;

    let catalog_content = fs::read_to_string(&paths.catalog_path)?;
    let (catalog_doc, catalog_issues) = parse_document(&paths.catalog_path, &catalog_content);

    let mut has_syntax_errors = false;
    if !catalog_issues.is_empty() {
        has_syntax_errors = true;
        eprintln!("Syntax errors in {}:", paths.catalog_path.display());
        for issue in &catalog_issues {
            eprintln!("  line {}: {}", issue.line, issue.message);
        }
    }

    let mut host_files = Vec::new();
    if args.all_hosts {
        if let Ok(entries) = fs::read_dir(&paths.data_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file()
                    && p.extension().and_then(|s| s.to_str()) == Some("md")
                    && p.file_name().and_then(|s| s.to_str()) != Some("packages.md")
                {
                    host_files.push(p);
                }
            }
        }
    } else if paths.host_path.exists() {
        host_files.push(paths.host_path.clone());
    }

    for hf in &host_files {
        let content = fs::read_to_string(hf)?;
        let issues = validate_document(hf, &content);
        if !issues.is_empty() {
            has_syntax_errors = true;
            eprintln!("Syntax errors in {}:", hf.display());
            for issue in &issues {
                eprintln!("  line {}: {}", issue.line, issue.message);
            }
        }
    }

    // Collect known packages
    let mut known_packages = HashSet::new();
    for sec in &catalog_doc.sections {
        for cat in &sec.categories {
            for item in &cat.items {
                known_packages.insert(item.name.clone());
            }
        }
    }

    // Hooks validation
    let hook_issues = validate_hooks(&paths.hooks_dir, &known_packages)?;
    if !hook_issues.is_empty() {
        has_syntax_errors = true;
        eprintln!("Hook validation issues:");
        for issue in &hook_issues {
            eprintln!("  {}: {}", issue.file.display(), issue.message);
        }
    }

    if has_syntax_errors {
        anyhow::bail!("Validation failed with errors");
    }

    // Check drift against current host
    let installed = backend.query_installed()?;
    let host_doc = if paths.host_path.exists() {
        let content = fs::read_to_string(&paths.host_path)?;
        Some(parse_document(&paths.host_path, &content).0)
    } else {
        None
    };

    let recon = reconcile_catalog_to_host(&catalog_doc, host_doc.as_ref(), &installed);
    let plan = compute_sync_plan(&recon.host_doc, None, &installed, false)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut pending_drift = false;

    if !recon.newly_added.is_empty() {
        pending_drift = true;
        println!("New packages in catalog not yet saved to {}:", paths.host_path.file_name().unwrap().to_string_lossy());
        for (pkg, src, chk) in &recon.newly_added {
            let state = if *chk { "enabled [x]" } else { "disabled [ ]" };
            println!("  - {} ({:?}) -> default {}", pkg, src, state);
        }
    }

    if !recon.removed_from_catalog.is_empty() {
        pending_drift = true;
        println!("Packages in host file removed from master catalog: {}", recon.removed_from_catalog.join(", "));
    }

    if !plan.to_install.is_empty() {
        pending_drift = true;
        println!("Pending installations ({}):", plan.to_install.len());
        for item in &plan.to_install {
            println!("  + {:<6} {}", item.source.prefix_str(), item.name);
        }
    }

    if !plan.to_remove.is_empty() {
        pending_drift = true;
        println!("Pending removals ({}):", plan.to_remove.len());
        for item in &plan.to_remove {
            println!("  - {:<6} {}", item.source.prefix_str(), item.name);
        }
    }

    if !pending_drift {
        println!("Catalog, host checklist, and installed packages are fully in sync.");
    } else if args.strict {
        anyhow::bail!("Strict mode: pending sync or system drifts found");
    }

    Ok(())
}

fn handle_sync(
    args: SyncArgs,
    paths: &ResolvedPaths,
    backend: &mut dyn SystemBackend,
) -> Result<(), anyhow::Error> {
    ensure_catalog_exists(paths)?;

    let catalog_content = fs::read_to_string(&paths.catalog_path)?;
    let (catalog_doc, issues) = parse_document(&paths.catalog_path, &catalog_content);
    if !issues.is_empty() {
        anyhow::bail!("Catalog file has syntax errors; run `mdpkg check`");
    }

    let installed = backend.query_installed()?;

    let host_doc = if paths.host_path.exists() {
        let content = fs::read_to_string(&paths.host_path)?;
        Some(parse_document(&paths.host_path, &content).0)
    } else {
        None
    };

    let recon = reconcile_catalog_to_host(&catalog_doc, host_doc.as_ref(), &installed);

    let requested_sections = if args.sections.is_empty() {
        None
    } else {
        Some(args.sections.as_slice())
    };

    let plan = compute_sync_plan(&recon.host_doc, requested_sections, &installed, args.force_aur)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if plan.to_install.is_empty() && plan.to_remove.is_empty() && !recon.structure_changed {
        println!("Everything is up to date. No changes needed.");
        return Ok(());
    }

    if !args.dry_run {
        let rendered = render_document(&recon.host_doc);
        fs::write(&paths.host_path, rendered)?;

        if !plan.to_install.is_empty() || !plan.to_remove.is_empty() {
            backend.ensure_elevated()?;
        }
    }

    execute_sync_plan(
        &plan,
        &paths.hooks_dir,
        &paths.host_name,
        &paths.data_dir,
        backend,
        args.no_confirm,
        args.dry_run,
    )?;

    println!("Sync complete.");
    Ok(())
}

fn handle_update(args: UpdateArgs, backend: &mut dyn SystemBackend) -> Result<(), anyhow::Error> {
    backend.ensure_elevated()?;
    backend.run_system_update(args.no_confirm, args.skip_npm, args.skip_orphans)
}

fn handle_add(
    args: AddArgs,
    paths: &ResolvedPaths,
    backend: &mut dyn SystemBackend,
) -> Result<(), anyhow::Error> {
    ensure_catalog_exists(paths)?;

    let catalog_content = fs::read_to_string(&paths.catalog_path)?;
    let (mut catalog_doc, _) = parse_document(&paths.catalog_path, &catalog_content);

    // Resolve source
    let source = if let Some(src) = args.source {
        match src {
            CliPackageSource::Repo => PackageSource::Repo,
            CliPackageSource::Aur => PackageSource::Aur,
            CliPackageSource::Npm => PackageSource::Npm,
        }
    } else if atty_check() {
        let selections = &["1. Manjaro Repository (default)", "2. AUR (a:)", "3. Global npm (n:)"];
        let idx = Select::new()
            .with_prompt("Select package source")
            .items(selections)
            .default(0)
            .interact()?;
        match idx {
            0 => PackageSource::Repo,
            1 => PackageSource::Aur,
            _ => PackageSource::Npm,
        }
    } else {
        PackageSource::Repo
    };

    // Resolve section
    let section = if let Some(sec) = args.section {
        sec
    } else if let Some(first_sec) = catalog_doc.sections.first() {
        first_sec.name.clone()
    } else {
        "MAIN".to_string()
    };

    // Resolve category
    let category = if let Some(cat) = args.category {
        cat
    } else if let Some(sec) = catalog_doc.sections.iter().find(|s| s.name.eq_ignore_ascii_case(&section)) {
        sec.categories.first().and_then(|c| c.name.clone()).unwrap_or_else(|| "general".to_string())
    } else {
        "general".to_string()
    };

    // Resolve description
    let description = if let Some(desc) = args.desc {
        desc
    } else if atty_check() {
        Input::<String>::new()
            .with_prompt("Short description (optional)")
            .allow_empty(true)
            .interact_text()?
    } else {
        String::new()
    };

    let item = PackageItem {
        checked: true,
        source,
        explicit_m_prefix: false,
        name: args.package.clone(),
        description,
        line_no: 0,
    };

    // Install immediately if not --no-install
    if !args.no_install {
        backend.ensure_elevated()?;
        let hooks = discover_hooks(&paths.hooks_dir).unwrap_or_default();
        if let Some(h) = hooks.iter().find(|h| h.package == item.name && h.stage == HookStage::PreInstall) {
            run_hook(h, &item, &paths.host_name, &paths.data_dir)?;
        }

        let pkg_list = vec![item.name.clone()];
        match item.source {
            PackageSource::Repo => backend.install_repo(&pkg_list, true, false)?,
            PackageSource::Aur => backend.install_aur(&pkg_list, true, false)?,
            PackageSource::Npm => backend.install_npm(&pkg_list, false)?,
        }

        if let Some(h) = hooks.iter().find(|h| h.package == item.name && h.stage == HookStage::PostInstall) {
            run_hook(h, &item, &paths.host_name, &paths.data_dir)?;
        }
    }

    // Insert into master catalog if not already present
    let already_in_catalog = catalog_doc
        .sections
        .iter()
        .any(|s| s.categories.iter().any(|c| c.items.iter().any(|i| i.name == args.package)));

    if !already_in_catalog {
        insert_package_item(&mut catalog_doc, &section, &category, item);
        fs::write(&paths.catalog_path, render_document(&catalog_doc))?;
    } else {
        println!("Package '{}' already in master catalog. Updating host state.", args.package);
    }

    // Update host file
    let installed = backend.query_installed()?;
    let host_doc = if paths.host_path.exists() {
        let content = fs::read_to_string(&paths.host_path)?;
        Some(parse_document(&paths.host_path, &content).0)
    } else {
        None
    };
    let mut recon = reconcile_catalog_to_host(&catalog_doc, host_doc.as_ref(), &installed);
    set_package_checked(&mut recon.host_doc, &args.package, true);
    fs::write(&paths.host_path, render_document(&recon.host_doc))?;

    println!("Package '{}' added and enabled on {}.", args.package, paths.host_name);
    Ok(())
}

fn handle_enable(
    args: PackageTargetArgs,
    paths: &ResolvedPaths,
    backend: &mut dyn SystemBackend,
) -> Result<(), anyhow::Error> {
    ensure_catalog_exists(paths)?;

    let catalog_content = fs::read_to_string(&paths.catalog_path)?;
    let (catalog_doc, _) = parse_document(&paths.catalog_path, &catalog_content);

    let installed = backend.query_installed()?;
    let host_doc = if paths.host_path.exists() {
        let content = fs::read_to_string(&paths.host_path)?;
        Some(parse_document(&paths.host_path, &content).0)
    } else {
        None
    };

    let mut recon = reconcile_catalog_to_host(&catalog_doc, host_doc.as_ref(), &installed);
    if !set_package_checked(&mut recon.host_doc, &args.package, true) {
        anyhow::bail!("Package '{}' not found in catalog", args.package);
    }

    fs::write(&paths.host_path, render_document(&recon.host_doc))?;

    // Install if not already installed
    let plan = compute_sync_plan(&recon.host_doc, None, &installed, false)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let to_install: Vec<PackageItem> = plan.to_install.into_iter().filter(|p| p.name == args.package).collect();

    if !to_install.is_empty() {
        backend.ensure_elevated()?;
        let sub_plan = crate::reconcile::SyncPlan {
            to_install,
            to_remove: Vec::new(),
            skipped_installed_aur: Vec::new(),
        };
        execute_sync_plan(&sub_plan, &paths.hooks_dir, &paths.host_name, &paths.data_dir, backend, true, false)?;
    }

    println!("Enabled '{}' on {}.", args.package, paths.host_name);
    Ok(())
}

fn handle_disable(
    args: DisableArgs,
    paths: &ResolvedPaths,
    backend: &mut dyn SystemBackend,
) -> Result<(), anyhow::Error> {
    ensure_catalog_exists(paths)?;

    let catalog_content = fs::read_to_string(&paths.catalog_path)?;
    let (catalog_doc, _) = parse_document(&paths.catalog_path, &catalog_content);

    let installed = backend.query_installed()?;
    let host_doc = if paths.host_path.exists() {
        let content = fs::read_to_string(&paths.host_path)?;
        Some(parse_document(&paths.host_path, &content).0)
    } else {
        None
    };

    let mut recon = reconcile_catalog_to_host(&catalog_doc, host_doc.as_ref(), &installed);
    if !set_package_checked(&mut recon.host_doc, &args.package, false) {
        anyhow::bail!("Package '{}' not found in catalog", args.package);
    }

    fs::write(&paths.host_path, render_document(&recon.host_doc))?;

    // Uninstall if currently installed
    let plan = compute_sync_plan(&recon.host_doc, None, &installed, false)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let to_remove: Vec<PackageItem> = plan.to_remove.into_iter().filter(|p| p.name == args.package).collect();

    if !to_remove.is_empty() {
        backend.ensure_elevated()?;
        let sub_plan = crate::reconcile::SyncPlan {
            to_install: Vec::new(),
            to_remove,
            skipped_installed_aur: Vec::new(),
        };
        execute_sync_plan(&sub_plan, &paths.hooks_dir, &paths.host_name, &paths.data_dir, backend, args.no_confirm, false)?;
    }

    println!("Disabled '{}' on {}.", args.package, paths.host_name);
    Ok(())
}

fn handle_remove(
    args: RemoveArgs,
    paths: &ResolvedPaths,
    backend: &mut dyn SystemBackend,
) -> Result<(), anyhow::Error> {
    handle_disable(
        DisableArgs {
            package: args.package.clone(),
            no_confirm: args.no_confirm,
        },
        paths,
        backend,
    )?;

    if args.purge_catalog {
        let catalog_content = fs::read_to_string(&paths.catalog_path)?;
        let (mut catalog_doc, _) = parse_document(&paths.catalog_path, &catalog_content);
        if remove_package_item(&mut catalog_doc, &args.package) {
            fs::write(&paths.catalog_path, render_document(&catalog_doc))?;
            println!("Purged '{}' from catalog {}.", args.package, paths.catalog_path.display());
        }
    }

    Ok(())
}

fn handle_list(
    args: ListArgs,
    paths: &ResolvedPaths,
    backend: &mut dyn SystemBackend,
) -> Result<(), anyhow::Error> {
    ensure_catalog_exists(paths)?;

    let catalog_content = fs::read_to_string(&paths.catalog_path)?;
    let (catalog_doc, _) = parse_document(&paths.catalog_path, &catalog_content);
    let installed = backend.query_installed()?;

    let host_doc = if paths.host_path.exists() {
        let content = fs::read_to_string(&paths.host_path)?;
        Some(parse_document(&paths.host_path, &content).0)
    } else {
        None
    };

    let recon = reconcile_catalog_to_host(&catalog_doc, host_doc.as_ref(), &installed);
    let hooks = discover_hooks(&paths.hooks_dir).unwrap_or_default();

    for sec in &recon.host_doc.sections {
        let sec_chk = if sec.checked { "[x]" } else { "[ ]" };
        println!("{} Section: {}", sec_chk, sec.name);
        if args.sections {
            continue;
        }

        for cat in &sec.categories {
            let cat_chk = if cat.checked { "[x]" } else { "[ ]" };
            let cat_title = cat.name.as_deref().unwrap_or("general");
            println!("  {} Category: {}", cat_chk, cat_title);

            for item in &cat.items {
                let is_inst = installed.is_installed(item.source, &item.name);
                let inst_marker = if is_inst { "(installed)" } else { "(not installed)" };
                let chk = if item.checked { "[x]" } else { "[ ]" };
                let src_prefix = match item.source {
                    PackageSource::Repo => "repo",
                    PackageSource::Aur => "aur",
                    PackageSource::Npm => "npm",
                };

                let attached_hooks: Vec<String> = hooks
                    .iter()
                    .filter(|h| h.package == item.name)
                    .map(|h| h.stage.as_str().to_string())
                    .collect();

                let hooks_str = if attached_hooks.is_empty() {
                    String::new()
                } else {
                    format!(" [hooks: {}]", attached_hooks.join(", "))
                };

                println!(
                    "    - {} {:<5} {:<24} {:<15} — {}{}",
                    chk, src_prefix, item.name, inst_marker, item.description, hooks_str
                );
            }
        }
    }

    Ok(())
}

fn atty_check() -> bool {
    if cfg!(test) || env::var("MDPKG_NON_INTERACTIVE").is_ok() {
        return false;
    }
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}
