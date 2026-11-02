//! Catálogo real de pacotes Lazarus, seleção e dependências do ACBr.
use anyhow::{bail, Context, Result};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Package {
    pub name: String,
    pub description: String,
    pub group: String,
    pub path: PathBuf,
    pub dependencies: Vec<String>,
    pub requires_fastreport: bool,
    pub design: bool,
}

pub struct Catalog {
    pub items: Vec<Package>,
    names: HashMap<String, usize>,
}

impl Catalog {
    pub fn load(acbr: &Path) -> Result<Self> {
        Self::load_for(acbr, crate::config::Ide::Lazarus)
    }

    pub fn load_for(acbr: &Path, ide: crate::config::Ide) -> Result<Self> {
        fn walk(dir: &Path, paths: &mut Vec<PathBuf>, extension: &str) -> Result<()> {
            for entry in fs::read_dir(dir).with_context(|| format!("lendo {}", dir.display()))? {
                let entry = entry?;
                let kind = entry.file_type()?;
                let path = entry.path();
                if kind.is_dir() {
                    walk(&path, paths, extension)?;
                } else if kind.is_file()
                    && path
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case(extension))
                {
                    paths.push(path);
                }
            }
            Ok(())
        }
        let delphi = ide == crate::config::Ide::Delphi;
        let extension = if delphi { "dpk" } else { "lpk" };
        let root = acbr.join(if delphi {
            "Pacotes/Delphi"
        } else {
            "Pacotes/Lazarus"
        });
        let mut paths = Vec::new();
        walk(&root, &mut paths, extension)?;
        if paths.is_empty() {
            bail!(
                "Nenhum pacote .{extension} encontrado em {}",
                root.display()
            );
        }
        let mut items = Vec::new();
        for path in paths {
            if delphi {
                items.push(crate::delphi::package(&path, &root)?);
                continue;
            }
            let bytes = fs::read(&path)?;
            let xml = String::from_utf8_lossy(&bytes);
            let doc = roxmltree::Document::parse(&xml)
                .with_context(|| format!("lendo {}", path.display()))?;
            let package = doc
                .descendants()
                .find(|n| n.has_tag_name("Package"))
                .context("XML sem Package")?;
            let attr = |tag| {
                package
                    .children()
                    .find(|n| n.has_tag_name(tag))
                    .and_then(|n| n.attribute("Value"))
            };
            let name = attr("Name").context("Pacote sem nome")?.to_string();
            let description = attr("Description").unwrap_or_default().to_string();
            let dependencies: Vec<String> = package
                .children()
                .find(|n| n.has_tag_name("RequiredPkgs"))
                .into_iter()
                .flat_map(|n| n.descendants())
                .filter(|n| n.has_tag_name("PackageName"))
                .filter_map(|n| n.attribute("Value").map(str::to_string))
                .collect();
            let requires_fastreport = dependencies.iter().any(|d| {
                let d = d.to_ascii_lowercase();
                d.starts_with("frx") || d == "fr_lazarus" || d.contains("fastreport")
            }) || path
                .strip_prefix(&root)?
                .components()
                .any(|c| c.as_os_str().eq_ignore_ascii_case("Fast"));
            let group = path
                .strip_prefix(&root)?
                .components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .unwrap_or_default();
            items.push(Package {
                name,
                description,
                group,
                dependencies,
                path,
                requires_fastreport,
                design: attr("Type").is_some_and(|t| t.contains("DesignTime")),
            });
        }
        items.sort_by_key(|p| (p.group.to_ascii_lowercase(), p.name.to_ascii_lowercase()));
        let mut names = HashMap::new();
        for (i, package) in items.iter().enumerate() {
            if names.insert(package.name.to_ascii_lowercase(), i).is_some() {
                bail!("Pacote duplicado: {}", package.name);
            }
        }
        // Propaga a exigência do FastReport aos componentes que dependem dele.
        loop {
            let requires_fastreport: Vec<usize> = items
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    !p.requires_fastreport
                        && p.dependencies.iter().any(|d| {
                            names
                                .get(&d.to_ascii_lowercase())
                                .is_some_and(|&j| items[j].requires_fastreport)
                        })
                })
                .map(|(i, _)| i)
                .collect();
            if requires_fastreport.is_empty() {
                break;
            }
            for i in requires_fastreport {
                items[i].requires_fastreport = true;
            }
        }
        Ok(Self { items, names })
    }

    pub fn defaults(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|p| !p.requires_fastreport)
            .map(|p| p.name.clone())
            .collect()
    }

    pub fn resolve(&self, selection: Option<&[String]>) -> Result<Vec<usize>> {
        self.resolve_with(selection, false)
    }

    pub fn all(&self) -> Vec<String> {
        self.items.iter().map(|p| p.name.clone()).collect()
    }

    pub fn resolve_with(&self, selection: Option<&[String]>, all: bool) -> Result<Vec<usize>> {
        fn visit(
            catalog: &Catalog,
            i: usize,
            active: &mut HashSet<usize>,
            done: &mut HashSet<usize>,
            result: &mut Vec<usize>,
        ) -> Result<()> {
            if done.contains(&i) {
                return Ok(());
            }
            let package = &catalog.items[i];
            if !active.insert(i) {
                bail!("Dependência circular em {}", package.name);
            }
            for dependency in &package.dependencies {
                if let Some(&j) = catalog.names.get(&dependency.to_ascii_lowercase()) {
                    visit(catalog, j, active, done, result)?;
                }
            }
            active.remove(&i);
            done.insert(i);
            result.push(i);
            Ok(())
        }
        let defaults;
        let selection = match selection {
            Some(s) => s,
            None => {
                defaults = if all { self.all() } else { self.defaults() };
                &defaults
            }
        };
        let mut result = Vec::new();
        let mut done = HashSet::new();
        for name in selection {
            let &i = self
                .names
                .get(&name.to_ascii_lowercase())
                .with_context(|| format!("Componente ACBr não encontrado: {name}"))?;
            visit(self, i, &mut HashSet::new(), &mut done, &mut result)?;
        }
        Ok(result)
    }

    pub fn required(&self, plan: &[usize]) -> HashSet<String> {
        plan.iter()
            .flat_map(|&i| &self.items[i].dependencies)
            .filter(|d| self.names.contains_key(&d.to_ascii_lowercase()))
            .map(|d| d.to_ascii_lowercase())
            .collect()
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Retira da reconstrução da IDE os pacotes ACBr desmarcados, preservando os demais.
pub fn sync_installed(pcp: &Path, catalog: &Catalog, selected: &[String]) -> Result<()> {
    let path = pcp.join("miscellaneousoptions.xml");
    if !path.is_file() {
        return Ok(());
    }
    let xml = fs::read_to_string(&path)?;
    let doc = roxmltree::Document::parse(&xml)?;
    let Some(list) = doc
        .descendants()
        .find(|n| n.has_tag_name("StaticAutoInstallPackages"))
    else {
        return Ok(());
    };
    let selected: HashSet<String> = selected.iter().map(|n| n.to_ascii_lowercase()).collect();
    let old: Vec<&str> = list
        .children()
        .filter_map(|n| n.attribute("Value"))
        .collect();
    let keep: Vec<&str> = old
        .iter()
        .copied()
        .filter(|name| {
            !catalog.names.contains_key(&name.to_ascii_lowercase())
                || selected.contains(&name.to_ascii_lowercase())
        })
        .collect();
    if keep.len() == old.len() {
        return Ok(());
    }
    let mut replacement = format!("<StaticAutoInstallPackages Count=\"{}\">", keep.len());
    for (i, name) in keep.iter().enumerate() {
        replacement.push_str(&format!(
            "\n        <Item{} Value=\"{}\"/>",
            i + 1,
            escape(name)
        ));
    }
    replacement.push_str("\n      </StaticAutoInstallPackages>");
    let range = list.range();
    let mut updated = xml.clone();
    updated.replace_range(range, &replacement);
    fs::copy(
        &path,
        pcp.join("miscellaneousoptions.acbrinstall-backup.xml"),
    )?;
    let temp = pcp.join(format!(".miscellaneousoptions-{}.tmp", std::process::id()));
    fs::write(&temp, updated)?;
    fs::rename(&temp, &path).context("atualizando seleção de pacotes na IDE")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sources(PathBuf);
    impl Sources {
        fn new() -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root =
                std::env::temp_dir().join(format!("acbr-selection-{}-{stamp}", std::process::id()));
            fs::create_dir_all(root.join("Pacotes/Lazarus")).unwrap();
            Self(root)
        }
        fn package(&self, name: &str, dependencies: &[&str]) {
            let dependencies: String = dependencies
                .iter()
                .enumerate()
                .map(|(i, d)| format!(r#"<Item{i}><PackageName Value="{d}"/></Item{i}>"#))
                .collect();
            fs::write(self.0.join("Pacotes/Lazarus").join(format!("{name}.lpk")), format!(r#"<CONFIG><Package><Name Value="{name}"/><RequiredPkgs>{dependencies}</RequiredPkgs></Package></CONFIG>"#)).unwrap();
        }
    }
    impl Drop for Sources {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn selects_dependencies_in_build_order_and_keeps_fastreport_optional() {
        let sources = Sources::new();
        sources.package("Comum", &[]);
        sources.package("Boleto", &["Comum"]);
        sources.package("Fortes", &["Boleto", "frce"]);
        sources.package(
            "Fast",
            &["Boleto", "frxe_lazarus", "frxPDFLazarus", "fr_lazarus"],
        );
        sources.package("Indirect", &["Fast"]);
        sources.package("Other", &[]);
        let catalog = Catalog::load(&sources.0).unwrap();
        let selected = vec!["FORTES".into()];
        let plan = catalog.resolve(Some(&selected)).unwrap();
        assert_eq!(
            plan.iter()
                .map(|&i| catalog.items[i].name.as_str())
                .collect::<Vec<_>>(),
            ["Comum", "Boleto", "Fortes"]
        );
        assert_eq!(
            catalog.required(&plan),
            HashSet::from(["comum".into(), "boleto".into()])
        );
        let defaults = catalog.resolve(None).unwrap();
        assert_eq!(defaults.len(), 4);
        assert!(defaults
            .iter()
            .all(|&i| !["Fast", "Indirect"].contains(&catalog.items[i].name.as_str())));
        assert_eq!(catalog.resolve(Some(&["Fast".into()])).unwrap().len(), 3);
        assert_eq!(
            catalog.resolve(Some(&["Indirect".into()])).unwrap().len(),
            4
        );
        assert_eq!(catalog.resolve_with(None, true).unwrap().len(), 6);
        assert!(catalog.resolve(Some(&["Unknown".into()])).is_err());
        assert!(catalog.resolve(Some(&[])).unwrap().is_empty());
        assert_eq!(
            catalog
                .resolve(Some(&["Boleto".into(), "Comum".into(), "Boleto".into()]))
                .unwrap()
                .len(),
            2
        );
        sources.package("CycleA", &["CycleB"]);
        sources.package("CycleB", &["CycleA"]);
        assert!(Catalog::load(&sources.0)
            .unwrap()
            .resolve(Some(&["CycleA".into()]))
            .is_err());
    }

    #[test]
    fn removes_deselected_acbr_from_ide_but_preserves_other_packages_and_backup() {
        let sources = Sources::new();
        sources.package("Boleto", &[]);
        sources.package("Other", &[]);
        sources.package("Fast", &["fr_lazarus"]);
        let catalog = Catalog::load(&sources.0).unwrap();
        let xml = r#"<CONFIG><Theme Value="dark"/><StaticAutoInstallPackages Count="4"><Item1 Value="OnlinePackageManager"/><Item2 Value="Boleto"/><Item3 Value="Other"/><Item4 Value="Fast"/></StaticAutoInstallPackages></CONFIG>"#;
        let path = sources.0.join("miscellaneousoptions.xml");
        fs::write(&path, xml).unwrap();
        sync_installed(&sources.0, &catalog, &["Boleto".into()]).unwrap();
        let result = fs::read_to_string(&path).unwrap();
        let doc = roxmltree::Document::parse(&result).unwrap();
        let list = doc
            .descendants()
            .find(|n| n.has_tag_name("StaticAutoInstallPackages"))
            .unwrap();
        assert_eq!(list.attribute("Count"), Some("2"));
        assert_eq!(
            list.children()
                .filter_map(|n| n.attribute("Value"))
                .collect::<Vec<_>>(),
            ["OnlinePackageManager", "Boleto"]
        );
        assert!(result.contains(r#"<Theme Value="dark"/>"#));
        assert_eq!(
            fs::read_to_string(
                sources
                    .0
                    .join("miscellaneousoptions.acbrinstall-backup.xml")
            )
            .unwrap(),
            xml
        );
        sync_installed(&sources.0, &catalog, &["Boleto".into()]).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), result);
    }
}
