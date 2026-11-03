use crate::{
    config::{AcbrAction, AcbrConfig, Ide, PascalConfig},
    config_store,
    runtime::Ctx,
    Product,
};
use anyhow::{bail, Context, Result};
use std::{path::PathBuf, sync::Arc};

pub enum Request {
    Acbr(AcbrConfig),
    Pascal(PascalConfig),
}
pub struct Options {
    pub request: Request,
    pub list: bool,
    pub environments: bool,
    pub configure_only: bool,
    pub build_only: bool,
}

pub fn help(product: Product) -> String {
    let common="Sem argumentos abre a interface gráfica.\n--cli               modo texto\n--dir <pasta>       ambiente Pascal/Lazarus\n--build-only        prepara o ambiente de compilação sem adicionar componentes à IDE (Lazarus)\n-V, --version       mostra a versão\n-h, --help          mostra esta ajuda\n";
    let specific=match product {
        Product::Pascal => "--fpc <versão>      versão do FPC (stable por padrão)\n--laz <versão>      versão do Lazarus\n--cross <lista>     win32,win64,linux32,linux64 (vazio desativa)\n--configure-ide     instala OPM e designer no Lazarus existente, sem baixar FPC\n",
        Product::Acbr => "--ide <alvo>        lazarus ou delphi (Delphi somente Windows)\n--acbr-dir <pasta>  raiz dos fontes ACBr\n--acbr-action <ação> install, download ou update\n--acbr-packages <lista> nomes de pacotes separados por vírgula\n--all-acbr          todos os pacotes, incluindo FastReport\n--default-acbr      seleção padrão, sem FastReport\n--fastreport-dir <pasta> FastReport já adquirido (opcional)\n--list-components  lista os pacotes sem compilar\n--list-environments lista instalações detectadas\n--delphi-dir <pasta> raiz de uma instalação Delphi\n--delphi-key <chave> perfil HKCU\\Software\\Embarcadero\\BDS\\<versão>\n--win64            compila também bibliotecas Delphi Win64\n--debug            compila também bibliotecas Delphi Debug\n--ide64            instala componentes também na IDE Delphi de 64 bits\n",
    };
    format!("{} {} — {}\n\nUso: {} --cli [opções]\n\n{common}{specific}\nAs últimas opções são restauradas do arquivo de configuração próprio.\n",product.title(),crate::VERSION,product.subtitle(),product.executable())
}

fn parse(
    product: Product,
    args: &[String],
    mut acbr: AcbrConfig,
    mut pascal: PascalConfig,
) -> Result<Options> {
    let (mut list, mut environments, mut configure_only) = (false, false, false);
    let mut build_only = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let mut value = || -> Result<String> {
            i += 1;
            let value = args.get(i).context("Falta um valor para a opção")?;
            if value.starts_with("--") {
                bail!("Falta um valor antes de {value}");
            }
            Ok(value.clone())
        };
        match (product, arg.as_str()) {
            (_, "--cli") => {}
            (_, "--build-only") => build_only = true,
            (Product::Pascal, "--dir") => pascal.base = PathBuf::from(value()?),
            (Product::Acbr, "--dir") => acbr.base = PathBuf::from(value()?),
            (Product::Pascal, "--fpc") => pascal.fpc = value()?,
            (Product::Pascal, "--laz") => pascal.laz = value()?,
            (Product::Pascal, "--cross") => {
                pascal.cross = value()?
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            }
            (Product::Pascal, "--configure-ide") => configure_only = true,
            (Product::Acbr, "--skip-fpc" | "--install-acbr") => {} // Compatibilidade com comandos anteriores.
            (Product::Acbr, "--ide") => {
                acbr.ide = match value()?.as_str() {
                    "lazarus" => Ide::Lazarus,
                    "delphi" => Ide::Delphi,
                    _ => bail!("IDE inválida: use lazarus ou delphi"),
                }
            }
            (Product::Acbr, "--acbr-dir") => acbr.acbr_dir = Some(PathBuf::from(value()?)),
            (Product::Acbr, "--acbr-action") => {
                acbr.acbr_action = match value()?.as_str() {
                    "install" => AcbrAction::Install,
                    "download" => AcbrAction::DownloadAndInstall,
                    "update" => AcbrAction::UpdateAndInstall,
                    _ => bail!("Ação ACBr inválida"),
                }
            }
            (Product::Acbr, "--acbr-packages") => {
                acbr.all_components = false;
                acbr.acbr_packages = Some(
                    value()?
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect(),
                )
            }
            (Product::Acbr, "--all-acbr") => {
                acbr.acbr_packages = None;
                acbr.all_components = true;
            }
            (Product::Acbr, "--default-acbr") => {
                acbr.acbr_packages = None;
                acbr.all_components = false;
            }
            (Product::Acbr, "--fastreport-dir") => {
                acbr.fastreport_dir = Some(PathBuf::from(value()?))
            }
            (Product::Acbr, "--list-components") => list = true,
            (Product::Acbr, "--list-environments") => environments = true,
            (Product::Acbr, "--delphi-dir") => acbr.delphi.root = PathBuf::from(value()?),
            (Product::Acbr, "--delphi-key") => acbr.delphi.registry_key = value()?,
            (Product::Acbr, "--win64") => acbr.delphi.win64 = true,
            (Product::Acbr, "--debug") => acbr.delphi.debug = true,
            (Product::Acbr, "--ide64") => acbr.delphi.ide64 = true,
            _ => bail!(
                "Opção não disponível em {}: {arg}. Use --help.",
                product.title()
            ),
        }
        i += 1;
    }
    if build_only && configure_only {
        bail!("--build-only e --configure-ide não podem ser usados juntos.");
    }
    if build_only && product == Product::Acbr && acbr.ide == Ide::Delphi {
        bail!("--build-only está disponível somente para Lazarus.");
    }
    Ok(Options {
        request: if product == Product::Acbr {
            Request::Acbr(acbr)
        } else {
            Request::Pascal(pascal)
        },
        list,
        environments,
        configure_only,
        build_only,
    })
}

pub fn run(product: Product, args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", product.title(), crate::VERSION);
        return 0;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{}", help(product));
        return 0;
    }
    match execute(product, args) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("ERRO: {error:#}");
            1
        }
    }
}

fn execute(product: Product, args: &[String]) -> Result<()> {
    let mut acbr = AcbrConfig::default();
    let mut pascal = PascalConfig::default();
    match product {
        Product::Acbr => match config_store::load_acbr() {
            Ok(Some(c)) => acbr = c,
            Ok(None) => {}
            Err(e) => eprintln!("AVISO: configuração ACBr inválida: {e:#}"),
        },
        Product::Pascal => match config_store::load_pascal() {
            Ok(Some(c)) => pascal = c,
            Ok(None) => {}
            Err(e) => eprintln!("AVISO: configuração Pascal inválida: {e:#}"),
        },
    }
    let options = parse(product, args, acbr, pascal)?;
    let ctx = Ctx::new(Arc::new(|line| println!("{line}")));
    let ctx = if options.list || options.environments {
        ctx
    } else {
        ctx.with_session_log(product)
    };
    let result = (|| match options.request {
        Request::Pascal(cfg) => {
            if options.configure_only {
                let path = config_store::save_pascal(&cfg)?;
                ctx.say(format!("Configuração salva em: {}", path.display()));
                crate::pascal::configurar_ide(&ctx, &cfg.base)?;
                ctx.say("OPM e designer integrado configurados. Reabra o Lazarus.");
                Ok(())
            } else if options.build_only {
                crate::pascal::preparar_compilacao(&cfg, &ctx)
            } else {
                crate::pascal::executar(&cfg, &ctx)
            }
        }
        Request::Acbr(mut cfg) => {
            if options.environments {
                for path in crate::inspection::discover_lazarus() {
                    println!("Lazarus: {}", path.display());
                }
                for installation in crate::delphi::discover()? {
                    println!(
                        "Delphi {}: {} [{}]",
                        installation.version,
                        installation.root.display(),
                        installation.registry_key
                    );
                }
                return Ok(());
            }
            if options.list {
                let sources = cfg
                    .acbr_dir
                    .clone()
                    .unwrap_or_else(|| cfg.base.join("ACBr"));
                let catalog = crate::packages::Catalog::load_for(&sources, cfg.ide)?;
                let selected =
                    catalog.resolve_with(cfg.acbr_packages.as_deref(), cfg.all_components)?;
                for (i, p) in catalog.items.iter().enumerate() {
                    println!(
                        "[{}] {}{}",
                        if selected.contains(&i) { "x" } else { " " },
                        p.name,
                        if p.requires_fastreport {
                            " (FastReport: requer instalação/licença)"
                        } else {
                            ""
                        }
                    );
                }
                return Ok(());
            }
            if cfg.ide == Ide::Delphi && cfg.delphi.registry_key.is_empty() {
                let installations = crate::delphi::discover()?;
                let selected = installations
                    .iter()
                    .find(|i| i.root == cfg.delphi.root)
                    .or_else(|| {
                        if cfg.delphi.root.as_os_str().is_empty() {
                            installations.first()
                        } else {
                            None
                        }
                    });
                if let Some(i) = selected {
                    cfg.delphi.root = i.root.clone();
                    cfg.delphi.registry_key = i.registry_key.clone();
                }
            }
            if options.build_only {
                crate::acbr::preparar_compilacao(&cfg, &ctx)
            } else {
                crate::acbr::executar(&cfg, &ctx)
            }
        }
    })();
    if let Err(error) = &result {
        ctx.record_failure(&format!("{error:#}"));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn build_only_is_explicit_and_rejects_incompatible_options() {
        for product in [Product::Acbr, Product::Pascal] {
            let result = parse(
                product,
                &args(&["--build-only"]),
                AcbrConfig::default(),
                PascalConfig::default(),
            )
            .unwrap();
            assert!(result.build_only);
        }
        assert!(parse(
            Product::Pascal,
            &args(&["--build-only", "--configure-ide"]),
            AcbrConfig::default(),
            PascalConfig::default()
        )
        .is_err());
        assert!(parse(
            Product::Acbr,
            &args(&["--build-only", "--ide", "delphi"]),
            AcbrConfig::default(),
            PascalConfig::default()
        )
        .is_err());
    }
    #[test]
    fn programs_reject_each_others_installation_options() {
        assert!(parse(
            Product::Acbr,
            &args(&["--fpc", "stable"]),
            AcbrConfig::default(),
            PascalConfig::default()
        )
        .is_err());
        assert!(parse(
            Product::Pascal,
            &args(&["--acbr-action", "install"]),
            AcbrConfig::default(),
            PascalConfig::default()
        )
        .is_err());
        let request = parse(
            Product::Acbr,
            &args(&["--ide", "delphi", "--win64", "--acbr-packages", "ACBr_NFe"]),
            AcbrConfig::default(),
            PascalConfig::default(),
        )
        .unwrap();
        let Request::Acbr(cfg) = request.request else {
            panic!()
        };
        assert_eq!(cfg.ide, Ide::Delphi);
        assert!(cfg.delphi.win64);
        assert_eq!(cfg.acbr_packages, Some(vec!["ACBr_NFe".into()]));
    }
}
