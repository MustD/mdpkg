use crate::model::{
    CategoryBlock, Document, PackageItem, PackageSource, SectionBlock, ValidationIssue,
};
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

static RE_SECTION: OnceLock<Regex> = OnceLock::new();
static RE_CATEGORY: OnceLock<Regex> = OnceLock::new();
static RE_ITEM: OnceLock<Regex> = OnceLock::new();
static RE_POTENTIAL_ITEM: OnceLock<Regex> = OnceLock::new();

fn section_regex() -> &'static Regex {
    RE_SECTION.get_or_init(|| {
        // e.g. ## [x] MAIN — Basic packages, ## [x] DEV-TOOLS — Development, or ## MAIN
        Regex::new(r#"^##\s*(?:\[([ xX])\]\s*)?(.*?)(?:(?:\s*—\s*|\s*--\s*|\s+-\s+)(.*))?$"#)
            .unwrap()
    })
}

fn category_regex() -> &'static Regex {
    RE_CATEGORY.get_or_init(|| {
        // e.g. ### [x] core CLI / shell or ### core CLI
        Regex::new(r#"^###\s*(?:\[([ xX])\]\s*)?(.*?)\s*$"#).unwrap()
    })
}

fn item_regex() -> &'static Regex {
    RE_ITEM.get_or_init(|| {
        // e.g. - [x] `git` — version control
        // - [x] m:`neovim` — vim editor
        // - [x] Ⓐ`google-chrome` — Chrome browser
        // - [x] a:`insync` — GDrive sync
        // - [ ] n:`typescript` — TS compiler
        // - [ ] `dpkg` - debian package management
        // - `foo`
        Regex::new(r#"^\s*-\s*(?:\[([ xX])\]\s+)?(?:([a-zA-Z0-9_-]+):|(Ⓐ))?\s*`([^`]+)`(?:\s*(?:—|--|-)\s*(.*))?$"#)
            .unwrap()
    })
}

fn potential_item_regex() -> &'static Regex {
    RE_POTENTIAL_ITEM.get_or_init(|| {
        // Any bullet line starting with - [x], - [ ], or just -
        Regex::new(r#"^\s*-\s*(?:\[[ xX]\]\s*)?.*"#).unwrap()
    })
}

fn flush_category(
    current_category: &mut Option<CategoryBlock>,
    current_section: &mut Option<SectionBlock>,
) {
    if let Some(cat) = current_category.take() {
        if let Some(sec) = current_section {
            if cat.name.is_some()
                || !cat.items.is_empty()
                || cat.notes.iter().any(|n| !n.trim().is_empty())
            {
                sec.categories.push(cat);
            }
        }
    }
}

pub fn parse_document(path: &Path, content: &str) -> (Document, Vec<ValidationIssue>) {
    let mut doc = Document {
        title: None,
        preamble: Vec::new(),
        sections: Vec::new(),
    };
    let mut issues = Vec::new();
    let mut seen_packages: HashSet<String> = HashSet::new();

    let sec_re = section_regex();
    let cat_re = category_regex();
    let item_re = item_regex();
    let pot_re = potential_item_regex();

    let mut current_section: Option<SectionBlock> = None;
    let mut current_category: Option<CategoryBlock> = None;
    let mut in_preamble = true;

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();

        // Check for document title: # Title (only if at the top level outside sections)
        if trimmed.starts_with("# ")
            && !trimmed.starts_with("## ")
            && !trimmed.starts_with("### ")
            && doc.title.is_none()
            && current_section.is_none()
        {
            doc.title = Some(trimmed[2..].trim().to_string());
            continue;
        }

        // Check for Category: ### ... (check ### before ## because ### starts with ##)
        if trimmed.starts_with("### ") || trimmed == "###" {
            in_preamble = false;
            if current_section.is_none() {
                issues.push(ValidationIssue {
                    file: path.to_path_buf(),
                    line: line_no,
                    message: format!("Category defined before any section: '{}'", line),
                });
            }

            // Flush previous category into current section
            flush_category(&mut current_category, &mut current_section);

            if let Some(caps) = cat_re.captures(trimmed) {
                let checked = caps
                    .get(1)
                    .map_or(true, |m| m.as_str().eq_ignore_ascii_case("x"));
                let name = caps
                    .get(2)
                    .map(|m| m.as_str().trim().to_string())
                    .filter(|s| !s.is_empty());

                current_category = Some(CategoryBlock {
                    name,
                    checked,
                    line_no,
                    notes: Vec::new(),
                    items: Vec::new(),
                });
            } else {
                issues.push(ValidationIssue {
                    file: path.to_path_buf(),
                    line: line_no,
                    message: format!("Malformed category heading: '{}'", line),
                });
            }
            continue;
        }

        // Check for Section: ## ...
        if trimmed.starts_with("## ") || trimmed == "##" {
            in_preamble = false;
            // Flush current category into current section
            flush_category(&mut current_category, &mut current_section);
            // Flush current section into doc
            if let Some(sec) = current_section.take() {
                doc.sections.push(sec);
            }

            if let Some(caps) = sec_re.captures(trimmed) {
                let checked = caps
                    .get(1)
                    .map_or(true, |m| m.as_str().eq_ignore_ascii_case("x"));
                let name = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
                let desc = caps.get(3).map_or("", |m| m.as_str()).trim().to_string();

                current_section = Some(SectionBlock {
                    name,
                    checked,
                    description: desc,
                    line_no,
                    categories: Vec::new(),
                });
            } else {
                issues.push(ValidationIssue {
                    file: path.to_path_buf(),
                    line: line_no,
                    message: format!("Malformed section heading: '{}'", line),
                });
            }
            continue;
        }

        // Check if line might be an item (fast-path filter avoids regex if line does not start with '-')
        if trimmed.starts_with('-') && pot_re.is_match(trimmed) {
            // Check if before section
            if current_section.is_none() {
                issues.push(ValidationIssue {
                    file: path.to_path_buf(),
                    line: line_no,
                    message: format!("Package item before section: '{}'", line),
                });
                continue;
            }

            // Try parsing as package item
            if let Some(caps) = item_re.captures(trimmed) {
                let checked = caps
                    .get(1)
                    .map_or(true, |m| m.as_str().eq_ignore_ascii_case("x"));
                let prefix = caps.get(2).map(|m| m.as_str());
                let legacy_aur = caps.get(3).is_some();
                let pkg_name = caps.get(4).unwrap().as_str().trim().to_string();
                let desc = caps.get(5).map_or("", |m| m.as_str()).trim().to_string();

                let source_res = PackageSource::from_prefix(prefix, legacy_aur);
                let source = match source_res {
                    Ok(src) => src,
                    Err(err) => {
                        issues.push(ValidationIssue {
                            file: path.to_path_buf(),
                            line: line_no,
                            message: format!("{}: '{}'", err, line),
                        });
                        continue;
                    }
                };

                let explicit_m_prefix =
                    matches!(prefix, Some("m") | Some("manjaro") | Some("repo"));

                if !seen_packages.insert(pkg_name.clone()) {
                    issues.push(ValidationIssue {
                        file: path.to_path_buf(),
                        line: line_no,
                        message: format!("Duplicate package '{}' found", pkg_name),
                    });
                }

                let item = PackageItem {
                    checked,
                    source,
                    explicit_m_prefix,
                    name: pkg_name,
                    description: desc,
                    line_no,
                };

                // Ensure we have a category in the current section
                if current_category.is_none() {
                    current_category = Some(CategoryBlock {
                        name: None,
                        checked: true,
                        line_no,
                        notes: Vec::new(),
                        items: Vec::new(),
                    });
                }
                current_category.as_mut().unwrap().items.push(item);
                continue;
            } else {
                // Malformed item line
                issues.push(ValidationIssue {
                    file: path.to_path_buf(),
                    line: line_no,
                    message: format!("Malformed package item line: '{}'", line),
                });
                continue;
            }
        }

        // Otherwise it is preamble or notes
        if in_preamble {
            doc.preamble.push(line.to_string());
        } else if let Some(cat) = &mut current_category {
            cat.notes.push(line.to_string());
        } else if let Some(_sec) = &mut current_section {
            // Notes under section before first category -> put into a default category or section notes
            if !trimmed.is_empty() {
                if current_category.is_none() {
                    current_category = Some(CategoryBlock {
                        name: None,
                        checked: true,
                        line_no,
                        notes: Vec::new(),
                        items: Vec::new(),
                    });
                }
                current_category
                    .as_mut()
                    .unwrap()
                    .notes
                    .push(line.to_string());
            }
        }
    }

    // Flush remaining category and section
    flush_category(&mut current_category, &mut current_section);
    if let Some(sec) = current_section.take() {
        doc.sections.push(sec);
    }

    (doc, issues)
}

pub fn validate_document(path: &Path, content: &str) -> Vec<ValidationIssue> {
    let (_, issues) = parse_document(path, content);
    issues
}

pub fn render_document(doc: &Document) -> String {
    use std::fmt::Write;

    let hint = doc.preamble.iter().map(|l| l.len() + 1).sum::<usize>()
        + doc.sections.len() * 64
        + doc
            .sections
            .iter()
            .map(|s| {
                s.categories.len() * 32
                    + s.categories
                        .iter()
                        .map(|c| {
                            c.items.len() * 48 + c.notes.iter().map(|n| n.len() + 1).sum::<usize>()
                        })
                        .sum::<usize>()
            })
            .sum::<usize>()
        + 128;
    let mut out = String::with_capacity(hint);

    if let Some(title) = &doc.title {
        let _ = write!(out, "# {}\n\n", title);
    }

    for line in &doc.preamble {
        out.push_str(line);
        out.push('\n');
    }

    for (s_idx, section) in doc.sections.iter().enumerate() {
        if (s_idx > 0 || !doc.preamble.is_empty() || doc.title.is_some())
            && !out.ends_with("\n\n")
            && !out.is_empty()
        {
            if out.ends_with('\n') {
                out.push('\n');
            } else {
                out.push_str("\n\n");
            }
        }

        let chk = if section.checked { "[x]" } else { "[ ]" };
        if section.description.is_empty() {
            let _ = writeln!(out, "## {} {}", chk, section.name);
        } else {
            let _ = writeln!(out, "## {} {} — {}", chk, section.name, section.description);
        }

        for category in &section.categories {
            out.push('\n');
            if let Some(cat_name) = &category.name {
                let cat_chk = if category.checked { "[x]" } else { "[ ]" };
                let _ = writeln!(out, "### {} {}", cat_chk, cat_name);
            }

            for note in &category.notes {
                out.push_str(note);
                out.push('\n');
            }

            for item in &category.items {
                let item_chk = if item.checked { "[x]" } else { "[ ]" };
                let prefix = match item.source {
                    PackageSource::Repo => {
                        if item.explicit_m_prefix {
                            "m:"
                        } else {
                            ""
                        }
                    }
                    PackageSource::Aur => "a:",
                    PackageSource::Npm => "n:",
                };

                if item.description.is_empty() {
                    let _ = writeln!(out, "- {} {}`{}`", item_chk, prefix, item.name);
                } else {
                    let _ = writeln!(
                        out,
                        "- {} {}`{}` — {}",
                        item_chk, prefix, item.name, item.description
                    );
                }
            }
        }
    }

    out
}

pub fn insert_package_item(doc: &mut Document, section: &str, category: &str, item: PackageItem) {
    let sec = if let Some(pos) = doc
        .sections
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case(section))
    {
        &mut doc.sections[pos]
    } else {
        doc.sections.push(SectionBlock {
            name: section.to_string(),
            checked: true,
            description: String::new(),
            line_no: 0,
            categories: Vec::new(),
        });
        doc.sections.last_mut().unwrap()
    };

    let cat = if let Some(pos) = sec.categories.iter().position(|c| {
        c.name
            .as_deref()
            .unwrap_or("")
            .eq_ignore_ascii_case(category)
    }) {
        &mut sec.categories[pos]
    } else {
        sec.categories.push(CategoryBlock {
            name: Some(category.to_string()),
            checked: true,
            line_no: 0,
            notes: Vec::new(),
            items: Vec::new(),
        });
        sec.categories.last_mut().unwrap()
    };

    cat.items.push(item);
}

pub fn set_package_checked(doc: &mut Document, pkg_name: &str, checked: bool) -> bool {
    let mut found = false;
    for sec in &mut doc.sections {
        for cat in &mut sec.categories {
            for item in &mut cat.items {
                if item.name == pkg_name {
                    item.checked = checked;
                    found = true;
                }
            }
        }
    }
    found
}

pub fn remove_package_item(doc: &mut Document, pkg_name: &str) -> bool {
    let mut removed = false;
    for sec in &mut doc.sections {
        for cat in &mut sec.categories {
            let initial_len = cat.items.len();
            cat.items.retain(|item| item.name != pkg_name);
            if cat.items.len() < initial_len {
                removed = true;
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PackageSource;
    use std::path::Path;

    #[test]
    fn parses_sources_and_normalizes_legacy_aur_and_hyphen_descriptions() {
        let input = "\
# Manjaro packages

> **Format legend**

## [x] MAIN — Basic packages

### [x] core CLI
- [x] `git` — version control
- [x] m:`neovim` — vim editor
- [x] Ⓐ`google-chrome` — Chrome browser
- [x] a:`insync` — GDrive sync
- [ ] n:`typescript` — TS compiler
- [ ] `dpkg` - debian package management
";
        let (doc, issues) = parse_document(Path::new("packages.md"), input);
        assert!(issues.is_empty(), "unexpected issues: {:?}", issues);
        let items = &doc.sections[0].categories[0].items;
        assert_eq!(items.len(), 6);
        assert_eq!(
            (items[0].name.as_str(), items[0].source),
            ("git", PackageSource::Repo)
        );
        assert_eq!(
            (
                items[1].name.as_str(),
                items[1].source,
                items[1].explicit_m_prefix
            ),
            ("neovim", PackageSource::Repo, true)
        );
        assert_eq!(
            (items[2].name.as_str(), items[2].source),
            ("google-chrome", PackageSource::Aur)
        );
        assert_eq!(
            (items[3].name.as_str(), items[3].source),
            ("insync", PackageSource::Aur)
        );
        assert_eq!(
            (items[4].name.as_str(), items[4].source, items[4].checked),
            ("typescript", PackageSource::Npm, false)
        );
        assert_eq!(
            (items[5].name.as_str(), items[5].description.as_str()),
            ("dpkg", "debian package management")
        );

        let rendered = render_document(&doc);
        assert!(rendered.contains("- [x] a:`google-chrome` — Chrome browser"));
        assert!(!rendered.contains("Ⓐ"));
    }

    #[test]
    fn validate_catches_syntax_errors_and_duplicates_with_line_numbers() {
        let bad = "\
- [x] `orphan-before-section` — bad
## [x] MAIN — desc
### [x] tools
- [x] unknown:`foo` — bad source
- [x] broken_no_backticks — bad line
- [x] `git` — first
- [ ] `git` — duplicate
";
        let issues = validate_document(Path::new("packages.md"), bad);
        let lines: Vec<usize> = issues.iter().map(|i| i.line).collect();
        assert_eq!(lines, vec![1, 4, 5, 7]);
    }

    #[test]
    fn test_hyphenated_section_name() {
        let input =
            "## [x] DEV-TOOLS — Development packages\n### [x] tools\n- [x] `cargo` — rust\n";
        let (doc, issues) = parse_document(Path::new("packages.md"), input);
        assert!(issues.is_empty());
        assert_eq!(doc.sections[0].name, "DEV-TOOLS");
        assert_eq!(doc.sections[0].description, "Development packages");
    }

    #[test]
    fn test_insert_set_checked_and_remove_item() {
        let input = "\
## [x] MAIN — Core

### [x] cli
- [x] `git` — vcs
";
        let (mut doc, _) = parse_document(Path::new("packages.md"), input);
        assert!(set_package_checked(&mut doc, "git", false));
        assert!(!doc.sections[0].categories[0].items[0].checked);

        insert_package_item(
            &mut doc,
            "MAIN",
            "cli",
            PackageItem {
                checked: true,
                source: PackageSource::Aur,
                explicit_m_prefix: false,
                name: "yay".to_string(),
                description: "aur helper".to_string(),
                line_no: 0,
            },
        );
        assert_eq!(doc.sections[0].categories[0].items.len(), 2);
        assert_eq!(doc.sections[0].categories[0].items[1].name, "yay");

        assert!(remove_package_item(&mut doc, "git"));
        assert_eq!(doc.sections[0].categories[0].items.len(), 1);
        assert_eq!(doc.sections[0].categories[0].items[0].name, "yay");
        assert!(!remove_package_item(&mut doc, "nonexistent"));
    }
}
