//! Infraestrutura compartilhada e instaladores independentes de Pascal e ACBr.
pub mod acbr;
pub mod cli;
pub mod config;
pub mod config_store;
pub mod delphi;
pub mod inspection;
mod opm;
mod package_paths;
pub mod packages;
pub mod pascal;
pub mod platform;
pub mod runtime;
pub mod versions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Product {
    Acbr,
    Pascal,
}

impl Product {
    pub fn application_id(self) -> &'static str {
        match self {
            Self::Acbr => "dev.rftd.acbrinstall",
            Self::Pascal => "dev.rftd.pascalinstall",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Acbr => "ACBrInstall",
            Self::Pascal => "PascalInstall",
        }
    }
    pub fn executable(self) -> &'static str {
        match self {
            Self::Acbr => "acbrinstall",
            Self::Pascal => "pascalinstall",
        }
    }
    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Acbr => "Componentes ACBr para Delphi e Lazarus.",
            Self::Pascal => "FPC, Lazarus e compiladores para seu ambiente Pascal.",
        }
    }
}
