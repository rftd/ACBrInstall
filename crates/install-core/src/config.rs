use crate::platform;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ide {
    #[default]
    Lazarus,
    Delphi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AcbrAction {
    #[serde(rename = "install")]
    Install,
    #[serde(rename = "download")]
    DownloadAndInstall,
    #[serde(rename = "update")]
    UpdateAndInstall,
}
impl AcbrAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Install => "Apenas instalar",
            Self::DownloadAndInstall => "Baixar e instalar",
            Self::UpdateAndInstall => "Atualizar e instalar",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AcbrConfig {
    pub base: PathBuf,
    pub ide: Ide,
    pub delphi: DelphiOptions,
    pub acbr_dir: Option<PathBuf>,
    pub acbr_action: AcbrAction,
    pub acbr_packages: Option<Vec<String>>,
    pub all_components: bool,
    pub fastreport_dir: Option<PathBuf>,
}
impl Default for AcbrConfig {
    fn default() -> Self {
        Self {
            base: platform::default_dir(),
            ide: Ide::Lazarus,
            delphi: DelphiOptions::default(),
            acbr_dir: Some(platform::default_acbr_dir()),
            acbr_action: AcbrAction::DownloadAndInstall,
            acbr_packages: None,
            all_components: false,
            fastreport_dir: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PascalConfig {
    pub base: PathBuf,
    pub fpc: String,
    pub laz: String,
    pub cross: Vec<String>,
}
impl Default for PascalConfig {
    fn default() -> Self {
        Self {
            base: platform::default_dir(),
            fpc: "stable".into(),
            laz: "stable".into(),
            cross: platform::default_cross(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DelphiOptions {
    pub root: PathBuf,
    pub registry_key: String,
    pub win64: bool,
    pub debug: bool,
    pub ide64: bool,
}
impl Default for DelphiOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            registry_key: String::new(),
            win64: false,
            debug: false,
            ide64: false,
        }
    }
}
