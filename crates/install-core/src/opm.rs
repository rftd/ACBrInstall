//! Dependências do ACBr distribuídas pelo Online Package Manager do Lazarus.
use crate::{
    acbr::info_lpk,
    runtime::{http_bytes, http_file, Ctx},
};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

const REPOSITORY: &str = "https://packages.lazarus-ide.org/";

fn relative_path(raw: &str) -> Result<PathBuf> {
    let normalized = raw.replace('\\', "/");
    let path = PathBuf::from(&normalized);
    if path.as_os_str().is_empty()
        || normalized.contains(':')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        bail!("Caminho inválido no catálogo OPM: {raw}");
    }
    Ok(path)
}

struct Download {
    archive: String,
    package: PathBuf,
    size: u64,
}

fn find_download(catalog: &Value, package: &str) -> Result<Download> {
    let entries = catalog.as_object().context("catálogo OPM inválido")?;
    for (key, value) in entries {
        let Some(index) = key.strip_prefix("PackageFiles") else {
            continue;
        };
        for file in value.as_array().into_iter().flatten() {
            if !file["Name"]
                .as_str()
                .unwrap_or_default()
                .eq_ignore_ascii_case(package)
            {
                continue;
            }
            let data = entries
                .get(&format!("PackageData{index}"))
                .context("metadados OPM ausentes")?;
            let archive = data["RepositoryFileName"]
                .as_str()
                .context("arquivo ZIP ausente no OPM")?;
            // O download deve permanecer no repositório oficial.
            if archive.contains(['/', '\\', ':'])
                || !archive.ends_with(".zip")
                || archive.starts_with('.')
            {
                bail!("Nome de arquivo OPM inválido: {archive}");
            }
            let base = relative_path(data["PackageBaseDir"].as_str().unwrap_or_default())?;
            let relative = file["RelativeFilePath"].as_str().unwrap_or_default();
            let mut path = base;
            if !relative.is_empty() {
                path.push(relative_path(relative)?);
            }
            path.push(relative_path(package)?);
            return Ok(Download {
                archive: archive.into(),
                package: path,
                size: data["RepositoryFileSize"]
                    .as_u64()
                    .context("tamanho do ZIP ausente no OPM")?,
            });
        }
    }
    bail!("Pacote {package} não encontrado no catálogo oficial OPM")
}

fn registered_package(pcp: &Path, name: &str) -> Option<PathBuf> {
    let xml = fs::read_to_string(pcp.join("packagefiles.xml")).ok()?;
    let doc = roxmltree::Document::parse(&xml).ok()?;
    doc.descendants().find_map(|node| {
        let value = |tag| {
            node.children()
                .find(|n| n.has_tag_name(tag))
                .and_then(|n| n.attribute("Value"))
        };
        if !value("Name")?.eq_ignore_ascii_case(name) {
            return None;
        }
        let path = PathBuf::from(value("Filename")?);
        (path.is_file() && info_lpk(&path).0.eq_ignore_ascii_case(name)).then_some(path)
    })
}

fn needs_package(packages: &[PathBuf], name: &str) -> Result<bool> {
    for path in packages {
        let bytes = fs::read(path)?;
        let text = String::from_utf8_lossy(&bytes);
        let doc = roxmltree::Document::parse(&text)
            .with_context(|| format!("lendo {}", path.display()))?;
        if doc.descendants().any(|n| {
            n.has_tag_name("PackageName")
                && n.attribute("Value")
                    .is_some_and(|v| v.eq_ignore_ascii_case(name))
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn preparar_dependencias(
    ctx: &Ctx,
    base: &Path,
    pcp: &Path,
    lazbuild: &Path,
    packages: &[PathBuf],
    fastreport_dir: Option<&Path>,
) -> Result<Vec<PathBuf>> {
    // O FastReport só usa fontes/links existentes: não passa pelo catálogo OPM.
    let mut prepared = preparar_fastreport(ctx, pcp, lazbuild, packages, fastreport_dir)?;
    for (trigger, name, label) in [
        ("frce", "frce", "FortesReport-CE"),
        ("lazreportpdfexport", "pack_powerpdf", "PowerPDF"),
    ] {
        if needs_package(packages, trigger)? || (trigger != name && needs_package(packages, name)?)
        {
            prepared.push(preparar_pacote(ctx, base, pcp, lazbuild, name, label)?);
        }
    }
    if !prepared.is_empty() {
        let mut args = vec![
            format!("--pcp={}", pcp.display()),
            "--add-package".into(),
            "--build-ide=".into(),
        ];
        args.extend(prepared.iter().map(|p| p.to_string_lossy().into_owned()));
        ctx.run(lazbuild, &args, None, true)
            .context("instalando dependências OPM na IDE antes do ACBr")?;
    }
    Ok(prepared)
}

fn required_packages(path: &Path) -> Result<Vec<String>> {
    let bytes = fs::read(path)?;
    let xml = String::from_utf8_lossy(&bytes);
    let doc =
        roxmltree::Document::parse(&xml).with_context(|| format!("lendo {}", path.display()))?;
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("PackageName"))
        .filter_map(|n| n.attribute("Value").map(str::to_string))
        .collect())
}

fn fastreport_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("frx") || name == "fr_lazarus" || name.contains("fastreport")
}

fn preparar_fastreport(
    ctx: &Ctx,
    pcp: &Path,
    lazbuild: &Path,
    packages: &[PathBuf],
    directory: Option<&Path>,
) -> Result<Vec<PathBuf>> {
    let mut roots = HashSet::new();
    for package in packages {
        roots.extend(
            required_packages(package)?
                .into_iter()
                .filter(|n| fastreport_name(n)),
        );
    }
    if roots.is_empty() {
        return Ok(Vec::new());
    }
    let mut available = HashMap::new();
    fn scan(dir: &Path, available: &mut HashMap<String, PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                scan(&path, available)?;
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("lpk"))
            {
                let name = info_lpk(&path).0.to_ascii_lowercase();
                if available.insert(name.clone(), path).is_some() {
                    bail!("Mais de um pacote FastReport chamado {name}. Selecione a pasta de uma única versão.");
                }
            }
        }
        Ok(())
    }
    if let Some(dir) = directory {
        if !dir.is_dir() {
            bail!("Pasta FastReport não encontrada: {}", dir.display());
        }
        scan(dir, &mut available)?;
    }
    fn visit(
        name: &str,
        pcp: &Path,
        available: &HashMap<String, PathBuf>,
        seen: &mut HashSet<String>,
        active: &mut HashSet<String>,
        plan: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let key = name.to_ascii_lowercase();
        if seen.contains(&key) {
            return Ok(());
        }
        if !active.insert(key.clone()) {
            bail!("Dependência circular no FastReport: {name}");
        }
        let path=available.get(&key).cloned().or_else(||registered_package(pcp,name))
            .with_context(||format!("FastReport selecionado, mas {name}.lpk não está registrado. Instale sua cópia adquirida no Lazarus ou informe --fastreport-dir. O instalador não baixa o produto pago."))?;
        for dependency in required_packages(&path)? {
            if available.contains_key(&dependency.to_ascii_lowercase())
                || fastreport_name(&dependency)
            {
                visit(&dependency, pcp, available, seen, active, plan)?;
            }
        }
        active.remove(&key);
        seen.insert(key);
        plan.push(path);
        Ok(())
    }
    let mut plan = Vec::new();
    let mut seen = HashSet::new();
    let mut roots: Vec<String> = roots.into_iter().collect();
    roots.sort();
    for name in roots {
        visit(
            &name,
            pcp,
            &available,
            &mut seen,
            &mut HashSet::new(),
            &mut plan,
        )?;
    }
    ctx.step("Preparando FastReport fornecido pelo usuário");
    let pcp_arg = format!("--pcp={}", pcp.display());
    for path in &plan {
        ctx.run(
            lazbuild,
            &[
                pcp_arg.clone(),
                "--add-package-link".into(),
                path.to_string_lossy().into_owned(),
            ],
            None,
            true,
        )?;
    }
    for path in &plan {
        ctx.run(
            lazbuild,
            &[pcp_arg.clone(), path.to_string_lossy().into_owned()],
            None,
            true,
        )
        .context("compilando FastReport fornecido pelo usuário")?;
    }
    Ok(plan
        .into_iter()
        .filter(|p| info_lpk(p).1.contains("DesignTime"))
        .collect())
}

fn preparar_pacote(
    ctx: &Ctx,
    base: &Path,
    pcp: &Path,
    lazbuild: &Path,
    name: &str,
    label: &str,
) -> Result<PathBuf> {
    ctx.step(&format!("Preparando {label} (dependência do ACBr)"));
    let cache = base.join("packages/opm").join(label);
    let package = if let Some(path) = registered_package(pcp, name) {
        ctx.say(format!("  Reutilizando {label}: {}", path.display()));
        path
    } else if cache.join(".package-path").is_file() {
        cache.join(relative_path(
            fs::read_to_string(cache.join(".package-path"))?.trim(),
        )?)
    } else {
        let catalog: Value =
            serde_json::from_slice(&http_bytes(&format!("{REPOSITORY}packagelist.json"))?)?;
        let download = find_download(&catalog, &format!("{name}.lpk"))?;
        let parent = cache.parent().unwrap();
        fs::create_dir_all(parent)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let staging = parent.join(format!(".{name}-{stamp}"));
        fs::create_dir(&staging)?;
        let archive = staging.with_extension("zip");
        let url = format!("{REPOSITORY}{}", download.archive);
        ctx.say(format!("  Baixando {url}"));
        http_file(&url, &archive)?;
        if fs::metadata(&archive)?.len() != download.size {
            bail!("Download incompleto de {label}: {}", archive.display());
        }
        if cfg!(windows) {
            let quote = |p: &Path| p.to_string_lossy().replace('\'', "''");
            ctx.run("powershell.exe", &[
                "-NoProfile".into(), "-NonInteractive".into(), "-Command".into(),
                format!("$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath '{}' -DestinationPath '{}'", quote(&archive), quote(&staging)),
            ], None, true)?;
        } else {
            ctx.run(
                "unzip",
                &[
                    "-q".into(),
                    archive.to_string_lossy().into_owned(),
                    "-d".into(),
                    staging.to_string_lossy().into_owned(),
                ],
                None,
                true,
            )?;
        }
        let path = staging.join(&download.package);
        if !info_lpk(&path).0.eq_ignore_ascii_case(name) || !path.is_file() {
            bail!("O ZIP do OPM não contém o pacote {name} esperado");
        }
        fs::write(
            staging.join(".package-path"),
            download.package.to_string_lossy().as_bytes(),
        )?;
        fs::rename(&staging, &cache).with_context(|| format!("guardando fontes de {label}"))?;
        let _ = fs::remove_file(archive);
        cache.join(download.package)
    };
    if !package.is_file() || !info_lpk(&package).0.eq_ignore_ascii_case(name) {
        return Err(anyhow!("Fontes {label} inválidos em {}", package.display()));
    }
    let path = package.to_string_lossy().into_owned();
    let pcp_arg = format!("--pcp={}", pcp.display());
    ctx.run(
        lazbuild,
        &[pcp_arg.clone(), "--add-package-link".into(), path.clone()],
        None,
        true,
    )
    .with_context(|| format!("registrando {label}"))?;
    ctx.run(lazbuild, &[pcp_arg, path], None, true)
        .with_context(|| format!("compilando {label} antes do ACBr"))?;
    Ok(package)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn fastreport_uses_only_user_sources_or_registered_links_and_checks_before_building() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::{Arc, Mutex};
        let root = std::env::temp_dir().join(format!(
            "fastreport-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let sources = root.join("purchased sources");
        let pcp = root.join("config");
        fs::create_dir_all(&sources).unwrap();
        fs::create_dir_all(&pcp).unwrap();
        // Linux and macOS do not place `true`/`false` in the same directory.
        // Own fixtures also prove the failure comes from lazbuild's exit code,
        // rather than from a missing executable on the runner.
        let executable = |name: &str, exit_code: u8| {
            let path = root.join(name);
            fs::write(&path, format!("#!/bin/sh\nexit {exit_code}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let success = executable("lazbuild-success", 0);
        let failure = executable("lazbuild-failure", 23);
        let acbr = root.join("ACBrReport.lpk");
        fs::write(&acbr, r#"<CONFIG><Package><Name Value="ACBrReport"/><RequiredPkgs><Item1><PackageName Value="fr_lazarus"/></Item1></RequiredPkgs></Package></CONFIG>"#).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let captured = log.clone();
        let ctx = Ctx::new(Arc::new(move |line| captured.lock().unwrap().push(line)));
        let error = preparar_fastreport(&ctx, &pcp, &success, &[acbr.clone()], None).unwrap_err();
        assert!(format!("{error:#}").contains("não está registrado"));
        assert!(log.lock().unwrap().is_empty());
        // A saved path is harmless if no selected component requires FastReport.
        assert!(
            preparar_fastreport(&ctx, &pcp, &failure, &[], Some(Path::new("missing")))
                .unwrap()
                .is_empty()
        );
        let runtime = sources.join("frxBase.lpk");
        let design = sources.join("fr_lazarus.lpk");
        fs::write(
            &runtime,
            r#"<CONFIG><Package><Name Value="frxBase"/><Type Value="RunTime"/></Package></CONFIG>"#,
        )
        .unwrap();
        fs::write(&design,r#"<CONFIG><Package><Name Value="fr_lazarus"/><Type Value="RunAndDesignTime"/><RequiredPkgs><Item1><PackageName Value="frxBase"/></Item1></RequiredPkgs></Package></CONFIG>"#).unwrap();
        let result =
            preparar_fastreport(&ctx, &pcp, &success, &[acbr.clone()], Some(&sources)).unwrap();
        assert_eq!(result, vec![design.clone()]);
        let calls: Vec<String> = log
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.starts_with("  $ "))
            .cloned()
            .collect();
        assert_eq!(calls.len(), 4);
        assert!(calls[0].ends_with(&runtime.to_string_lossy().to_string()));
        assert!(calls[1].ends_with(&design.to_string_lossy().to_string()));
        assert!(calls[2].ends_with(&runtime.to_string_lossy().to_string()));
        assert!(calls[3].ends_with(&design.to_string_lossy().to_string()));
        assert!(calls[..2].iter().all(|l| l.contains("--add-package-link")));
        assert!(calls[2..].iter().all(|l| !l.contains("--add-package")));
        fs::write(pcp.join("packagefiles.xml"),format!(r#"<CONFIG><UserPkgLinks><Item1><Name Value="frxBase"/><Filename Value="{}"/></Item1><Item2><Name Value="fr_lazarus"/><Filename Value="{}"/></Item2></UserPkgLinks></CONFIG>"#,runtime.display(),design.display())).unwrap();
        assert_eq!(
            preparar_fastreport(&ctx, &pcp, &success, &[acbr.clone()], None).unwrap(),
            vec![design]
        );
        let error = preparar_fastreport(&ctx, &pcp, &failure, &[acbr], None).unwrap_err();
        assert!(format!("{error:#}").contains("código 23"));
        assert!(!log.lock().unwrap().join("\n").contains("Baixando"));
    }

    #[test]
    fn selects_frce_instead_of_incompatible_fortes4lazarus() {
        let catalog = serde_json::json!({
            "PackageData1": {"RepositoryFileName":"Fortes4Lazarus.zip"},
            "PackageFiles1": [{"Name":"fortes324forlaz.lpk"}],
            "PackageData2": {"RepositoryFileName":"FortesReport-CE.zip", "PackageBaseDir":"fortesreport-ce4\\/", "RepositoryFileSize":123},
            "PackageFiles2": [{"Name":"frce.lpk", "RelativeFilePath":"Packages\\/"}]
        });
        let result = find_download(&catalog, "frce.lpk").unwrap();
        assert_eq!(result.archive, "FortesReport-CE.zip");
        assert_eq!(
            result.package,
            PathBuf::from("fortesreport-ce4/Packages/frce.lpk")
        );
        assert_eq!(result.size, 123);
        assert!(find_download(&catalog, "missing.lpk").is_err());
    }

    #[test]
    fn rejects_paths_outside_package_directory() {
        for path in [
            "../file",
            "C:\\file",
            "/file",
            "x/../../file",
            "\\\\server\\file",
        ] {
            assert!(relative_path(path).is_err(), "{path}");
        }
        assert_eq!(
            relative_path("Packages\\/frce.lpk").unwrap(),
            PathBuf::from("Packages/frce.lpk")
        );
    }
}
