//! Corrige a capitalização de caminhos existentes nos pacotes ACBr no Linux.
use crate::runtime::Ctx;
use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn existing_path(base: &Path, raw: &str) -> Result<Option<String>> {
    if raw.is_empty() || raw.contains("$(") || raw.contains(':') {
        return Ok(None);
    }
    let normalized = raw.replace('\\', "/");
    let mut cursor = if normalized.starts_with('/') {
        PathBuf::from("/")
    } else {
        base.to_path_buf()
    };
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || matches!(part, "." | "..") {
            cursor.push(part);
            parts.push(part.to_string());
            continue;
        }
        if cursor.join(part).exists() {
            cursor.push(part);
            parts.push(part.to_string());
            continue;
        }
        if !cursor.is_dir() {
            return Ok(None);
        }
        let mut matches = Vec::new();
        for entry in fs::read_dir(&cursor)? {
            let name = entry?.file_name();
            if name.to_string_lossy().eq_ignore_ascii_case(part) {
                matches.push(name);
            }
        }
        if matches.len() > 1 {
            bail!(
                "Caminho ambíguo em {}: mais de uma entrada corresponde a {part}",
                cursor.display()
            );
        }
        let Some(name) = matches.pop() else {
            return Ok(None);
        };
        parts.push(name.to_string_lossy().into_owned());
        cursor.push(name);
    }
    if !cursor.exists() {
        return Ok(None);
    }
    let separator = if raw.contains('\\') { "\\" } else { "/" };
    Ok(Some(parts.join(separator)))
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn corrected_xml(package: &Path, xml: &str) -> Result<Option<String>> {
    let doc =
        roxmltree::Document::parse(xml).with_context(|| format!("lendo {}", package.display()))?;
    let base = package.parent().context("pacote sem diretório")?;
    let mut edits = Vec::new();
    for node in doc.descendants() {
        let filename =
            node.has_tag_name("Filename") && node.ancestors().any(|p| p.has_tag_name("Files"));
        let search = matches!(
            node.tag_name().name(),
            "OtherUnitFiles" | "IncludeFiles" | "SrcPath" | "UnitPath"
        );
        if !filename && !search {
            continue;
        }
        let Some(attr) = node.attributes().find(|a| a.name() == "Value") else {
            continue;
        };
        let original = attr.value();
        let mut paths = Vec::new();
        for raw in original.split(';') {
            paths.push(existing_path(base, raw)?.unwrap_or_else(|| raw.into()));
        }
        let corrected = paths.join(";");
        if corrected != original {
            edits.push((attr.range_value(), escape_attribute(&corrected)));
        }
    }
    if edits.is_empty() {
        return Ok(None);
    }
    let mut updated = xml.to_string();
    for (range, value) in edits.into_iter().rev() {
        updated.replace_range(range, &value);
    }
    Ok(Some(updated))
}

pub fn corrigir(ctx: &Ctx, acbr: &Path, packages: &[PathBuf]) -> Result<()> {
    if cfg!(windows) {
        return Ok(());
    }
    let mut changes = Vec::new();
    for package in packages {
        let xml =
            fs::read_to_string(package).with_context(|| format!("lendo {}", package.display()))?;
        if let Some(updated) = corrected_xml(package, &xml)? {
            changes.push((package, updated));
        }
    }
    if changes.is_empty() {
        return Ok(());
    }
    ctx.step("Corrigindo capitalização dos caminhos dos pacotes ACBr");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let backup = acbr
        .join("Lib/.acbrinstall-backup")
        .join(format!("{stamp}-paths-{}", std::process::id()));
    ctx.say(format!(
        "  Backup dos pacotes originais: {}",
        backup.display()
    ));
    for (package, updated) in changes {
        let saved = backup.join(
            package
                .strip_prefix(acbr)
                .context("pacote fora dos fontes ACBr")?,
        );
        fs::create_dir_all(saved.parent().unwrap())?;
        fs::copy(package, &saved)?;
        let temp = package.with_extension(format!("lpk.{stamp}.tmp"));
        let result = (|| -> Result<()> {
            fs::write(&temp, updated)?;
            fs::set_permissions(&temp, fs::metadata(package)?.permissions())?;
            fs::rename(&temp, package)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result.with_context(|| format!("corrigindo caminhos de {}", package.display()))?;
        ctx.say(format!("  Caminhos corrigidos: {}", package.display()));
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root =
                std::env::temp_dir().join(format!("acbr-paths-{}-{stamp}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn fixes_search_paths_and_filenames_with_backup_and_preserves_xml() {
        let fixture = Fixture::new();
        let sources = fixture.0.join("Fontes/DACTE/FPDF & outros");
        fs::create_dir_all(&sources).unwrap();
        fs::write(sources.join("Reg.pas"), "unit Reg;").unwrap();
        let case_sensitive = !sources.join("reg.pas").exists();
        let package = fixture.0.join("Pacotes/report.lpk");
        fs::create_dir_all(package.parent().unwrap()).unwrap();
        let xml = "<CONFIG>\r\n<!-- preserve -->\r\n<Package><SearchPaths><OtherUnitFiles Value=\"..\\Fontes\\DACTe\\FPDF &amp; outros;$(PkgOutDir);missing\"/></SearchPaths><Files><Item1><Filename Value=\"..\\Fontes\\DACTe\\FPDF &amp; outros\\reg.pas\"/></Item1></Files></Package>\r\n</CONFIG>\r\n";
        fs::write(&package, xml).unwrap();
        let ctx = Ctx::new(Arc::new(|_| {}));
        corrigir(&ctx, &fixture.0, &[package.clone()]).unwrap();
        if !case_sensitive {
            assert_eq!(fs::read_to_string(&package).unwrap(), xml);
            assert!(!fixture.0.join("Lib/.acbrinstall-backup").exists());
            return;
        }
        let expected = xml.replace("DACTe", "DACTE").replace("reg.pas", "Reg.pas");
        assert_eq!(fs::read_to_string(&package).unwrap(), expected);
        let backups = fixture.0.join("Lib/.acbrinstall-backup");
        let backup = fs::read_dir(&backups)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            fs::read_to_string(backup.join("Pacotes/report.lpk")).unwrap(),
            xml
        );
        assert_eq!(
            fs::read_to_string(sources.join("Reg.pas")).unwrap(),
            "unit Reg;"
        );
        corrigir(&ctx, &fixture.0, &[package]).unwrap();
        assert_eq!(fs::read_dir(backups).unwrap().count(), 1);
    }

    #[test]
    fn leaves_nonexistent_paths_and_macros_unchanged_and_rejects_ambiguity() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("DACTE")).unwrap();
        if fixture.0.join("dacte").exists() {
            assert_eq!(
                existing_path(&fixture.0, "DACTe").unwrap().unwrap(),
                "DACTe"
            );
            return;
        }
        fs::create_dir(fixture.0.join("dacte")).unwrap();
        assert!(existing_path(&fixture.0, "DACTe").is_err());
        assert!(existing_path(&fixture.0, "DACTE/missing")
            .unwrap()
            .is_none());
        assert!(existing_path(&fixture.0, "$(PkgOutDir)/DACTe")
            .unwrap()
            .is_none());
        assert_eq!(
            existing_path(&fixture.0, "DACTE").unwrap().unwrap(),
            "DACTE"
        );
    }
}
