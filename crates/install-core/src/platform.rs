use anyhow::{bail, Result};
use std::{
    env,
    path::{Path, PathBuf},
};

pub const IS_WIN: bool = cfg!(windows);
pub const EXE: &str = if IS_WIN { ".exe" } else { "" };
pub const CROSS: [(&str, &str, &str); 4] = [
    ("win32", "i386", "win32"),
    ("win64", "x86_64", "win64"),
    ("linux32", "i386", "linux"),
    ("linux64", "x86_64", "linux"),
];

pub fn default_dir() -> PathBuf {
    if IS_WIN {
        PathBuf::from(r"C:\lazarus_dev")
    } else {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/opt"))
            .join("lazarus_dev")
    }
}
pub fn default_cross() -> Vec<String> {
    if cfg!(target_os = "macos") {
        Vec::new()
    } else if IS_WIN {
        vec!["linux64".into(), "linux32".into()]
    } else {
        vec!["win64".into(), "win32".into()]
    }
}

pub fn default_acbr_dir() -> PathBuf {
    std::env::var_os(if IS_WIN { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ACBr")
}
pub fn native_target() -> String {
    let os = if IS_WIN {
        "win64"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    format!("{}-{os}", env::consts::ARCH)
}
pub fn fpcup_asset(os: &str, arch: &str) -> Result<String> {
    let os = match os {
        "windows" => "win64",
        "linux" => "linux",
        "macos" => "darwin",
        _ => bail!("Sistema não suportado: {os}"),
    };
    if !matches!(arch, "x86_64" | "aarch64") {
        bail!("Arquitetura não suportada: {arch}");
    }
    Ok(format!(
        "fpclazup-{arch}-{os}{}",
        if os == "win64" { ".exe" } else { "" }
    ))
}
pub fn lazarus_dir(base: &Path) -> PathBuf {
    if base.join(format!("lazbuild{EXE}")).is_file() {
        base.to_path_buf()
    } else {
        base.join("lazarus")
    }
}
pub fn lazbuild(base: &Path) -> PathBuf {
    lazarus_dir(base).join(format!("lazbuild{EXE}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_native_bootstrap_for_supported_systems() {
        assert_eq!(
            fpcup_asset("macos", "aarch64").unwrap(),
            "fpclazup-aarch64-darwin"
        );
        assert_eq!(
            fpcup_asset("macos", "x86_64").unwrap(),
            "fpclazup-x86_64-darwin"
        );
        assert_eq!(
            fpcup_asset("windows", "x86_64").unwrap(),
            "fpclazup-x86_64-win64.exe"
        );
        assert_eq!(
            fpcup_asset("linux", "x86_64").unwrap(),
            "fpclazup-x86_64-linux"
        );
        assert!(fpcup_asset("linux", "i386").is_err());
    }
}

pub fn primary_config(base: &Path) -> PathBuf {
    let pcp = base.join("config_lazarus");
    if pcp.exists() {
        pcp
    } else if IS_WIN {
        PathBuf::from(env::var_os("APPDATA").unwrap_or_default()).join("lazarus")
    } else {
        PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".lazarus")
    }
}
