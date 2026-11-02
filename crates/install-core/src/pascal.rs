//! Instalação de FPC, Lazarus, cross-compilers e pacotes da IDE.
use crate::{
    config::PascalConfig,
    platform::primary_config,
    platform::{CROSS, EXE},
    runtime::{http_bytes, http_file, s, which, Ctx},
    versions::Component,
};

use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};
const FPCUP_RELEASES_API: &str =
    "https://api.github.com/repos/LongDirtyAnimAlf/Reiniero-fpcup/releases";
const FPCUP_FALLBACK_BASE: &str =
    "https://github.com/LongDirtyAnimAlf/Reiniero-fpcup/releases/download/v2.4.0i/";

// ------------------- 0) dependências do sistema (Linux) --------------------

// Bibliotecas de DESENVOLVIMENTO exigidas para linkar a IDE (widgetset GTK2).
// Exige o nome exato (.so sem versão), que só existe no pacote -dev/-devel.
const LIBS_DEV: [(&str, &str); 8] = [
    ("libgtk-x11-2.0.so", "GTK2"),
    ("libgdk-x11-2.0.so", "GDK (GTK2)"),
    ("libgdk_pixbuf-2.0.so", "gdk-pixbuf"),
    ("libX11.so", "X11"),
    ("libXtst.so", "XTest"),
    ("libcairo.so", "cairo"),
    ("libpango-1.0.so", "pango"),
    ("libatk-1.0.so", "atk"),
];
const LIB_DIRS: [&str; 9] = [
    "/usr/lib",
    "/usr/lib64",
    "/lib",
    "/lib64",
    "/usr/lib/x86_64-linux-gnu",
    "/lib/x86_64-linux-gnu",
    "/usr/local/lib",
    "/usr/lib/aarch64-linux-gnu",
    "/lib/aarch64-linux-gnu",
];

fn verificar_dependencias_linux(ctx: &Ctx) -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Ok(());
    }
    ctx.step("Verificando dependências do sistema");
    let mut faltando: Vec<String> = Vec::new();
    for t in ["make", "ld", "unzip", "svn"] {
        if which(t).is_none() {
            faltando.push(format!("ferramenta '{t}'"));
        }
    }
    let mut gtk_ausente = false;
    for (lib, nome) in LIBS_DEV {
        if !LIB_DIRS.iter().any(|d| Path::new(d).join(lib).exists()) {
            if lib.contains("gtk") || lib.contains("gdk-x11") {
                gtk_ausente = true;
            }
            faltando.push(format!("biblioteca de desenvolvimento: {nome} ({lib})"));
        }
    }
    if faltando.is_empty() {
        ctx.say("  tudo certo");
        return Ok(());
    }

    let gerente = [
        ("apt-get", "apt"),
        ("dnf", "dnf"),
        ("pacman", "pacman"),
        ("zypper", "zypper"),
    ]
    .iter()
    .find(|(bin, _)| which(bin).is_some())
    .map(|(_, g)| *g);

    let mut msg = String::from("Dependências ausentes:\n");
    for f in &faltando {
        msg.push_str(&format!("  - {f}\n"));
    }
    match gerente {
        Some("apt") => msg.push_str("\nInstale com:\n  sudo apt install build-essential subversion unzip libgtk2.0-dev libx11-dev libgdk-pixbuf2.0-dev libcairo2-dev libpango1.0-dev libxtst-dev libatk1.0-dev libglib2.0-dev\n"),
        Some("dnf") => msg.push_str("\nInstale com:\n  sudo dnf install gcc make binutils subversion unzip gtk2-devel libX11-devel gdk-pixbuf2-devel cairo-devel pango-devel libXtst-devel atk-devel glib2-devel\n"),
        Some("pacman") => {
            msg.push_str("\nInstale com:\n  sudo pacman -S --needed base-devel subversion unzip libx11 libxtst gdb\n");
            if gtk_ausente {
                msg.push_str("\nO gtk2 saiu dos repositórios oficiais do Arch/Manjaro; instale pelo AUR:\n  pamac build gtk2      (Manjaro)   ou   yay -S gtk2\n");
            }
        }
        Some("zypper") => msg.push_str("\nInstale com:\n  sudo zypper install gcc make binutils subversion unzip gtk2-devel libX11-devel libXtst-devel\n"),
        _ => msg.push_str("\nInstale os pacotes equivalentes (compilador C/make/binutils, subversion, unzip, gtk2-devel, libX11-devel, libXtst-devel).\n"),
    }
    msg.push_str("\nDepois rode o instalador novamente.");
    bail!(msg)
}

// --------------------------- 1) fpclazup (CLI) -----------------------------

fn baixar_fpcup(ctx: &Ctx, dir: &Path) -> Result<PathBuf> {
    ctx.step("Baixando fpclazup (CLI do fpcup) - repositório Reiniero-fpcup");
    fs::create_dir_all(dir)?;
    let alvo = crate::platform::fpcup_asset(std::env::consts::OS, std::env::consts::ARCH)?;

    let mut url: Option<String> = None;
    match http_bytes(FPCUP_RELEASES_API)
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).map_err(Into::into))
    {
        Ok(v) => {
            'fora: for rel in v.as_array().into_iter().flatten() {
                for a in rel["assets"].as_array().into_iter().flatten() {
                    let nome = a["name"].as_str().unwrap_or("");
                    if nome.eq_ignore_ascii_case(&alvo) {
                        url = a["browser_download_url"].as_str().map(String::from);
                        ctx.say(format!(
                            "  release: {}",
                            rel["tag_name"].as_str().unwrap_or("?")
                        ));
                        break 'fora;
                    }
                }
            }
        }
        Err(e) => ctx.say(format!(
            "  aviso: não consegui consultar a API do GitHub ({e:#})"
        )),
    }
    let url = url.unwrap_or_else(|| {
        let u = format!("{FPCUP_FALLBACK_BASE}{alvo}");
        ctx.say(format!("  usando URL de fallback: {u}"));
        u
    });

    let dest = dir.join(format!("fpclazup{EXE}"));
    http_file(&url, &dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = fs::metadata(&dest)?.permissions();
        p.set_mode(0o755);
        fs::set_permissions(&dest, p)?;
    }
    ctx.say(format!("  baixado: {alvo}"));
    Ok(dest)
}

fn instalar_fpc_lazarus(ctx: &Ctx, fpcup: &Path, cfg: &PascalConfig) -> Result<()> {
    ctx.step("Instalando FPC + Lazarus (pode demorar bastante)");
    let mut args = vec![
        format!("--installdir={}", cfg.base.display()),
        format!("--fpcURL={}", Component::Fpc.source(&cfg.fpc)),
        format!("--lazURL={}", Component::Lazarus.source(&cfg.laz)),
        "--noconfirm".into(),
        "--verbose".into(),
    ];
    if cfg!(target_os = "macos") {
        args.push("--lclplatform=cocoa".into());
    }
    ctx.run(fpcup, &args, None, true)?;
    Ok(())
}

fn instalar_cross(ctx: &Ctx, fpcup: &Path, cfg: &PascalConfig) -> Result<()> {
    for nome in &cfg.cross {
        let Some((_, cpu, os)) = CROSS.iter().find(|(id, _, _)| *id == nome.as_str()) else {
            ctx.say(format!("aviso: alvo cross desconhecido: {nome}"));
            continue;
        };
        ctx.step(&format!("Instalando cross-compiler: {nome} ({cpu}-{os})"));
        // o fpcup tenta baixar binutils/libs do alvo (confira: fpclazup --help)
        let args = vec![
            format!("--installdir={}", cfg.base.display()),
            format!("--fpcURL={}", Component::Fpc.source(&cfg.fpc)),
            format!("--lazURL={}", Component::Lazarus.source(&cfg.laz)),
            format!("--cputarget={cpu}"),
            format!("--ostarget={os}"),
            "--noconfirm".into(),
            "--verbose".into(),
        ];
        let rc = ctx.run(fpcup, &args, None, false)?;
        if rc != 0 {
            ctx.say(format!(
                "AVISO: cross {nome} terminou com código {rc} (veja o log acima)"
            ));
        }
    }
    Ok(())
}

pub fn configurar_ide(ctx: &Ctx, base: &Path) -> Result<()> {
    ctx.step("Configurando Online Package Manager e designer integrado");
    let lazarus = crate::platform::lazarus_dir(base);
    let lazbuild = lazarus.join(format!("lazbuild{EXE}"));
    let pcp = format!("--pcp={}", primary_config(base).display());
    let packages = [
        "components/anchordocking/anchordocking.lpk",
        "components/anchordocking/design/anchordockingdsgn.lpk",
        "components/dockedformeditor/dockedformeditor.lpk",
        "components/onlinepackagemanager/onlinepackagemanager.lpk",
    ]
    .map(|p| lazarus.join(p));
    for path in &packages {
        if !path.is_file() {
            bail!("Pacote da IDE não encontrado: {}", path.display());
        }
    }
    for path in &packages {
        ctx.run(
            &lazbuild,
            &[pcp.clone(), "--add-package-link".into(), s(path)],
            None,
            true,
        )?;
    }
    let mut args = vec![pcp, "--add-package".into(), "--build-ide=".into()];
    // AnchorDocking é runtime; somente seu pacote de design entra na IDE.
    args.extend(packages[1..].iter().map(|p| s(p)));
    ctx.run(&lazbuild, &args, None, true)
        .context("instalando OPM e designer integrado na IDE Lazarus")?;
    Ok(())
}

pub fn executar(cfg: &PascalConfig, ctx: &Ctx) -> Result<()> {
    crate::platform::fpcup_asset(std::env::consts::OS, std::env::consts::ARCH)?;
    for target in &cfg.cross {
        if !CROSS.iter().any(|(id, _, _)| *id == target) {
            bail!("Alvo cross desconhecido: {target}");
        }
    }
    match crate::config_store::save_pascal(cfg) {
        Ok(path) => ctx.say(format!("Configuração salva em: {}", path.display())),
        Err(error) => ctx.say(format!(
            "AVISO: não foi possível salvar a configuração: {error:#}"
        )),
    }
    fs::create_dir_all(&cfg.base)?;
    verificar_dependencias_linux(ctx)?;
    if cfg!(target_os = "macos") {
        ctx.step("Verificando ferramentas do macOS");
        for tool in ["xcrun", "make", "svn", "unzip"] {
            if which(tool).is_none() {
                bail!("Ferramenta ausente: {tool}. Instale as Command Line Tools do Xcode (xcode-select --install) e o cliente SVN.");
            }
        }
        ctx.run("xcrun", &["--find".into(), "clang".into()], None, true)?;
    }
    let bootstrap = baixar_fpcup(ctx, &cfg.base.join("_fpcup"))?;
    instalar_fpc_lazarus(ctx, &bootstrap, cfg)?;
    instalar_cross(ctx, &bootstrap, cfg)?;
    configurar_ide(ctx, &cfg.base)?;
    ctx.step("Concluído");
    ctx.say(format!(
        "Ambiente Pascal pronto em {}. Use ACBrInstall para instalar os componentes.",
        cfg.base.display()
    ));
    Ok(())
}
