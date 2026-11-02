//! Backend Delphi: detecção no Windows, pacotes DPK e registro na IDE.
use crate::{
    config::{AcbrConfig, DelphiOptions},
    packages::{Catalog, Package},
    runtime::Ctx,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Installation {
    pub version: String,
    pub root: PathBuf,
    pub registry_key: String,
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn powershell(script: &str) -> Result<serde_json::Value> {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &format!("$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[System.Text.Encoding]::UTF8; {script}")]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command
        .output()
        .context("executando consulta ao registro do Delphi")?;
    if !output.status.success() {
        bail!(
            "Falha na consulta ao Delphi: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("resposta inválida ao consultar Delphi")
}

pub fn discover() -> Result<Vec<Installation>> {
    if !cfg!(windows) {
        return Ok(Vec::new());
    }
    let value = powershell(
        r#"
$result=@();
foreach($vendor in @('Embarcadero','CodeGear','Borland')) {
  $key='Registry::HKEY_CURRENT_USER\Software\'+$vendor+'\BDS';
  if(Test-Path -LiteralPath $key) {
    foreach($version in Get-ChildItem -LiteralPath $key) {
      $root=$version.GetValue('RootDir');
      if(!$root){
        foreach($machine in @('Registry::HKEY_LOCAL_MACHINE\SOFTWARE\','Registry::HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\')){
          $machineKey=$machine+$vendor+'\BDS\'+$version.PSChildName;
          if(Test-Path -LiteralPath $machineKey){$root=(Get-Item -LiteralPath $machineKey).GetValue('RootDir');if($root){break}}
        }
      }
      if($root -and (Test-Path -LiteralPath (Join-Path $root 'bin\dcc32.exe'))) {
        $result+=@{version=$version.PSChildName;root=$root;registry_key=('HKCU\Software\'+$vendor+'\BDS\'+$version.PSChildName)}
      }
    }
  }
}
ConvertTo-Json -InputObject @($result)
"#,
    )?;
    let mut installations: Vec<Installation> = serde_json::from_value(value)?;
    installations.sort_by(|a, b| {
        b.version
            .parse::<f64>()
            .unwrap_or(0.0)
            .total_cmp(&a.version.parse::<f64>().unwrap_or(0.0))
    });
    Ok(installations)
}

pub fn validate(options: &DelphiOptions) -> Result<()> {
    if !cfg!(windows) {
        bail!("Delphi está disponível somente no Windows.");
    }
    if !options.root.join("bin/dcc32.exe").is_file() {
        bail!("dcc32.exe não encontrado em {}", options.root.display());
    }
    let key = options.registry_key.replace('/', "\\");
    let valid = ["Embarcadero", "CodeGear", "Borland"].iter().any(|vendor| {
        key.strip_prefix(&format!("HKCU\\Software\\{vendor}\\BDS\\"))
            .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.'))
    });
    if !valid {
        bail!("Selecione uma versão Delphi detectada ou informe --delphi-key HKCU\\Software\\Embarcadero\\BDS\\<versão>.");
    }
    if key
        .rsplit('\\')
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
        < 9.0
    {
        bail!("O backend Delphi inicial suporta XE2 ou posterior (BDS 9.0+).");
    }
    if (options.win64 || options.ide64) && !options.root.join("bin/dcc64.exe").is_file() {
        bail!("dcc64.exe não encontrado nesta instalação Delphi.");
    }
    if options.ide64
        && !options
            .root
            .join("lib/win64/release/designide.dcp")
            .is_file()
    {
        bail!("Esta instalação Delphi não oferece pacotes para a IDE de 64 bits.");
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
enum Token {
    Id(String),
    Text(String),
    Punct(char),
}

fn tokens(source: &str) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' {
            i += 1;
            while i < chars.len() && chars[i] != '}' {
                i += 1;
            }
            i += 1;
        } else if c == '(' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == ')') {
                i += 1;
            }
            i += 2;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '\'' {
            i += 1;
            let mut text = String::new();
            while i < chars.len() {
                if chars[i] == '\'' {
                    if chars.get(i + 1) == Some(&'\'') {
                        text.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                text.push(chars[i]);
                i += 1;
            }
            result.push(Token::Text(text));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            result.push(Token::Id(chars[start..i].iter().collect()));
        } else {
            if !c.is_whitespace() {
                result.push(Token::Punct(c));
            }
            i += 1;
        }
    }
    result
}

pub(crate) fn package(path: &Path, root: &Path) -> Result<Package> {
    let bytes = fs::read(path)?;
    let source = String::from_utf8_lossy(&bytes);
    let tokens = tokens(&source);
    let name = tokens
        .windows(2)
        .find_map(|w| match (&w[0], &w[1]) {
            (Token::Id(k), Token::Id(name)) if k.eq_ignore_ascii_case("package") => {
                Some(name.clone())
            }
            _ => None,
        })
        .context("DPK sem declaração package")?;
    let mut dependencies = Vec::new();
    let mut requires = false;
    for token in &tokens {
        match token {
            Token::Id(id) if id.eq_ignore_ascii_case("requires") => requires = true,
            Token::Punct(';') if requires => break,
            Token::Id(id) if requires => dependencies.push(id.clone()),
            _ => {}
        }
    }
    let upper = source.to_ascii_uppercase();
    let design = upper.contains("{$DESIGNONLY}")
        || (!upper.contains("{$RUNONLY}") && name.to_ascii_uppercase().starts_with("DCL"));
    let group = path
        .strip_prefix(root)?
        .components()
        .next()
        .context("DPK sem diretório")?
        .as_os_str()
        .to_string_lossy()
        .into_owned();
    let requires_fastreport = path
        .strip_prefix(root)?
        .components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("Fast"))
        || dependencies.iter().any(|n| {
            n.to_ascii_lowercase().starts_with("frx") || n.eq_ignore_ascii_case("fr_lazarus")
        });
    Ok(Package {
        name,
        description: if design {
            "Componentes para a paleta da IDE"
        } else {
            "Biblioteca de execução"
        }
        .into(),
        group,
        path: path.into(),
        dependencies,
        requires_fastreport,
        design,
    })
}

fn source_dirs(package: &Package) -> Result<BTreeSet<PathBuf>> {
    let bytes = fs::read(&package.path)?;
    let tokens = tokens(&String::from_utf8_lossy(&bytes));
    let mut result = BTreeSet::new();
    result.insert(
        package
            .path
            .parent()
            .context("DPK sem diretório")?
            .to_path_buf(),
    );
    for pair in tokens.windows(2) {
        if let (Token::Id(word), Token::Text(file)) = (&pair[0], &pair[1]) {
            if word.eq_ignore_ascii_case("in") {
                let path = package.path.parent().unwrap().join(file.replace('\\', "/"));
                if let Some(parent) = path.parent() {
                    result.insert(parent.to_path_buf());
                }
            }
        }
    }
    Ok(result)
}

pub fn compiler_args(
    package: &Package,
    output: &Path,
    search: &[PathBuf],
    debug: bool,
) -> Vec<String> {
    let paths = search
        .iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join(";");
    let mut args=vec!["-B".into(), "-Q".into(), "-$Q-".into(), "-$R-".into(),
        format!("-N0{}",output.display()),format!("-LE{}",output.display()),format!("-LN{}",output.display()),
        format!("-U{paths}"),format!("-I{paths}"),format!("-R{paths}"),
        "-NSSystem;Xml;Data;Datasnap;Web;Soap;Winapi;System.Win;Data.Win;Vcl;Vcl.Imaging;Vcl.Touch;Vcl.Samples;Vcl.Shell".into()];
    args.extend(
        if debug {
            ["-$O-", "-$W+", "-$D+", "-$L+", "-$Y+", "-$C+", "-DDEBUG"]
        } else {
            ["-$O+", "-$W-", "-$D-", "-$L-", "-$Y-", "-$C-", "-DRELEASE"]
        }
        .map(str::to_string),
    );
    if debug {
        args.push("-V".into());
    }
    args.push(package.path.to_string_lossy().into_owned());
    args
}

fn write_registry(ctx: &Ctx, script: String) -> Result<()> {
    ctx.run(
        "powershell.exe",
        &[
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            format!("$ErrorActionPreference='Stop'; {script}"),
        ],
        None,
        true,
    )?;
    Ok(())
}

pub fn install(ctx: &Ctx, cfg: &AcbrConfig, acbr: &Path) -> Result<()> {
    validate(&cfg.delphi)?;
    let catalog = Catalog::load_for(acbr, crate::config::Ide::Delphi)?;
    let plan = catalog.resolve_with(cfg.acbr_packages.as_deref(), cfg.all_components)?;
    if plan.is_empty() {
        bail!("Selecione pelo menos um componente ACBr.");
    }
    let fastreport_dir = cfg
        .fastreport_dir
        .as_ref()
        .filter(|_| plan.iter().any(|&i| catalog.items[i].requires_fastreport));
    if let Some(directory) = fastreport_dir {
        if !directory.is_dir() {
            bail!("Pasta FastReport não encontrada: {}", directory.display());
        }
    }
    let key = cfg
        .delphi
        .registry_key
        .replace("HKCU\\", "HKEY_CURRENT_USER\\");
    let key_ps = format!("Registry::{key}");
    let version = key.rsplit('\\').next().unwrap();
    let root = acbr.join("Lib/Delphi/ACBrInstall").join(version);
    fs::create_dir_all(&root)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let backup = root.join(format!("registry-{stamp}.reg"));
    ctx.step("Preparando instalação ACBr para Delphi (Windows)");
    ctx.run(
        "reg.exe",
        &[
            "export".into(),
            cfg.delphi.registry_key.clone(),
            backup.to_string_lossy().into_owned(),
            "/y".into(),
        ],
        None,
        true,
    )?;
    ctx.say(format!(
        "Backup da configuração Delphi: {}",
        backup.display()
    ));
    let mut source = BTreeSet::new();
    source.insert(acbr.join("Fontes/ACBrComum"));
    for &i in &plan {
        source.extend(source_dirs(&catalog.items[i])?);
    }
    let mut platforms = vec!["Win32"];
    if cfg.delphi.win64 || cfg.delphi.ide64 {
        platforms.push("Win64");
    }
    for platform in platforms {
        let lib_key = format!("{key_ps}\\Library\\{platform}");
        let search_value=powershell(&format!("$p='';if(Test-Path -LiteralPath {0}){{$k=Get-Item -LiteralPath {0};$p=[string]$k.GetValue('Search Path','',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)}};ConvertTo-Json -InputObject $p", quote(&lib_key)))?;
        let old = search_value.as_str().unwrap_or_default();
        // Expande macros usando a própria configuração da instalação selecionada.
        let expanded = powershell(&format!(
            r#"
$env:BDS={root}; $env:BDSLIB=Join-Path $env:BDS 'lib'; $env:BDSINCLUDE=Join-Path $env:BDS 'include';
$common=[Environment]::GetFolderPath('CommonDocuments');
foreach($suffix in @('Embarcadero\Studio\{version}','RAD Studio\{version}','Embarcadero\RAD Studio\{version}')){{
  $candidate=Join-Path $common $suffix;
  if(Test-Path -LiteralPath $candidate){{$env:BDSCOMMONDIR=$candidate;break}}
}}
$vars={vars};
if(Test-Path -LiteralPath $vars){{$k=Get-Item -LiteralPath $vars;foreach($name in $k.GetValueNames()){{[Environment]::SetEnvironmentVariable($name,[string]$k.GetValue($name),'Process')}}}}
$p={paths};
for($i=0;$i -lt 3;$i++){{
  $p=[Environment]::ExpandEnvironmentVariables($p);
  $p=[regex]::Replace($p,'\$\(([^)]+)\)',{{param($m) $v=[Environment]::GetEnvironmentVariable($m.Groups[1].Value);if($v){{$v}}else{{$m.Value}}}})
}}
ConvertTo-Json -InputObject $p
"#,
            root = quote(&cfg.delphi.root.to_string_lossy()),
            version = version,
            vars = quote(&format!("{key_ps}\\Environment Variables")),
            paths = quote(old)
        ))?;
        let compiler = cfg.delphi.root.join(if platform == "Win32" {
            "bin/dcc32.exe"
        } else {
            "bin/dcc64.exe"
        });
        let mut modes = vec!["Release"];
        if cfg.delphi.debug {
            modes.push("Debug");
        }
        let mut release_packages = Vec::new();
        for mode in modes {
            let output = root.join(platform).join(mode);
            fs::create_dir_all(&output)?;
            let mut search = vec![
                output.clone(),
                cfg.delphi
                    .root
                    .join("lib")
                    .join(platform.to_ascii_lowercase())
                    .join("release"),
            ];
            if mode == "Debug" {
                search.push(root.join(platform).join("Release"));
            }
            search.extend(
                expanded
                    .as_str()
                    .unwrap_or_default()
                    .split(';')
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from),
            );
            search.extend(source.iter().cloned());
            if let Some(directory) = fastreport_dir {
                let per_platform = directory.join(platform).join(mode);
                search.push(directory.clone());
                if mode == "Debug" {
                    let release = directory.join(platform).join("Release");
                    if release.is_dir() {
                        search.insert(1, release);
                    }
                }
                if per_platform.is_dir() {
                    search.insert(1, per_platform);
                }
            }
            for &i in &plan {
                let package = &catalog.items[i];
                if package.design && platform == "Win64" && !cfg.delphi.ide64 {
                    continue;
                }
                ctx.step(&format!(
                    "Delphi {version} / {platform} / {mode}: {}",
                    package.name
                ));
                ctx.run(
                    &compiler,
                    &compiler_args(package, &output, &search, mode == "Debug"),
                    package.path.parent(),
                    true,
                )
                .with_context(|| format!("compilando {} no Delphi", package.name))?;
                if package.design && mode == "Release" {
                    let bpl = output.join(format!("{}.bpl", package.name));
                    if !bpl.is_file() {
                        bail!("BPL não gerado: {}", bpl.display());
                    }
                    release_packages.push((bpl, package.name.clone()));
                }
            }
        }
        let selected: Vec<serde_json::Value> = release_packages
            .iter()
            .map(|(path, name)| serde_json::json!({"path":path,"name":name}))
            .collect();
        let names: Vec<&str> = catalog.items.iter().map(|p| p.name.as_str()).collect();
        let known = format!(
            "{key_ps}\\{}",
            if platform == "Win32" {
                "Known Packages"
            } else {
                "Known Packages x64"
            }
        );
        let cache = format!(
            "{key_ps}\\{}",
            if platform == "Win32" {
                "Package Cache"
            } else {
                "Package Cache x64"
            }
        );
        let release = root.join(platform).join("Release");
        let debug = root
            .join(platform)
            .join(if cfg.delphi.debug { "Debug" } else { "Release" });
        write_registry(
            ctx,
            format!(
                r#"
$known={known}; $lib={lib}; $cache={cache};
$names=ConvertFrom-Json {names}; $selected=ConvertFrom-Json {selected};
if(!(Test-Path -LiteralPath $known)){{New-Item -Path $known -Force | Out-Null}}
$key=Get-Item -LiteralPath $known;
foreach($name in @($key.GetValueNames())){{
  if($names -contains [IO.Path]::GetFileNameWithoutExtension($name)){{$key.DeleteValue($name)}}
}}
foreach($entry in $selected){{
  $key.SetValue($entry.path,$entry.name,[Microsoft.Win32.RegistryValueKind]::String);
  $cached=$cache+'\'+[IO.Path]::GetFileName($entry.path);
  if(Test-Path -LiteralPath $cached){{Remove-Item -LiteralPath $cached -Recurse -Force}}
}}
if(!(Test-Path -LiteralPath $lib)){{New-Item -Path $lib -Force | Out-Null}}
$key=Get-Item -LiteralPath $lib;
foreach($value in @('Search Path','Package Search Path','Debug DCU Path')){{
  $append=if($value -eq 'Debug DCU Path'){{{debug}}}else{{{release}}};
  $old=[string]$key.GetValue($value,'',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames);
  $parts=@($old.Split(';') | Where-Object {{$_ -and $_ -ne $append}})+@($append);
  $key.SetValue($value,($parts -join ';'),[Microsoft.Win32.RegistryValueKind]::String)
}}
"#,
                known = quote(&known),
                lib = quote(&lib_key),
                cache = quote(&cache),
                names = quote(&serde_json::to_string(&names)?),
                selected = quote(&serde_json::to_string(&selected)?),
                release = quote(&release.to_string_lossy()),
                debug = quote(&debug.to_string_lossy())
            ),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lexer_ignores_comments_and_preserves_pascal_paths() {
        let source="// requires Fake;\npackage Example; {$RUNONLY} requires rtl, ACBr_Comum; contains Test in '..\\Fontes\\Test.pas'; end.";
        let tokens = tokens(source);
        assert!(!tokens.contains(&Token::Id("Fake".into())));
        assert!(tokens.contains(&Token::Text("..\\Fontes\\Test.pas".into())));
        assert!(tokens.contains(&Token::Id("ACBr_Comum".into())));
    }
    #[test]
    fn unsupported_hosts_do_not_detect_or_install_delphi() {
        if !cfg!(windows) {
            assert!(discover().unwrap().is_empty());
            assert!(validate(&DelphiOptions::default()).is_err());
        }
    }

    #[test]
    fn dpk_catalog_resolves_design_runtime_and_keeps_paid_integrations_optional() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("delphi-catalog-{}-{stamp}", std::process::id()));
        let packages = root.join("Pacotes/Delphi/ACBrNFe");
        fs::create_dir_all(&packages).unwrap();
        fs::write(packages.join("ACBr_NFe.dpk"),"package ACBr_NFe; {$RUNONLY} requires rtl; contains UnitA in '..\\..\\Fontes\\UnitA.pas'; end.").unwrap();
        fs::write(
            packages.join("DCLACBr_NFe.dpk"),
            "package DCLACBr_NFe; {$DESIGNONLY} requires rtl, designide, ACBr_NFe; end.",
        )
        .unwrap();
        fs::write(
            packages.join("ACBr_Fast.dpk"),
            "package ACBr_Fast; {$RUNONLY} requires rtl, frx; end.",
        )
        .unwrap();
        let catalog = Catalog::load_for(&root, crate::config::Ide::Delphi).unwrap();
        let plan = catalog.resolve(Some(&["DCLACBr_NFe".into()])).unwrap();
        assert_eq!(
            plan.iter()
                .map(|&i| catalog.items[i].name.as_str())
                .collect::<Vec<_>>(),
            ["ACBr_NFe", "DCLACBr_NFe"]
        );
        assert!(!catalog.items[plan[0]].design);
        assert!(catalog.items[plan[1]].design);
        assert_eq!(catalog.resolve(None).unwrap().len(), 2);
        assert_eq!(
            catalog.resolve(Some(&["ACBr_Fast".into()])).unwrap().len(),
            1
        );
        assert_eq!(catalog.resolve_with(None, true).unwrap().len(), 3);
        let output = root.join("build with spaces/Win32/Release");
        let search = vec![root.join("library with spaces"), root.join("ACBrComum")];
        let args = compiler_args(&catalog.items[plan[0]], &output, &search, false);
        assert!(args.contains(&"-$O+".into()));
        assert!(args.contains(&format!("-LE{}", output.display())));
        assert!(args.contains(&format!(
            "-U{};{}",
            search[0].display(),
            search[1].display()
        )));
        assert_eq!(
            args.last().unwrap(),
            &catalog.items[plan[0]].path.to_string_lossy()
        );
        let debug = compiler_args(&catalog.items[plan[0]], &root.join("Debug"), &search, true);
        assert!(
            debug.contains(&"-$O-".into())
                && debug.contains(&"-DDEBUG".into())
                && debug.contains(&"-V".into())
        );
        assert!(!debug.contains(&"-DRELEASE".into()));
        fs::remove_dir_all(root).unwrap();
    }
}
