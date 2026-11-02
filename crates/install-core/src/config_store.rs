//! Configurações independentes, com migração dos dados do instalador anterior.
use crate::{
    config::{AcbrConfig, PascalConfig},
    Product,
};
use anyhow::{Context, Result};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn path(product: Product) -> Result<PathBuf> {
    if cfg!(windows) {
        let root = env::var_os("APPDATA")
            .filter(|p| !p.is_empty())
            .context("APPDATA não definido")?;
        Ok(PathBuf::from(root)
            .join(product.title())
            .join("config.json"))
    } else if cfg!(target_os = "macos") {
        let home = env::var_os("HOME").context("HOME não definido")?;
        Ok(PathBuf::from(home)
            .join("Library/Application Support")
            .join(product.title())
            .join("config.json"))
    } else {
        let root = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| {
                env::var_os("HOME")
                    .filter(|p| !p.is_empty())
                    .map(|p| PathBuf::from(p).join(".config"))
            })
            .context("HOME e XDG_CONFIG_HOME não definidos")?;
        Ok(root.join(product.executable()).join("config.json"))
    }
}

fn load_from<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("lendo {}", path.display())),
    };
    Ok(Some(serde_json::from_slice(&bytes).with_context(|| {
        format!("JSON inválido em {}", path.display())
    })?))
}
pub fn load_acbr() -> Result<Option<AcbrConfig>> {
    load_from(&path(Product::Acbr)?)
}

fn load_legacy_pascal(path: &Path) -> Result<Option<PascalConfig>> {
    let Some(value) = load_from::<serde_json::Value>(path)? else {
        return Ok(None);
    };
    if value.get("fpc").is_some() || value.get("laz").is_some() {
        Ok(Some(serde_json::from_value(value)?))
    } else {
        Ok(None)
    }
}
pub fn load_pascal() -> Result<Option<PascalConfig>> {
    if let Some(cfg) = load_from(&path(Product::Pascal)?)? {
        return Ok(Some(cfg));
    }
    // A migração só lê o arquivo antigo: cada programa salva no seu diretório.
    load_legacy_pascal(&path(Product::Acbr)?)
}
pub fn save_acbr(cfg: &AcbrConfig) -> Result<PathBuf> {
    // Preserva versões FPC/Lazarus do formato antigo antes de salvar só ACBr.
    let old = path(Product::Acbr)?;
    let pascal = path(Product::Pascal)?;
    migrate_pascal(&old, &pascal)?;
    save(Product::Acbr, cfg)
}

fn migrate_pascal(old: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        return Ok(());
    }
    if let Some(cfg) = load_legacy_pascal(old)? {
        save_to(destination, &cfg)?;
    }
    Ok(())
}
pub fn save_pascal(cfg: &PascalConfig) -> Result<PathBuf> {
    save(Product::Pascal, cfg)
}
fn save(product: Product, cfg: &impl serde::Serialize) -> Result<PathBuf> {
    let path = path(product)?;
    save_to(&path, cfg)?;
    Ok(path)
}

fn save_to(path: &Path, cfg: &impl serde::Serialize) -> Result<()> {
    let parent = path.parent().context("pasta de configuração inválida")?;
    fs::create_dir_all(parent).with_context(|| format!("criando {}", parent.display()))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temp = parent.join(format!(".config-{}-{stamp}.tmp", std::process::id()));
    let mut bytes = serde_json::to_vec_pretty(cfg)?;
    bytes.push(b'\n');
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.with_context(|| format!("salvando {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AcbrAction, Ide};
    #[test]
    fn migrates_legacy_configuration_and_keeps_products_independent() {
        let legacy = r#"{"base":"/old/env","fpc":"3.2.2","laz":"4.6","cross":["win64"],"skip_fpc":false,"skip_acbr":true,"acbr_dir":"/ACBr","acbr_action":"install","acbr_packages":["ACBr_NFe"]}"#;
        let acbr: AcbrConfig = serde_json::from_str(legacy).unwrap();
        let pascal: PascalConfig = serde_json::from_str(legacy).unwrap();
        assert_eq!(acbr.ide, Ide::Lazarus);
        assert_eq!(acbr.acbr_action, AcbrAction::Install);
        assert_eq!(acbr.acbr_packages, Some(vec!["ACBr_NFe".into()]));
        assert_eq!(pascal.fpc, "3.2.2");
        assert_eq!(pascal.cross, ["win64"]);
        assert!(!serde_json::to_string(&acbr).unwrap().contains("skip_fpc"));
        assert!(!serde_json::to_string(&pascal)
            .unwrap()
            .contains("acbr_action"));
        assert_ne!(path(Product::Acbr).unwrap(), path(Product::Pascal).unwrap());
        let root = env::temp_dir().join(format!(
            "install-config-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = root.join("config.json");
        fs::create_dir_all(&root).unwrap();
        fs::write(&file, legacy).unwrap();
        let migrated = root.join("pascal/config.json");
        migrate_pascal(&file, &migrated).unwrap();
        assert_eq!(
            load_from::<PascalConfig>(&migrated).unwrap().unwrap(),
            pascal
        );
        let custom = PascalConfig {
            fpc: "custom-version".into(),
            ..pascal.clone()
        };
        save_to(&migrated, &custom).unwrap();
        migrate_pascal(&file, &migrated).unwrap();
        assert_eq!(
            load_from::<PascalConfig>(&migrated).unwrap().unwrap(),
            custom
        );
        save_to(&file, &acbr).unwrap();
        assert_eq!(load_from::<AcbrConfig>(&file).unwrap().unwrap(), acbr);
        assert!(load_legacy_pascal(&file).unwrap().is_none());
        save_to(&file, &pascal).unwrap();
        assert_eq!(load_from::<PascalConfig>(&file).unwrap().unwrap(), pascal);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::write(&file, "invalid").unwrap();
        assert!(load_from::<AcbrConfig>(&file).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
