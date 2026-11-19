use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackageSource {
    Repo,
    Aur,
    Apt,
    Npm,
    Custom,
}

impl PackageSource {
    pub fn prefix_str(&self) -> &'static str {
        match self {
            PackageSource::Repo => "",
            PackageSource::Aur => "a:",
            PackageSource::Apt => "d:",
            PackageSource::Npm => "n:",
            PackageSource::Custom => "c:",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            PackageSource::Repo => "repo",
            PackageSource::Aur => "aur",
            PackageSource::Apt => "apt",
            PackageSource::Npm => "npm",
            PackageSource::Custom => "custom",
        }
    }

    pub fn from_prefix(prefix: Option<&str>, legacy_aur: bool) -> Result<Self, String> {
        if legacy_aur {
            return Ok(PackageSource::Aur);
        }
        match prefix {
            None => Ok(PackageSource::Repo),
            Some("m") | Some("manjaro") | Some("repo") => Ok(PackageSource::Repo),
            Some("a") | Some("aur") => Ok(PackageSource::Aur),
            Some("d") | Some("apt") | Some("debian") | Some("ubuntu") => Ok(PackageSource::Apt),
            Some("n") | Some("npm") => Ok(PackageSource::Npm),
            Some("c") | Some("custom") => Ok(PackageSource::Custom),
            Some(unknown) => Err(format!("unknown source prefix: '{}:'", unknown)),
        }
    }
}

impl std::fmt::Display for PackageSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.prefix_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageItem {
    pub checked: bool,
    pub source: PackageSource,
    pub explicit_m_prefix: bool,
    pub name: String,
    pub description: String,
    pub line_no: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub file: PathBuf,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryBlock {
    pub name: Option<String>,
    pub checked: bool,
    pub line_no: usize,
    pub notes: Vec<String>,
    pub items: Vec<PackageItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionBlock {
    pub name: String,
    pub checked: bool,
    pub description: String,
    pub line_no: usize,
    pub categories: Vec<CategoryBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub title: Option<String>,
    pub preamble: Vec<String>,
    pub sections: Vec<SectionBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookStage {
    PreInstall,
    PostInstall,
    PreRemoval,
    PostRemoval,
    Install,
    Remove,
    Update,
}

impl HookStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookStage::PreInstall => "preInstall",
            HookStage::PostInstall => "postInstall",
            HookStage::PreRemoval => "preRemoval",
            HookStage::PostRemoval => "postRemoval",
            HookStage::Install => "install",
            HookStage::Remove => "remove",
            HookStage::Update => "update",
        }
    }

    pub fn from_stage_str(s: &str) -> Option<Self> {
        match s {
            "preInstall" => Some(HookStage::PreInstall),
            "postInstall" => Some(HookStage::PostInstall),
            "preRemoval" => Some(HookStage::PreRemoval),
            "postRemoval" => Some(HookStage::PostRemoval),
            "install" => Some(HookStage::Install),
            "remove" => Some(HookStage::Remove),
            "update" => Some(HookStage::Update),
            _ => None,
        }
    }
}

impl std::fmt::Display for HookStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseHookStageError(pub String);

impl std::fmt::Display for ParseHookStageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown hook stage: '{}'", self.0)
    }
}

impl std::error::Error for ParseHookStageError {}

impl std::str::FromStr for HookStage {
    type Err = ParseHookStageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_stage_str(s).ok_or_else(|| ParseHookStageError(s.to_string()))
    }
}

impl TryFrom<&str> for HookStage {
    type Error = ParseHookStageError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_display_and_from_str() {
        assert_eq!(PackageSource::Aur.to_string(), "a:");
        assert_eq!(PackageSource::Repo.to_string(), "");
        assert_eq!(PackageSource::Npm.to_string(), "n:");

        assert_eq!(HookStage::PreInstall.to_string(), "preInstall");
        assert_eq!(
            HookStage::from_str("postInstall"),
            Ok(HookStage::PostInstall)
        );
        assert_eq!(HookStage::try_from("preRemoval"), Ok(HookStage::PreRemoval));
        assert!(HookStage::from_str("invalid").is_err());
    }

    #[test]
    fn test_five_package_sources_and_seven_hook_stages() {
        assert_eq!(
            PackageSource::from_prefix(None, false),
            Ok(PackageSource::Repo)
        );
        assert_eq!(
            PackageSource::from_prefix(Some("m"), false),
            Ok(PackageSource::Repo)
        );
        assert_eq!(
            PackageSource::from_prefix(Some("a"), false),
            Ok(PackageSource::Aur)
        );
        assert_eq!(
            PackageSource::from_prefix(Some("d"), false),
            Ok(PackageSource::Apt)
        );
        assert_eq!(
            PackageSource::from_prefix(Some("apt"), false),
            Ok(PackageSource::Apt)
        );
        assert_eq!(
            PackageSource::from_prefix(Some("debian"), false),
            Ok(PackageSource::Apt)
        );
        assert_eq!(
            PackageSource::from_prefix(Some("ubuntu"), false),
            Ok(PackageSource::Apt)
        );
        assert_eq!(
            PackageSource::from_prefix(Some("n"), false),
            Ok(PackageSource::Npm)
        );
        assert_eq!(
            PackageSource::from_prefix(Some("c"), false),
            Ok(PackageSource::Custom)
        );
        assert_eq!(
            PackageSource::from_prefix(Some("custom"), false),
            Ok(PackageSource::Custom)
        );

        assert_eq!(PackageSource::Apt.prefix_str(), "d:");
        assert_eq!(PackageSource::Custom.prefix_str(), "c:");

        assert_eq!(PackageSource::Repo.short_label(), "repo");
        assert_eq!(PackageSource::Aur.short_label(), "aur");
        assert_eq!(PackageSource::Apt.short_label(), "apt");
        assert_eq!(PackageSource::Npm.short_label(), "npm");
        assert_eq!(PackageSource::Custom.short_label(), "custom");

        assert_eq!(HookStage::from_str("install"), Ok(HookStage::Install));
        assert_eq!(HookStage::from_str("remove"), Ok(HookStage::Remove));
        assert_eq!(HookStage::from_str("update"), Ok(HookStage::Update));
        assert_eq!(HookStage::Install.as_str(), "install");
        assert_eq!(HookStage::Remove.as_str(), "remove");
        assert_eq!(HookStage::Update.as_str(), "update");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookScript {
    pub package: String,
    pub stage: HookStage,
    pub path: PathBuf,
}
