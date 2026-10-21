use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackageSource {
    Repo,
    Aur,
    Npm,
}

impl PackageSource {
    pub fn prefix_str(&self) -> &'static str {
        match self {
            PackageSource::Repo => "",
            PackageSource::Aur => "a:",
            PackageSource::Npm => "n:",
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
            Some("n") | Some("npm") => Ok(PackageSource::Npm),
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
}

impl HookStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookStage::PreInstall => "preInstall",
            HookStage::PostInstall => "postInstall",
            HookStage::PreRemoval => "preRemoval",
            HookStage::PostRemoval => "postRemoval",
        }
    }

    pub fn from_stage_str(s: &str) -> Option<Self> {
        match s {
            "preInstall" => Some(HookStage::PreInstall),
            "postInstall" => Some(HookStage::PostInstall),
            "preRemoval" => Some(HookStage::PreRemoval),
            "postRemoval" => Some(HookStage::PostRemoval),
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
        assert_eq!(HookStage::from_str("postInstall"), Ok(HookStage::PostInstall));
        assert_eq!(HookStage::try_from("preRemoval"), Ok(HookStage::PreRemoval));
        assert!(HookStage::from_str("invalid").is_err());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookScript {
    pub package: String,
    pub stage: HookStage,
    pub path: PathBuf,
}
