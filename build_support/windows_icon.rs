// Shared by the two executable crates. No SDK or resource compiler is needed
// when building for Linux/macOS.
use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=../../build_support/windows_icon.rs");
    let product = env::var("CARGO_PKG_NAME").unwrap();
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join(format!("../../assets/{product}.ico"));
    println!("cargo:rerun-if-changed={}", icon.display());
    if env::var("CARGO_CFG_TARGET_OS").unwrap() != "windows" {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let resource = output.join("app.rc");
    let icon = icon.canonicalize().expect("application icon");
    // Forward slashes avoid RC escape sequences in Windows paths.
    let icon = icon
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    fs::write(
        &resource,
        format!("#pragma code_page(65001)\n1 ICON \"{icon}\"\n"),
    )
    .expect("writing icon resource");
    let result = output.join("app.res");
    if env::var("CARGO_CFG_TARGET_ENV").unwrap() == "msvc" {
        let mut candidates = Vec::new();
        if let Some(sdk) = env::var_os("WindowsSdkDir") {
            candidates.push(PathBuf::from(sdk).join("bin"));
        }
        if let Some(programs) = env::var_os("ProgramFiles(x86)") {
            candidates.push(PathBuf::from(programs).join("Windows Kits/10/bin"));
        }
        let host_arch = if env::var("HOST").unwrap().starts_with("aarch64") {
            "arm64"
        } else {
            "x64"
        };
        let mut compilers = Vec::new();
        for directory in candidates {
            if let Ok(versions) = fs::read_dir(directory) {
                compilers.extend(
                    versions
                        .flatten()
                        .map(|v| v.path().join(host_arch).join("rc.exe"))
                        .filter(|p| p.is_file()),
                );
            }
        }
        compilers.sort();
        let compiler = compilers.pop().unwrap_or_else(|| PathBuf::from("rc.exe"));
        let status = Command::new(compiler)
            .arg("/nologo")
            .arg("/fo")
            .arg(&result)
            .arg(&resource)
            .status()
            .expect("Windows SDK resource compiler (rc.exe)");
        assert!(status.success(), "failed to embed application icon");
        println!("cargo:rustc-link-arg-bins={}", result.display());
    } else {
        let object = output.join("app-icon.o");
        let target = env::var("TARGET").unwrap();
        let compiler = env::var_os("WINDRES").unwrap_or_else(|| {
            if target.starts_with("x86_64") {
                "x86_64-w64-mingw32-windres".into()
            } else {
                "windres".into()
            }
        });
        let status = Command::new(compiler)
            .arg(&resource)
            .args(["-O", "coff", "-o"])
            .arg(&object)
            .status()
            .expect("windres");
        assert!(status.success(), "failed to embed application icon");
        println!("cargo:rustc-link-arg-bins={}", object.display());
    }
}
