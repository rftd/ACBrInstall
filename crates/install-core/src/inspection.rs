//! Inspeção de pastas sem executar os programas encontrados.
use crate::{config::AcbrAction, platform::EXE};
use anyhow::{Context, Result};
use std::{fs, path::Path};

#[derive(Default)]
pub struct EnvironmentInfo {
    pub fpc: bool,
    pub lazarus: bool,
    pub partial: bool,
}

impl EnvironmentInfo {
    pub fn complete(&self) -> bool {
        self.fpc && self.lazarus
    }

    pub fn summary(&self) -> &'static str {
        match (self.fpc, self.lazarus, self.partial) {
            (true, true, _) => "FPC e Lazarus encontrados. Ambiente existente disponível.",
            (true, false, _) => "FPC encontrado; Lazarus/lazbuild ausente. Instalação incompleta.",
            (false, true, _) => {
                "Lazarus encontrado; compilador FPC ausente. Instalação incompleta."
            }
            (false, false, true) => {
                "Há arquivos de uma instalação anterior, mas faltam FPC e lazbuild."
            }
            _ => "Nenhuma instalação de FPC/Lazarus encontrada nesta pasta.",
        }
    }
}

pub struct AcbrInfo {
    pub sources: bool,
    pub svn: bool,
    pub occupied: bool,
}

impl AcbrInfo {
    pub fn suggested_action(&self) -> Option<AcbrAction> {
        match (self.sources, self.svn, self.occupied) {
            (true, true, _) => Some(AcbrAction::UpdateAndInstall),
            (true, false, _) => Some(AcbrAction::Install),
            (false, _, false) => Some(AcbrAction::DownloadAndInstall),
            _ => None,
        }
    }

    pub fn summary(&self) -> &'static str {
        match (self.sources, self.svn, self.occupied) {
            (true, true, _) => "Fontes ACBr e cópia SVN encontrados. Pode atualizar e instalar.",
            (true, false, _) => "Fontes ACBr encontrados, sem SVN. Pode instalar os fontes locais.",
            (false, _, true) => "A pasta contém arquivos, mas não foram encontrados pacotes ACBr em Pacotes/Lazarus.",
            _ => "Nenhum fonte ACBr encontrado. Destino disponível para baixar.",
        }
    }
}

fn has_file(dir: &Path, names: &[&str], depth: usize) -> Result<bool> {
    if !dir.is_dir() {
        return Ok(false);
    }
    for name in names {
        if dir.join(format!("{name}{EXE}")).is_file() {
            return Ok(true);
        }
    }
    if depth > 0 {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && has_file(&entry.path(), names, depth - 1)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn environment(base: &Path) -> Result<EnvironmentInfo> {
    if base.exists() && !base.is_dir() {
        anyhow::bail!("O caminho de instalação não é uma pasta.");
    }
    let fpc = has_file(
        &base.join("fpc/bin"),
        &["fpc", "ppcx64", "ppc386", "ppca64"],
        2,
    )
    .context("não foi possível verificar o compilador FPC")?;
    Ok(EnvironmentInfo {
        fpc,
        lazarus: crate::platform::lazbuild(base).is_file(),
        partial: ["fpc", "fpcsrc", "lazarus", "config_lazarus", "_fpcup"]
            .iter()
            .any(|dir| base.join(dir).is_dir()),
    })
}

pub fn discover_lazarus() -> Vec<std::path::PathBuf> {
    let mut candidates = vec![crate::platform::default_dir()];
    if cfg!(windows) {
        candidates.push(std::path::PathBuf::from(r"C:\lazarus"));
    } else if cfg!(target_os = "macos") {
        candidates.push(std::path::PathBuf::from("/Applications/Lazarus"));
    } else {
        candidates.push(std::path::PathBuf::from("/usr/share/lazarus"));
        if let Ok(versions) = fs::read_dir("/usr/lib/lazarus") {
            candidates.extend(versions.flatten().map(|e| e.path()));
        }
    }
    candidates.retain(|p| crate::platform::lazbuild(p).is_file());
    candidates.sort();
    candidates.dedup();
    candidates
}

fn has_package(dir: &Path, depth: usize, extension: &str) -> Result<bool> {
    if !dir.is_dir() {
        return Ok(false);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
        {
            return Ok(true);
        }
        if kind.is_dir() && depth > 0 && has_package(&entry.path(), depth - 1, extension)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn acbr(dir: &Path) -> Result<AcbrInfo> {
    acbr_for(dir, crate::config::Ide::Lazarus)
}

pub fn acbr_for(dir: &Path, ide: crate::config::Ide) -> Result<AcbrInfo> {
    if dir.exists() && !dir.is_dir() {
        anyhow::bail!("O caminho dos fontes não é uma pasta.");
    }
    Ok(AcbrInfo {
        sources: has_package(
            &dir.join(if ide == crate::config::Ide::Delphi {
                "Pacotes/Delphi"
            } else {
                "Pacotes/Lazarus"
            }),
            8,
            if ide == crate::config::Ide::Delphi {
                "dpk"
            } else {
                "lpk"
            },
        )
        .context("não foi possível verificar os pacotes ACBr")?,
        svn: dir.join(".svn").is_dir(),
        occupied: dir.is_dir() && fs::read_dir(dir)?.next().transpose()?.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("acbr-inspection-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn file(&self, name: &str) {
            let file = self.0.join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, "fixture").unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn new_folder_has_no_environment_and_can_download_acbr() {
        let fixture = Fixture::new();
        for path in [&fixture.0, &fixture.0.join("new")] {
            let info = environment(path).unwrap();
            assert!(!info.complete() && !info.partial);
            assert_eq!(
                acbr(path).unwrap().suggested_action(),
                Some(AcbrAction::DownloadAndInstall)
            );
        }
    }

    #[test]
    fn requires_compiler_and_lazbuild_before_reusing_environment() {
        let fixture = Fixture::new();
        fixture.file("fpcsrc/compiler/version.pas");
        let info = environment(&fixture.0).unwrap();
        assert!(info.partial && !info.complete());

        fixture.file(&format!("lazarus/lazbuild{EXE}"));
        let info = environment(&fixture.0).unwrap();
        assert!(info.lazarus && !info.fpc && !info.complete());

        fixture.file(&format!("fpc/bin/x86_64-linux/fpc{EXE}"));
        assert!(environment(&fixture.0).unwrap().complete());
    }

    #[test]
    fn detects_nested_acbr_packages_and_selects_svn_update_only_when_available() {
        let fixture = Fixture::new();
        let path = fixture.0.join("ACBr");
        fixture.file("ACBr/Pacotes/Lazarus/ACBrComum/ACBrComum.LPK");
        let info = acbr(&path).unwrap();
        assert!(info.sources && !info.svn);
        assert_eq!(info.suggested_action(), Some(AcbrAction::Install));

        fs::create_dir(path.join(".svn")).unwrap();
        let info = acbr(&path).unwrap();
        assert!(info.sources && info.svn);
        assert_eq!(info.suggested_action(), Some(AcbrAction::UpdateAndInstall));
    }

    #[test]
    fn unrelated_files_and_empty_package_directories_are_not_acbr_sources() {
        let fixture = Fixture::new();
        fixture.file("notes.txt");
        fs::create_dir_all(fixture.0.join("Pacotes/Lazarus")).unwrap();
        let info = acbr(&fixture.0).unwrap();
        assert!(!info.sources && info.occupied);
        assert_eq!(info.suggested_action(), None);
        assert!(acbr(&fixture.0.join("notes.txt")).is_err());
        assert!(environment(&fixture.0.join("notes.txt")).is_err());
    }
}
