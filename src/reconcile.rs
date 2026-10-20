use crate::markdown::render_document;
use crate::model::{CategoryBlock, Document, PackageItem, PackageSource, SectionBlock};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct InstalledPackages {
    pub repo_and_aur: HashSet<String>,
    pub npm: HashSet<String>,
}

impl InstalledPackages {
    pub fn is_installed(&self, source: PackageSource, name: &str) -> bool {
        match source {
            PackageSource::Repo | PackageSource::Aur => self.repo_and_aur.contains(name),
            PackageSource::Npm => self.npm.contains(name),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReconcileResult {
    pub host_doc: Document,
    pub newly_added: Vec<(String, PackageSource, bool)>,
    pub removed_from_catalog: Vec<String>,
    pub structure_changed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SyncPlan {
    pub to_install: Vec<PackageItem>,
    pub to_remove: Vec<PackageItem>,
    pub skipped_installed_aur: Vec<String>,
}

pub fn reconcile_catalog_to_host(
    catalog: &Document,
    existing_host: Option<&Document>,
    installed: &InstalledPackages,
) -> ReconcileResult {
    // Collect host section checkboxes
    let mut host_sections: HashMap<String, bool> = HashMap::new();
    // Collect host category checkboxes: key (section_name, category_name)
    let mut host_categories: HashMap<(String, Option<String>), bool> = HashMap::new();
    // Collect host item checkboxes: key package_name -> checked
    let mut host_items: HashMap<String, bool> = HashMap::new();
    let mut host_packages: HashSet<String> = HashSet::new();

    if let Some(host) = existing_host {
        for sec in &host.sections {
            host_sections.insert(sec.name.to_lowercase(), sec.checked);
            for cat in &sec.categories {
                let cat_key = cat.name.as_ref().map(|n| n.to_lowercase());
                host_categories.insert((sec.name.to_lowercase(), cat_key), cat.checked);
                for item in &cat.items {
                    host_items.insert(item.name.clone(), item.checked);
                    host_packages.insert(item.name.clone());
                }
            }
        }
    }

    let mut catalog_packages: HashSet<String> = HashSet::new();
    let mut newly_added: Vec<(String, PackageSource, bool)> = Vec::new();

    let mut reconciled_sections: Vec<SectionBlock> = Vec::new();

    for sec in &catalog.sections {
        let sec_key = sec.name.to_lowercase();
        let sec_checked = host_sections.get(&sec_key).copied().unwrap_or(sec.checked);

        let mut reconciled_categories: Vec<CategoryBlock> = Vec::new();

        for cat in &sec.categories {
            let cat_key = cat.name.as_ref().map(|n| n.to_lowercase());
            let cat_checked = host_categories
                .get(&(sec_key.clone(), cat_key))
                .copied()
                .unwrap_or(cat.checked);

            let mut reconciled_items: Vec<PackageItem> = Vec::new();

            for item in &cat.items {
                catalog_packages.insert(item.name.clone());

                let (item_checked, is_new) = if let Some(&checked) = host_items.get(&item.name) {
                    (checked, false)
                } else {
                    let default_checked = installed.is_installed(item.source, &item.name);
                    (default_checked, true)
                };

                if is_new {
                    newly_added.push((item.name.clone(), item.source, item_checked));
                }

                reconciled_items.push(PackageItem {
                    checked: item_checked,
                    source: item.source,
                    explicit_m_prefix: item.explicit_m_prefix,
                    name: item.name.clone(),
                    description: item.description.clone(),
                    line_no: item.line_no,
                });
            }

            reconciled_categories.push(CategoryBlock {
                name: cat.name.clone(),
                checked: cat_checked,
                line_no: cat.line_no,
                notes: cat.notes.clone(),
                items: reconciled_items,
            });
        }

        reconciled_sections.push(SectionBlock {
            name: sec.name.clone(),
            checked: sec_checked,
            description: sec.description.clone(),
            line_no: sec.line_no,
            categories: reconciled_categories,
        });
    }

    // Determine packages present in host but removed from catalog
    let mut removed_from_catalog: Vec<String> = Vec::new();
    for pkg in &host_packages {
        if !catalog_packages.contains(pkg) {
            removed_from_catalog.push(pkg.clone());
        }
    }
    removed_from_catalog.sort();

    let host_doc = Document {
        title: catalog.title.clone(),
        preamble: catalog.preamble.clone(),
        sections: reconciled_sections,
    };

    let structure_changed = match existing_host {
        None => true,
        Some(host) => render_document(&host_doc) != render_document(host),
    };

    ReconcileResult {
        host_doc,
        newly_added,
        removed_from_catalog,
        structure_changed,
    }
}

pub fn compute_sync_plan(
    host_doc: &Document,
    requested_sections: Option<&[String]>,
    installed: &InstalledPackages,
    force_aur: bool,
) -> Result<SyncPlan, String> {
    let mut to_install: Vec<PackageItem> = Vec::new();
    let mut to_remove: Vec<PackageItem> = Vec::new();
    let mut skipped_installed_aur: Vec<String> = Vec::new();

    let mut seen_install: HashSet<String> = HashSet::new();
    let mut seen_remove: HashSet<String> = HashSet::new();
    let mut seen_skipped_aur: HashSet<String> = HashSet::new();

    if let Some(req_secs) = requested_sections {
        if !req_secs.is_empty() {
            // Verify requested sections exist
            for req in req_secs {
                if !host_doc
                    .sections
                    .iter()
                    .any(|s| s.name.eq_ignore_ascii_case(req))
                {
                    return Err(format!("Section '{}' not found in document", req));
                }
            }
        }
    }

    for sec in &host_doc.sections {
        let in_scope = if let Some(req_secs) = requested_sections {
            if req_secs.is_empty() {
                sec.checked
            } else {
                req_secs.iter().any(|r| r.eq_ignore_ascii_case(&sec.name))
            }
        } else {
            sec.checked
        };

        if !in_scope {
            continue;
        }

        for cat in &sec.categories {
            let cat_enabled = cat.checked;

            for item in &cat.items {
                let effective_enabled = cat_enabled && item.checked;
                let is_inst = installed.is_installed(item.source, &item.name);

                if effective_enabled {
                    if !is_inst {
                        if seen_install.insert(item.name.clone()) {
                            to_install.push(item.clone());
                        }
                    } else if item.source == PackageSource::Aur {
                        if force_aur {
                            if seen_install.insert(item.name.clone()) {
                                to_install.push(item.clone());
                            }
                        } else if seen_skipped_aur.insert(item.name.clone()) {
                            skipped_installed_aur.push(item.name.clone());
                        }
                    }
                } else if is_inst {
                    if seen_remove.insert(item.name.clone()) {
                        to_remove.push(item.clone());
                    }
                }
            }
        }
    }

    Ok(SyncPlan {
        to_install,
        to_remove,
        skipped_installed_aur,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{parse_document, render_document};
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn reconcile_preserves_host_checks_and_defaults_new_packages_to_actual_installed_state() {
        let catalog_md = "\
## [x] MAIN — Core
### [x] cli
- [x] `git` — vcs
- [x] `ripgrep` — search
- [x] a:`google-chrome` — browser
";
        let host_md = "\
## [x] MAIN — Old desc
### [x] cli
- [ ] `git` — old vcs desc
";
        let (catalog, _) = parse_document(Path::new("packages.md"), catalog_md);
        let (host, _) = parse_document(Path::new("myhost.md"), host_md);
        let installed = InstalledPackages {
            repo_and_aur: HashSet::from(["git".to_string(), "ripgrep".to_string()]),
            npm: HashSet::new(),
        };

        let res = reconcile_catalog_to_host(&catalog, Some(&host), &installed);
        let items = &res.host_doc.sections[0].categories[0].items;
        // git was explicitly [ ] in host_md -> stays [ ] even though installed!
        assert!(!items[0].checked);
        assert_eq!(items[0].description, "vcs");
        // ripgrep is new -> installed on host -> defaults to [x]
        assert!(items[1].checked);
        // google-chrome is new -> not installed on host -> defaults to [ ]
        assert!(!items[2].checked);
        assert_eq!(res.newly_added.len(), 2);
    }

    #[test]
    fn unchecked_category_queues_installed_children_for_removal_without_mutating_item_checkboxes() {
        let host_md = "\
## [x] MAIN — Core
### [ ] disabled-cat
- [x] `git` — vcs
### [x] enabled-cat
- [x] `fd` — find
- [x] a:`insync` — sync
";
        let (host, _) = parse_document(Path::new("myhost.md"), host_md);
        let installed = InstalledPackages {
            repo_and_aur: HashSet::from(["git".to_string(), "insync".to_string()]),
            npm: HashSet::new(),
        };

        let plan = compute_sync_plan(&host, None, &installed, false).unwrap();
        assert_eq!(plan.to_install.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["fd"]);
        assert_eq!(plan.to_remove.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["git"]);
        assert_eq!(plan.skipped_installed_aur, vec!["insync".to_string()]);

        // Item checkbox inside disabled category remains [x] when rendered back
        let rendered = render_document(&host);
        assert!(rendered.contains("### [ ] disabled-cat\n- [x] `git` — vcs"));
    }
}
