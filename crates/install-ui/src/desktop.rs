//! Wayland resolves taskbar/titlebar icons through app_id + a desktop entry.
//! Register user-local resources even for a standalone executable/AppImage.
use install_core::Product;
use std::{
    env, fs, io,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn template(product: Product) -> &'static str {
    match product {
        Product::Acbr => include_str!("../../../assets/linux/dev.rftd.acbrinstall.desktop"),
        Product::Pascal => include_str!("../../../assets/linux/dev.rftd.pascalinstall.desktop"),
    }
}

fn data_home() -> io::Result<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .filter(|p| !p.is_empty())
                .map(|p| PathBuf::from(p).join(".local/share"))
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "HOME e XDG_DATA_HOME não definidos",
            )
        })
}

fn launcher_executable(appimage: Option<PathBuf>, current: PathBuf) -> PathBuf {
    // current_exe points into a temporary mount when launched from an AppImage.
    appimage
        .filter(|p| p.is_absolute() && p.is_file())
        .unwrap_or(current)
}

fn exec_value(path: &Path) -> io::Result<String> {
    let raw = path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Caminho do executável não é UTF-8",
        )
    })?;
    if raw.contains(['\n', '\r', '\t']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Caminho do executável contém caracteres de controle",
        ));
    }
    // Desktop Entry string escaping is applied after Exec argument quoting.
    // There is no shell, but %, $, backticks, quotes and backslashes are special.
    let mut quoted = String::from("\"");
    for c in raw.chars() {
        match c {
            '%' => quoted.push_str("%%"),
            '\\' => quoted.push_str("\\\\"),
            '"' | '$' | '`' => {
                quoted.push('\\');
                quoted.push(c);
            }
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    Ok(quoted.replace('\\', "\\\\"))
}

fn write_changed(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    if fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(false);
    }
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = parent.join(format!(".installer-{}-{stamp}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map(|_| true)
}

fn install_to(directory: &Path, product: Product, executable: &Path) -> io::Result<bool> {
    let id = product.application_id();
    let icon = directory.join(format!("icons/hicolor/256x256/apps/{id}.png"));
    let mut changed = write_changed(&icon, crate::icons::png(product))?;
    let desktop = directory.join(format!("applications/{id}.desktop"));
    // An independently installed/customized launcher belongs to its owner.
    if fs::read_to_string(&desktop)
        .is_ok_and(|text| !text.lines().any(|line| line == "X-RFTD-Installer=true"))
    {
        return Ok(changed);
    }
    let exec = exec_value(executable)?;
    let text = template(product)
        .lines()
        .map(|line| {
            if line.starts_with("Exec=") {
                format!("Exec={exec}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    changed |= write_changed(&desktop, text.as_bytes())?;
    Ok(changed)
}

pub fn install(product: Product) -> io::Result<()> {
    let directory = data_home()?;
    let executable = launcher_executable(
        env::var_os("APPIMAGE").map(PathBuf::from),
        env::current_exe()?,
    );
    if install_to(&directory, product, &executable)? {
        // KDE caches desktop entries. Refresh before creating the Wayland window
        // so the first launch already resolves the registered icon.
        if env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .any(|d| d.eq_ignore_ascii_case("KDE"))
        {
            for tool in ["kbuildsycoca6", "kbuildsycoca5"] {
                if Command::new(tool)
                    .arg("--noincremental")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success())
                {
                    break;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_matching_ids_and_icons_without_rewriting_unchanged_resources_or_custom_launchers()
    {
        let root = env::temp_dir().join(format!(
            "desktop-icons-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let image = root.join("ACBr Install.AppImage");
        fs::write(&image, b"fixture").unwrap();
        let mounted = root.join(".mount-removed/usr/bin/acbrinstall");
        assert_eq!(
            launcher_executable(Some(image.clone()), mounted.clone()),
            image
        );
        assert_eq!(
            launcher_executable(Some(root.join("missing")), mounted.clone()),
            mounted
        );
        for product in [Product::Acbr, Product::Pascal] {
            assert!(install_to(&root, product, &image).unwrap());
            assert!(!install_to(&root, product, &image).unwrap());
            let id = product.application_id();
            let desktop = root.join(format!("applications/{id}.desktop"));
            let text = fs::read_to_string(&desktop).unwrap();
            assert!(text.contains(&format!("Icon={id}\n")));
            assert!(text.contains(&format!("StartupWMClass={id}\n")));
            assert!(text.contains(&format!("Exec=\"{}\"", image.display())));
            assert_eq!(
                fs::read(root.join(format!("icons/hicolor/256x256/apps/{id}.png"))).unwrap(),
                crate::icons::png(product)
            );
            fs::write(&desktop, "[Desktop Entry]\nName=Custom\nExec=custom\n").unwrap();
            assert!(!install_to(&root, product, &mounted).unwrap());
            assert!(fs::read_to_string(&desktop)
                .unwrap()
                .contains("Exec=custom"));
        }
        assert!(exec_value(Path::new("/tmp/a\nb")).is_err());
        assert_eq!(
            exec_value(Path::new("/tmp/100% $`\"\\")).unwrap(),
            "\"/tmp/100%% \\\\$\\\\`\\\\\"\\\\\\\\\""
        );
        fs::remove_dir_all(root).unwrap();
    }
}
