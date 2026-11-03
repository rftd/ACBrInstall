//! Fontes ACBr e instalação em um ambiente existente.
use crate::{
    config::{AcbrAction, AcbrConfig as Config},
    platform::{primary_config, EXE},
    runtime::{s, which, Ctx},
};
use anyhow::{anyhow, bail, Context, Result};
#[cfg(test)]
use std::env;
#[cfg(test)]
use std::sync::{atomic::Ordering, Arc, Mutex};
use std::{
    fs,
    path::{Path, PathBuf},
};
pub const ACBR_SVN: &str = "https://svn.code.sf.net/p/acbr/code/trunk2/";

// ------------------------------- 2) ACBr (SVN) -----------------------------

fn achar_svn(base: &Path) -> Option<PathBuf> {
    if let Some(p) = which("svn") {
        return Some(p);
    }
    // o fpcup pode ter baixado o svn dentro da pasta de instalação
    fn walk(dir: &Path, alvo: &str, prof: u32) -> Option<PathBuf> {
        if prof >= 4 {
            return None;
        }
        let nome = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if matches!(nome, "ACBr" | "lazarus" | "fpcsrc") {
            return None;
        }
        let mut subs = Vec::new();
        for e in fs::read_dir(dir).ok()?.flatten() {
            let p = e.path();
            if p.is_file() && e.file_name().to_string_lossy() == alvo {
                return Some(p);
            } else if p.is_dir() {
                subs.push(p);
            }
        }
        subs.into_iter().find_map(|d| walk(&d, alvo, prof + 1))
    }
    walk(base, &format!("svn{EXE}"), 0)
}

fn validar_fontes_acbr(destino: &Path, action: AcbrAction) -> Result<()> {
    match action {
        AcbrAction::Install | AcbrAction::UpdateAndInstall => {
            if !destino.join("Pacotes").join("Lazarus").is_dir()
                && !destino.join("Pacotes/Delphi").is_dir()
            {
                bail!("Fontes do ACBr inválidos em {}: selecione a pasta que contém Pacotes/Lazarus ou Pacotes/Delphi.", destino.display());
            }
            if action == AcbrAction::UpdateAndInstall && !destino.join(".svn").is_dir() {
                bail!("{} não é a raiz de uma cópia SVN do ACBr. Use Apenas instalar para fontes locais sem SVN.", destino.display());
            }
        }
        AcbrAction::DownloadAndInstall => {
            if destino.exists() && (!destino.is_dir() || fs::read_dir(destino)?.next().is_some()) {
                bail!("O destino {} já contém arquivos. Escolha uma pasta vazia, Apenas instalar ou Atualizar e instalar.", destino.display());
            }
        }
    }
    Ok(())
}

fn preparar_acbr(ctx: &Ctx, base: &Path, destino: &Path, action: AcbrAction) -> Result<PathBuf> {
    if action == AcbrAction::Install {
        ctx.step("Usando fontes locais do ACBr (sem baixar ou atualizar)");
        return fs::canonicalize(destino).context("resolvendo pasta dos fontes do ACBr");
    }
    let svn = achar_svn(base).ok_or_else(|| {
        anyhow!(
            "Cliente SVN não encontrado. Instale-o (Linux: pacote 'subversion' | \
             Windows: TortoiseSVN com 'command line tools' ou SlikSVN) e tente novamente."
        )
    })?;
    if action == AcbrAction::UpdateAndInstall {
        ctx.step("Atualizando fontes do ACBr (svn update)");
        ctx.run(
            &svn,
            &["update".into(), "--non-interactive".into(), s(destino)],
            None,
            true,
        )?;
    } else {
        ctx.step("Baixando ACBr via SVN (trunk2) - pode demorar");
        ctx.run(
            &svn,
            &[
                "checkout".into(),
                "--non-interactive".into(),
                ACBR_SVN.into(),
                s(destino),
            ],
            None,
            true,
        )?;
    }
    fs::canonicalize(destino).context("resolvendo pasta dos fontes do ACBr")
}

pub fn baixar_fontes_acbr(ctx: &Ctx, base: &Path, destino: &Path) -> Result<()> {
    validar_fontes_acbr(destino, AcbrAction::DownloadAndInstall)?;
    if let Some(parent) = destino.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    preparar_acbr(ctx, base, destino, AcbrAction::DownloadAndInstall)?;
    Ok(())
}

/// (nome do pacote, tipo) lidos do .lpk (XML)
pub(crate) fn info_lpk(p: &Path) -> (String, String) {
    let stem = p
        .file_stem()
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Ok(bytes) = fs::read(p) else {
        return (stem, String::new());
    };
    let txt = String::from_utf8_lossy(&bytes);
    let Ok(doc) = roxmltree::Document::parse(&txt) else {
        return (stem, String::new());
    };
    let Some(pkg) = doc.descendants().find(|n| n.has_tag_name("Package")) else {
        return (stem, String::new());
    };
    let attr = |tag: &str| {
        pkg.children()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.attribute("Value"))
            .map(str::to_string)
    };
    (
        attr("Name").unwrap_or(stem),
        attr("Type").unwrap_or_default(),
    )
}

/// Retira binários antigos do caminho de busca, inclusive unidades movidas entre
/// pacotes. Apenas recompilar não remove PPUs que sobraram no pacote anterior.
fn preparar_recompilacao_acbr(ctx: &Ctx, acbr: &Path) -> Result<()> {
    fn collect(dir: &Path, target: &str, in_target: bool, files: &mut Vec<PathBuf>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                collect(
                    &path,
                    target,
                    in_target || entry.file_name() == target,
                    files,
                )?;
            } else if kind.is_file() && in_target {
                let ext = path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_ascii_lowercase();
                if matches!(ext.as_str(), "ppu" | "o" | "a" | "compiled") {
                    files.push(path);
                }
            }
        }
        Ok(())
    }

    let root = acbr.join("Lib/Lazarus");
    let target = crate::platform::native_target();
    let mut files = Vec::new();
    collect(&root, &target, false, &mut files).context("verificando artefatos antigos do ACBr")?;
    if files.is_empty() {
        return Ok(());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let backup = acbr
        .join("Lib/.acbrinstall-backup")
        .join(format!("{stamp}-{}", std::process::id()));
    ctx.say(format!(
        "  Preparando recompilação de {target}; backup dos artefatos em {}",
        backup.display()
    ));
    for file in &files {
        if ctx.cancelled() {
            bail!("Cancelado pelo usuário.");
        }
        let dest = backup.join(file.strip_prefix(&root)?);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::rename(file, &dest)
            .with_context(|| format!("guardando artefato {}", file.display()))?;
    }
    ctx.say(format!(
        "  {} artefatos antigos guardados; os pacotes serão recompilados.",
        files.len()
    ));
    Ok(())
}

fn instalar_acbr(ctx: &Ctx, cfg: &Config, acbr: &Path) -> Result<()> {
    instalar_acbr_modo(ctx, cfg, acbr, true)
}

fn instalar_acbr_modo(ctx: &Ctx, cfg: &Config, acbr: &Path, install_ide: bool) -> Result<()> {
    let base = &cfg.base;
    ctx.step("Instalando pacotes ACBr no Lazarus");
    let lazbuild = crate::platform::lazbuild(base);
    if !lazbuild.exists() {
        bail!("lazbuild não encontrado em {}", lazbuild.display());
    }
    let pcp = primary_config(base);
    let pcp_arg = format!("--pcp={}", pcp.display());

    let catalog = crate::packages::Catalog::load(acbr)?;
    let plan = catalog.resolve_with(cfg.acbr_packages.as_deref(), cfg.all_components)?;
    if plan.is_empty() {
        bail!("Selecione pelo menos um componente ACBr para instalar.");
    }
    let lpks: Vec<PathBuf> = plan
        .iter()
        .map(|&i| catalog.items[i].path.clone())
        .collect();
    ctx.say(format!(
        "  {} pacotes selecionados, incluindo dependências.",
        lpks.len()
    ));
    for (_, package) in catalog
        .items
        .iter()
        .enumerate()
        .filter(|(i, p)| p.requires_fastreport && !plan.contains(i))
    {
        ctx.say(format!(
            "  Não selecionado: {} (FastReport opcional).",
            package.name
        ));
    }

    crate::package_paths::corrigir(ctx, acbr, &lpks)?;
    let dependencies = crate::opm::preparar_dependencias(
        ctx,
        base,
        &pcp,
        &lazbuild,
        &lpks,
        cfg.fastreport_dir.as_deref(),
        install_ide,
    )?;

    // 1) registra todos os pacotes (links) para resolver dependências
    for l in &lpks {
        ctx.run(
            &lazbuild,
            &[pcp_arg.clone(), "--add-package-link".into(), s(l)],
            None,
            true,
        )
        .with_context(|| format!("registrando pacote {}", l.display()))?;
    }
    preparar_recompilacao_acbr(ctx, acbr)?;
    // 2) compila todos
    for l in &lpks {
        ctx.run(&lazbuild, &[pcp_arg.clone(), s(l)], None, true)
            .with_context(|| format!("compilando pacote {}", l.display()))?;
    }
    if !install_ide {
        ctx.say("Pacotes ACBr compilados e registrados para uso pelo lazbuild.");
        return Ok(());
    }
    // 3) instala na IDE os pacotes de design-time e recompila a IDE
    let instalar: Vec<String> = lpks
        .iter()
        .map(|l| info_lpk(l))
        .filter(|(_, t)| t.contains("DesignTime"))
        .map(|(n, _)| n)
        .collect();
    crate::packages::sync_installed(&pcp, &catalog, &instalar)?;
    if instalar.is_empty() {
        ctx.say("  Somente pacotes runtime selecionados; atualizando a IDE sem componentes ACBr visuais.");
        ctx.run(&lazbuild, &[pcp_arg, "--build-ide=".into()], None, true)?;
        return Ok(());
    }
    // --add-package é uma flag; nomes dos pacotes são argumentos posicionais.
    let mut args = vec![pcp_arg, "--add-package".into(), "--build-ide=".into()];
    args.extend(dependencies.iter().map(|p| s(p)));
    args.extend(instalar);
    ctx.run(&lazbuild, &args, None, true)
        .context("instalando pacotes ACBr e recompilando a IDE Lazarus")?;
    ctx.say(format!("\n  Pasta do ACBr: {}", acbr.display()));
    Ok(())
}

pub fn executar(cfg: &Config, ctx: &Ctx) -> Result<()> {
    executar_modo(cfg, ctx, true)
}

/// Registra e compila pacotes/dependências sem alterar a seleção instalada na IDE.
pub fn preparar_compilacao(cfg: &Config, ctx: &Ctx) -> Result<()> {
    executar_modo(cfg, ctx, false)
}

fn executar_modo(cfg: &Config, ctx: &Ctx, install_ide: bool) -> Result<()> {
    if !install_ide && cfg.ide != crate::config::Ide::Lazarus {
        bail!("--build-only está disponível somente para Lazarus.");
    }
    if cfg.ide == crate::config::Ide::Delphi && !cfg!(windows) {
        bail!("A instalação para Delphi está disponível somente no Windows.");
    }
    match crate::config_store::save_acbr(cfg) {
        Ok(path) => ctx.say(format!("Configuração salva em: {}", path.display())),
        Err(error) => ctx.say(format!(
            "AVISO: não foi possível salvar a configuração: {error:#}"
        )),
    }
    if cfg.acbr_packages.as_ref().is_some_and(Vec::is_empty) {
        bail!("Selecione pelo menos um componente ACBr.");
    }
    let destination = cfg
        .acbr_dir
        .clone()
        .unwrap_or_else(|| cfg.base.join("ACBr"));
    validar_fontes_acbr(&destination, cfg.acbr_action)?;
    if cfg.ide == crate::config::Ide::Lazarus && !crate::platform::lazbuild(&cfg.base).is_file() {
        bail!("Lazarus não encontrado em {}. Prepare o ambiente com PascalInstall ou selecione uma instalação existente.", cfg.base.display());
    }
    if cfg.ide == crate::config::Ide::Delphi {
        crate::delphi::validate(&cfg.delphi)?;
    }
    ctx.say(format!("Fontes do ACBr: {}", destination.display()));
    let acbr = preparar_acbr(ctx, &cfg.base, &destination, cfg.acbr_action)?;
    match cfg.ide {
        crate::config::Ide::Lazarus => {
            if install_ide {
                instalar_acbr(ctx, cfg, &acbr)?;
            } else {
                instalar_acbr_modo(ctx, cfg, &acbr, false)?;
            }
        }
        crate::config::Ide::Delphi => crate::delphi::install(ctx, cfg, &acbr)?,
    }
    ctx.step("Concluído");
    if install_ide {
        ctx.say("Componentes ACBr instalados. Reabra sua IDE.");
    } else {
        ctx.say("Ambiente ACBr pronto para compilação.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sources(PathBuf);

    impl Sources {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = env::temp_dir().join(format!(
                "acbrinstall-test-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Sources {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn acbr_actions_validate_sources_without_overwriting_existing_files() {
        let sources = Sources::new();
        let path = &sources.0;
        assert!(validar_fontes_acbr(&path.join("new"), AcbrAction::DownloadAndInstall).is_ok());
        assert!(validar_fontes_acbr(path, AcbrAction::DownloadAndInstall).is_ok());
        assert!(validar_fontes_acbr(path, AcbrAction::Install).is_err());
        assert!(validar_fontes_acbr(path, AcbrAction::UpdateAndInstall).is_err());

        fs::create_dir_all(path.join("Pacotes/Lazarus")).unwrap();
        let package = path.join("Pacotes/Lazarus/local.lpk");
        fs::write(&package, "local sources").unwrap();
        assert!(validar_fontes_acbr(path, AcbrAction::Install).is_ok());
        assert!(validar_fontes_acbr(path, AcbrAction::DownloadAndInstall).is_err());
        assert!(validar_fontes_acbr(path, AcbrAction::UpdateAndInstall).is_err());

        fs::create_dir(path.join(".svn")).unwrap();
        assert!(validar_fontes_acbr(path, AcbrAction::UpdateAndInstall).is_ok());
        assert_eq!(fs::read_to_string(package).unwrap(), "local sources");
    }

    #[test]
    fn local_sources_are_used_without_svn_or_downloading() {
        let sources = Sources::new();
        fs::create_dir_all(sources.0.join("Pacotes/Lazarus")).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let captured = log.clone();
        let ctx = Ctx::new(Arc::new(move |line| captured.lock().unwrap().push(line)));
        let path = preparar_acbr(
            &ctx,
            &sources.0.join("no-tools"),
            &sources.0,
            AcbrAction::Install,
        )
        .unwrap();
        assert_eq!(path, fs::canonicalize(&sources.0).unwrap());
        assert!(!log.lock().unwrap().iter().any(|line| line.contains("  $ ")));
        assert!(!sources.0.join(".svn").exists());
    }

    #[test]
    fn rebuild_backs_up_stale_units_without_touching_sources_or_other_targets() {
        let fixture = Sources::new();
        let target = crate::platform::native_target();
        let output = fixture.0.join("Lib/Lazarus/ACBrDFe").join(&target);
        fs::create_dir_all(&output).unwrap();
        for name in [
            "ACBrXmlDocument.ppu",
            "ACBrXmlDocument.o",
            "acbrdfecomum.compiled",
        ] {
            fs::write(output.join(name), name).unwrap();
        }
        fs::write(output.join("keep.pas"), "source").unwrap();
        let other = fixture.0.join("Lib/Lazarus/ACBrDFe/i386-win32");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("ACBrXmlDocument.ppu"), "other platform").unwrap();
        let ctx = Ctx::new(Arc::new(|_| {}));
        preparar_recompilacao_acbr(&ctx, &fixture.0).unwrap();
        assert!(!output.join("ACBrXmlDocument.ppu").exists());
        assert!(!output.join("ACBrXmlDocument.o").exists());
        assert!(!output.join("acbrdfecomum.compiled").exists());
        assert_eq!(
            fs::read_to_string(output.join("keep.pas")).unwrap(),
            "source"
        );
        assert_eq!(
            fs::read_to_string(other.join("ACBrXmlDocument.ppu")).unwrap(),
            "other platform"
        );
        let backups = fixture.0.join("Lib/.acbrinstall-backup");
        let backup = fs::read_dir(&backups)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let saved = backup.join("ACBrDFe").join(&target);
        for name in [
            "ACBrXmlDocument.ppu",
            "ACBrXmlDocument.o",
            "acbrdfecomum.compiled",
        ] {
            assert_eq!(fs::read_to_string(saved.join(name)).unwrap(), name);
        }
        preparar_recompilacao_acbr(&ctx, &fixture.0).unwrap();
        assert_eq!(fs::read_dir(backups).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn build_only_does_not_rebuild_or_change_installed_ide_packages() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Sources::new();
        let base = fixture.0.join("environment");
        let sources = fixture.0.join("sources");
        let packages = sources.join("Pacotes/Lazarus");
        let pcp = base.join("config_lazarus");
        fs::create_dir_all(&packages).unwrap();
        fs::create_dir_all(base.join("lazarus")).unwrap();
        fs::create_dir_all(&pcp).unwrap();
        fs::write(
            packages.join("Test.lpk"),
            r#"<CONFIG><Package><Name Value="Test"/><Type Value="DesignTime"/></Package></CONFIG>"#,
        )
        .unwrap();
        let installed = pcp.join("staticpackages.inc");
        fs::write(&installed, "existing packages").unwrap();
        let tool = base.join("lazarus/lazbuild");
        fs::write(
            &tool,
            r#"#!/bin/sh
for arg in "$@"; do
  case "$arg" in --build-ide*|--add-package) exit 77;; esac
done
exit 0
"#,
        )
        .unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        let cfg = Config {
            base,
            ..Config::default()
        };
        let ctx = Ctx::new(Arc::new(|_| {}));
        instalar_acbr_modo(&ctx, &cfg, &sources, false).unwrap();
        assert_eq!(fs::read_to_string(installed).unwrap(), "existing packages");
        assert!(!pcp.join("miscellaneousoptions.xml").exists());
    }

    #[cfg(unix)]
    #[test]
    fn reuses_registered_fortes_and_stops_before_acbr_if_it_fails() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Sources::new();
        let base = fixture.0.join("environment");
        let pcp = base.join("config_lazarus");
        let acbr = fixture.0.join("ACBr");
        fs::create_dir_all(acbr.join("Pacotes/Lazarus")).unwrap();
        fs::create_dir_all(base.join("lazarus")).unwrap();
        fs::create_dir_all(&pcp).unwrap();
        let fortes = fixture.0.join("frce.lpk");
        fs::write(
            &fortes,
            r#"<CONFIG><Package><Name Value="frce"/></Package></CONFIG>"#,
        )
        .unwrap();
        fs::write(pcp.join("packagefiles.xml"), format!(r#"<CONFIG><UserPkgLinks><Item1><Name Value="frce"/><Filename Value="{}"/></Item1></UserPkgLinks></CONFIG>"#, fortes.display())).unwrap();
        fs::write(acbr.join("Pacotes/Lazarus/Report.lpk"), r#"<CONFIG><Package><Name Value="Report"/><RequiredPkgs><Item1><PackageName Value="frce"/></Item1></RequiredPkgs></Package></CONFIG>"#).unwrap();
        let lazbuild = base.join("lazarus/lazbuild");
        fs::write(
            &lazbuild,
            "#!/bin/sh\n[ \"$2\" = --add-package-link ] && exit 0\nexit 7\n",
        )
        .unwrap();
        fs::set_permissions(&lazbuild, fs::Permissions::from_mode(0o755)).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let captured = log.clone();
        let ctx = Ctx::new(Arc::new(move |line| captured.lock().unwrap().push(line)));
        let cfg = Config {
            base,
            ..Config::default()
        };
        let error = instalar_acbr(&ctx, &cfg, &acbr).unwrap_err();
        assert!(format!("{error:#}").contains("compilando FortesReport-CE antes do ACBr"));
        let log = log.lock().unwrap().join("\n");
        assert!(log.contains("Reutilizando FortesReport-CE"));
        assert!(!log.contains("Baixando"));
        assert!(!log.contains("Report.lpk"));
        assert!(!log.contains("--build-ide"));
    }

    #[cfg(unix)]
    #[test]
    fn acbr_uses_positional_packages_and_propagates_lazbuild_failures() {
        use std::os::unix::fs::PermissionsExt;

        for (failure, expected_context, expected_calls) in [
            ("link", "registrando pacote", 1),
            ("compile", "compilando pacote", 4),
            ("ide", "recompilando a IDE", 7),
            ("none", "", 7),
        ] {
            let fixture = Sources::new();
            let base = fixture.0.join("environment with spaces");
            let sources = fixture.0.join("ACBr sources");
            let packages = sources.join("Pacotes/Lazarus");
            fs::create_dir_all(&packages).unwrap();
            for (name, kind) in [
                ("One", "DesignTime"),
                ("Two", "RunAndDesignTime"),
                ("Runtime", "RunTime"),
            ] {
                fs::write(
                    packages.join(format!("{name}.lpk")),
                    format!(r#"<CONFIG><Package><Name Value="{name}"/><Type Value="{kind}"/></Package></CONFIG>"#),
                ).unwrap();
            }
            fs::create_dir_all(base.join("lazarus")).unwrap();
            fs::create_dir_all(base.join("config_lazarus")).unwrap();
            let lazbuild = base.join("lazarus/lazbuild");
            fs::write(
                &lazbuild,
                r#"#!/bin/sh
case "$2" in
    --add-package-link) stage=link ;;
    --add-package)
        stage=ide
        test "$#" -eq 5 && test "$3" = '--build-ide=' &&
            test "$4" = One && test "$5" = Two || exit 91
        ;;
    --*) exit 92 ;;
    *) stage=compile ;;
esac
printf '%s\n' "$stage" >> "${0%/*}/calls"
test "$stage" != "$(cat "${0%/*}/failure")" || exit 23
"#,
            )
            .unwrap();
            fs::set_permissions(&lazbuild, fs::Permissions::from_mode(0o755)).unwrap();
            fs::write(base.join("lazarus/failure"), failure).unwrap();
            let log = Arc::new(Mutex::new(Vec::new()));
            let captured = log.clone();
            let ctx = Ctx::new(Arc::new(move |line| captured.lock().unwrap().push(line)));
            let cfg = Config {
                base: base.clone(),

                acbr_dir: Some(sources),
                acbr_action: AcbrAction::Install,
                ..Config::default()
            };
            let result = instalar_acbr(&ctx, &cfg, cfg.acbr_dir.as_ref().unwrap());
            if failure == "none" {
                result.unwrap();
                assert!(log
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|l| l.contains("Pasta do ACBr:")));
            } else {
                let error = format!("{:#}", result.unwrap_err());
                assert!(error.contains(expected_context), "{error}");
                assert!(error.contains("código 23"), "{error}");
                assert!(!log
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|l| l.contains("Pasta do ACBr:")));
            }
            assert_eq!(
                fs::read_to_string(base.join("lazarus/calls"))
                    .unwrap()
                    .lines()
                    .count(),
                expected_calls
            );
        }
    }
}
