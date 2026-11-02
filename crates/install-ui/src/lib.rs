#[cfg(target_os = "linux")]
mod desktop;
mod gui;
mod icons;
mod system_theme;
pub fn launch(product: install_core::Product) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        std::process::exit(install_core::cli::run(product, &args));
    }
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::FreeConsole();
    }
    if let Err(error) = gui::run(product) {
        eprintln!("Erro ao iniciar {}: {error}", product.title());
        std::process::exit(1);
    }
}
