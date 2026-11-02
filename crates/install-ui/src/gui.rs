use eframe::egui;
mod acbr_wizard;
mod log_export;
use install_core::acbr as installer;
use install_core::versions::Component;
use install_core::{
    config::{AcbrAction, AcbrConfig, DelphiOptions, Ide, PascalConfig},
    inspection::{self, AcbrInfo, EnvironmentInfo},
    runtime::Ctx,
    Product,
};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const PRIMARY: egui::Color32 = egui::Color32::from_rgb(37, 99, 235);

#[derive(Clone, Copy)]
struct Palette {
    bg: egui::Color32,
    surface: egui::Color32,
    field: egui::Color32,
    ink: egui::Color32,
    muted: egui::Color32,
    accent: egui::Color32,
    tint: egui::Color32,
    border: egui::Color32,
    error: egui::Color32,
    warning: egui::Color32,
}

impl Palette {
    fn new(dark: bool) -> Self {
        let rgb = egui::Color32::from_rgb;
        if dark {
            Self {
                bg: rgb(15, 23, 36),
                surface: rgb(24, 35, 52),
                field: rgb(18, 28, 44),
                ink: rgb(228, 236, 248),
                muted: rgb(156, 175, 201),
                accent: rgb(127, 176, 255),
                tint: rgb(31, 52, 83),
                border: rgb(48, 65, 88),
                error: rgb(255, 145, 155),
                warning: rgb(241, 190, 101),
            }
        } else {
            Self {
                bg: rgb(243, 247, 254),
                surface: egui::Color32::WHITE,
                field: rgb(248, 250, 254),
                ink: rgb(20, 45, 80),
                muted: rgb(96, 122, 153),
                accent: PRIMARY,
                tint: rgb(237, 244, 255),
                border: rgb(220, 231, 247),
                error: rgb(185, 48, 62),
                warning: rgb(153, 92, 15),
            }
        }
    }

    fn of(ui: &egui::Ui) -> Self {
        Self::new(ui.visuals().dark_mode)
    }
}

fn configure_style(ctx: &egui::Context, dark: bool) {
    // Keep egui on the theme whose custom palette we are configuring. OS
    // changes are handled below, so they cannot select an unstyled palette.
    ctx.set_theme(if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    let colors = Palette::new(dark);
    let mut style = (*ctx.style()).clone();
    style.visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    style.visuals.override_text_color = Some(colors.ink);
    style.visuals.panel_fill = colors.bg;
    style.visuals.window_fill = colors.surface;
    style.visuals.warn_fg_color = colors.warning;
    style.visuals.error_fg_color = colors.error;
    style.visuals.extreme_bg_color = colors.field;
    style.visuals.faint_bg_color = colors.tint;
    style.visuals.selection.bg_fill = colors.tint;
    style.visuals.selection.stroke = egui::Stroke::new(1.5_f32, colors.accent);
    style.visuals.hyperlink_color = colors.accent;
    style.visuals.window_rounding = 12.0.into();
    style.visuals.menu_rounding = 8.0.into();
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.rounding = 6.0.into();
        widget.bg_stroke = egui::Stroke::new(1.0_f32, colors.border);
        widget.fg_stroke = egui::Stroke::new(1.5_f32, colors.ink);
        widget.expansion = 0.0;
    }
    style.visuals.widgets.inactive.bg_fill = colors.field;
    style.visuals.widgets.inactive.weak_bg_fill = colors.tint;
    style.visuals.widgets.hovered.bg_fill = colors.tint;
    style.visuals.widgets.hovered.weak_bg_fill = colors.tint;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, colors.accent);
    style.visuals.widgets.active.bg_fill = colors.tint;
    style.visuals.widgets.active.weak_bg_fill = colors.tint;
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, colors.ink);
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, colors.border);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 9.0);
    style.spacing.interact_size.y = 36.0;
    style.spacing.text_edit_width = 280.0;
    style.spacing.combo_width = 180.0;
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
    ctx.set_style(style);
}

struct AppTheme {
    dark: bool,
    follows_system: bool,
    system: crate::system_theme::SystemTheme,
}

impl AppTheme {
    fn new(ctx: &egui::Context) -> Self {
        Self::with_system(ctx, crate::system_theme::SystemTheme::new(ctx))
    }

    fn with_system(ctx: &egui::Context, mut system: crate::system_theme::SystemTheme) -> Self {
        let dark = system.current().or(ctx.system_theme()) == Some(egui::Theme::Dark);
        configure_style(ctx, dark);
        Self {
            dark,
            follows_system: true,
            system,
        }
    }

    fn sync(&mut self, ctx: &egui::Context) {
        if self.follows_system {
            let dark = self.system.current().or(ctx.system_theme()) == Some(egui::Theme::Dark);
            if self.dark != dark {
                self.dark = dark;
                configure_style(ctx, dark);
            }
        }
    }

    fn toggle(&mut self, ctx: &egui::Context) {
        self.follows_system = false;
        self.dark = !self.dark;
        configure_style(ctx, self.dark);
    }

    fn follow_system(&mut self, ctx: &egui::Context) {
        self.follows_system = true;
        self.sync(ctx);
    }
}

fn paint_theme_icon(ui: &egui::Ui, rect: egui::Rect, dark: bool) {
    let center = rect.center();
    let stroke = egui::Stroke::new(1.8_f32, egui::Color32::WHITE);
    let painter = ui.painter();
    if dark {
        painter.circle_stroke(center, 3.5, stroke);
        for ray in 0..8 {
            let angle = ray as f32 * std::f32::consts::TAU / 8.0;
            let direction = egui::vec2(angle.cos(), angle.sin());
            painter.line_segment([center + direction * 6.0, center + direction * 8.5], stroke);
        }
    } else {
        let quarter = std::f32::consts::FRAC_PI_2;
        let mut outline = Vec::with_capacity(34);
        for step in 0..=24 {
            let angle = -quarter - step as f32 / 24.0 * 3.0 * quarter;
            outline.push(center + egui::vec2(angle.cos(), angle.sin()) * 8.0);
        }
        let cutout = center + egui::vec2(8.0, -8.0);
        for step in 0..=8 {
            let angle = quarter + step as f32 / 8.0 * quarter;
            outline.push(cutout + egui::vec2(angle.cos(), angle.sin()) * 8.0);
        }
        painter.add(egui::epaint::PathShape::closed_line(outline, stroke));
    }
}

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    let colors = Palette::of(ui);
    egui::Frame::none()
        .fill(colors.surface)
        .stroke(egui::Stroke::new(1.0_f32, colors.border))
        .rounding(12.0)
        .inner_margin(18.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        });
}

fn section_heading(ui: &mut egui::Ui, number: &str, title: &str, subtitle: &str) {
    let colors = Palette::of(ui);
    ui.horizontal(|ui| {
        egui::Frame::none()
            .fill(colors.tint)
            .rounding(7.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(number).color(colors.accent).strong());
            });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.label(egui::RichText::new(title).size(17.0).strong());
            ui.label(egui::RichText::new(subtitle).size(12.0).color(colors.muted));
        });
    });
    ui.add_space(6.0);
}

fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(13.0).strong());
}

enum Msg {
    SourcesReady(Result<(), String>),
    Line(String),
    Done(Result<(), String>),
}

#[derive(Default)]
struct FolderPicker {
    rx: Option<Receiver<Option<PathBuf>>>,
}

impl FolderPicker {
    fn poll(&mut self, value: &mut String) -> bool {
        let Some(rx) = &self.rx else { return false };
        match rx.try_recv() {
            Ok(selection) => {
                let selected = selection.is_some();
                if let Some(path) = selection {
                    *value = path.to_string_lossy().into_owned();
                }
                self.rx = None;
                return selected;
            }
            Err(mpsc::TryRecvError::Disconnected) => self.rx = None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        false
    }

    fn show(&mut self, ui: &mut egui::Ui, value: &mut String, hint: &str, title: &str) {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 100.0 - ui.spacing().item_spacing.x).max(60.0);
            ui.add_sized(
                [width, 36.0],
                egui::TextEdit::singleline(value)
                    .margin(egui::vec2(10.0, 8.0))
                    .hint_text(hint),
            );
            if ui
                .add_sized([100.0, 36.0], egui::Button::new("Procurar…"))
                .on_hover_text("Selecionar uma pasta no computador")
                .clicked()
            {
                let path = PathBuf::from(if value.trim().is_empty() {
                    hint
                } else {
                    value.trim()
                });
                let initial_dir = path.ancestors().find(|p| p.is_dir()).map(PathBuf::from);
                let title = title.to_string();
                let ctx = ui.ctx().clone();
                let (tx, rx) = mpsc::channel();
                self.rx = Some(rx);
                std::thread::spawn(move || {
                    let mut dialog = rfd::FileDialog::new().set_title(title);
                    if let Some(dir) = initial_dir {
                        dialog = dialog.set_directory(dir);
                    }
                    let _ = tx.send(dialog.pick_folder());
                    ctx.request_repaint();
                });
            }
        });
    }
}

struct VersionList {
    component: Component,
    values: Vec<String>,
    error: Option<String>,
    rx: Option<Receiver<Result<Vec<String>, String>>>,
}

struct InspectionResult {
    environment: Result<EnvironmentInfo, String>,
    acbr: Result<AcbrInfo, String>,
    catalog: Result<install_core::packages::Catalog, String>,
}

#[derive(Default)]
struct FolderInspection {
    paths: Option<(PathBuf, PathBuf)>,
    applied_paths: Option<(PathBuf, PathBuf)>,
    due: Option<Instant>,
    rx: Option<Receiver<InspectionResult>>,
    result: Option<InspectionResult>,
    force_environment: bool,
    force_acbr: bool,
}

impl FolderInspection {
    fn invalidate(&mut self, environment: bool, acbr: bool) {
        self.paths = None;
        self.force_environment |= environment;
        self.force_acbr |= acbr;
    }

    fn poll(
        &mut self,
        base: PathBuf,
        acbr: PathBuf,
        ctx: &egui::Context,
        product: Product,
        ide: Ide,
    ) -> Option<(bool, bool)> {
        let paths = (base, acbr);
        if self.paths.as_ref() != Some(&paths) {
            self.paths = Some(paths.clone());
            self.result = None;
            self.rx = None; // Discard results for a previously selected folder.
            self.due = Some(Instant::now() + Duration::from_millis(350));
        }
        if let Some(due) = self.due {
            if Instant::now() < due {
                ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
            } else {
                let (tx, rx) = mpsc::channel();
                self.rx = Some(rx);
                self.due = None;
                let (base, acbr) = paths.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let result = InspectionResult {
                        environment: if product == Product::Acbr && ide == Ide::Delphi {
                            Err("Selecione a instalação Delphi.".into())
                        } else if base.as_os_str().is_empty() {
                            Err("Informe a pasta de instalação.".into())
                        } else {
                            inspection::environment(&base).map_err(|e| format!("{e:#}"))
                        },
                        acbr: if product == Product::Acbr {
                            inspection::acbr_for(&acbr, ide).map_err(|e| format!("{e:#}"))
                        } else {
                            Err(String::new())
                        },
                        catalog: if product == Product::Acbr {
                            install_core::packages::Catalog::load_for(&acbr, ide)
                                .map_err(|e| format!("{e:#}"))
                        } else {
                            Err(String::new())
                        },
                    };
                    let _ = tx.send(result);
                    ctx.request_repaint();
                });
            }
        }
        if let Some(rx) = &self.rx {
            if let Ok(result) = rx.try_recv() {
                let changed = self.applied_paths.as_ref().map_or((true, true), |old| {
                    (
                        old.0 != paths.0 || self.force_environment,
                        old.1 != paths.1 || self.force_acbr,
                    )
                });
                self.force_environment = false;
                self.force_acbr = false;
                self.result = Some(result);
                self.applied_paths = Some(paths);
                self.rx = None;
                return Some(changed);
            }
        }
        None
    }

    fn pending(&self) -> bool {
        self.due.is_some() || self.rx.is_some()
    }
}

impl VersionList {
    fn new(component: Component, ctx: &egui::Context, enabled: bool) -> Self {
        let mut list = Self {
            component,
            values: vec!["stable".into()],
            error: None,
            rx: None,
        };
        if enabled {
            list.load(ctx);
        }
        list
    }

    fn load(&mut self, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.error = None;
        let component = self.component;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = component.releases().map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn poll(&mut self) {
        if let Some(rx) = &self.rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(values) => self.values = values,
                    Err(error) => self.error = Some(error),
                }
                self.rx = None;
            }
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, id: &str, selected: &mut String) {
        let colors = Palette::of(ui);
        ui.vertical(|ui| {
            egui::ComboBox::from_id_salt(id)
                .selected_text(selected.as_str())
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for version in &self.values {
                        ui.selectable_value(selected, version.clone(), version);
                    }
                });
            if self.rx.is_some() {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(12.0).color(colors.accent));
                    ui.label(
                        egui::RichText::new("Buscando releases...")
                            .small()
                            .color(colors.muted),
                    );
                });
            } else if let Some(error) = self.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Tags indisponíveis")
                            .small()
                            .color(colors.muted),
                    )
                    .on_hover_text(error);
                    if ui.small_button("Tentar novamente").clicked() {
                        self.load(ui.ctx());
                    }
                });
            }
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Installation,
    Activity,
}

struct App {
    product: Product,
    logo: egui::TextureHandle,
    ide: Ide,
    delphi: DelphiOptions,
    delphi_dir: String,
    detected_delphi: Vec<install_core::delphi::Installation>,
    delphi_rx: Option<Receiver<Result<Vec<install_core::delphi::Installation>, String>>>,
    detected_lazarus: Vec<PathBuf>,
    page: Page,
    acbr_step: acbr_wizard::Step,
    theme: AppTheme,
    about_open: bool,
    dir: String,
    fpc: String,
    laz: String,
    fpc_versions: VersionList,
    laz_versions: VersionList,
    cross: Vec<(&'static str, &'static str, bool)>, // (id, rótulo, marcado)
    acbr_dir: String,
    dir_picker: FolderPicker,
    acbr_picker: FolderPicker,
    fastreport_dir: String,
    fastreport_picker: FolderPicker,
    all_components: bool,
    inspection: FolderInspection,
    acbr_action: AcbrAction,
    acbr_packages: Option<Vec<String>>,
    package_filter: String,
    package_family: Option<&'static str>,
    install_attempted: bool,
    log: Vec<String>,
    log_path: Option<PathBuf>,
    log_export: log_export::LogExport,
    status: String,
    running: bool,
    rx: Option<Receiver<Msg>>,
    ctx_run: Option<Ctx>,
    phase: String,
    started_at: Option<Instant>,
    finished_after: Option<Duration>,
    succeeded: Option<bool>,
    cancelling: bool,
}

impl App {
    fn new(ctx: &egui::Context, product: Product) -> Self {
        let theme = AppTheme::new(ctx);
        let mut acbr = AcbrConfig::default();
        let mut pascal = PascalConfig::default();
        let (restored, warning) = match product {
            Product::Acbr => match install_core::config_store::load_acbr() {
                Ok(Some(cfg)) => {
                    acbr = cfg;
                    (true, String::new())
                }
                Ok(None) => (false, String::new()),
                Err(e) => (false, format!("AVISO: configuração ACBr inválida: {e:#}")),
            },
            Product::Pascal => match install_core::config_store::load_pascal() {
                Ok(Some(cfg)) => {
                    pascal = cfg;
                    (true, String::new())
                }
                Ok(None) => (false, String::new()),
                Err(e) => (false, format!("AVISO: configuração Pascal inválida: {e:#}")),
            },
        };
        if !cfg!(windows) {
            acbr.ide = Ide::Lazarus;
        }
        let base = if product == Product::Acbr {
            acbr.base.clone()
        } else {
            pascal.base.clone()
        };
        let mut inspection = FolderInspection::default();
        if restored {
            inspection.applied_paths = Some((
                base.clone(),
                acbr.acbr_dir.clone().unwrap_or_else(|| base.join("ACBr")),
            ));
        }
        let labels = [
            ("win32", "Windows 32-bit"),
            ("win64", "Windows 64-bit"),
            ("linux32", "Linux 32-bit"),
            ("linux64", "Linux 64-bit"),
        ];
        let delphi_rx = if product == Product::Acbr && cfg!(windows) {
            let (tx, rx) = mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(install_core::delphi::discover().map_err(|e| format!("{e:#}")));
                ctx.request_repaint();
            });
            Some(rx)
        } else {
            None
        };
        let icon = crate::icons::window_icon(product);
        let logo = ctx.load_texture(
            "application-icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            ),
            egui::TextureOptions::LINEAR,
        );
        App {
            product,
            logo,
            ide: acbr.ide,
            delphi_dir: acbr.delphi.root.to_string_lossy().into_owned(),
            delphi: acbr.delphi,
            detected_delphi: Vec::new(),
            delphi_rx,
            detected_lazarus: inspection::discover_lazarus(),
            page: Page::Installation,
            acbr_step: acbr_wizard::Step::Welcome,
            theme,
            about_open: false,
            dir: base.to_string_lossy().into_owned(),
            fpc: pascal.fpc,
            laz: pascal.laz,
            fpc_versions: VersionList::new(Component::Fpc, ctx, product == Product::Pascal),
            laz_versions: VersionList::new(Component::Lazarus, ctx, product == Product::Pascal),
            cross: labels
                .iter()
                .map(|(id, label)| (*id, *label, pascal.cross.iter().any(|p| p == id)))
                .collect(),
            acbr_dir: acbr
                .acbr_dir
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            dir_picker: FolderPicker::default(),
            acbr_picker: FolderPicker::default(),
            fastreport_dir: acbr
                .fastreport_dir
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            fastreport_picker: FolderPicker::default(),
            all_components: acbr.all_components,
            inspection,
            acbr_action: acbr.acbr_action,
            acbr_packages: acbr.acbr_packages,
            package_filter: String::new(),
            package_family: None,
            install_attempted: false,
            log: if warning.is_empty() {
                Vec::new()
            } else {
                vec![warning.clone()]
            },
            status: warning,
            log_path: None,
            log_export: log_export::LogExport::default(),
            running: false,
            rx: None,
            ctx_run: None,
            phase: String::new(),
            started_at: None,
            finished_after: None,
            succeeded: None,
            cancelling: false,
        }
    }

    fn start(&mut self, egui_ctx: egui::Context) {
        self.install_attempted = true;
        self.log.clear();
        self.status = "Instalando...".into();
        self.running = true;
        self.page = Page::Activity;
        self.acbr_step = acbr_wizard::Step::Installation;
        self.phase = "Preparando instalação".into();
        self.started_at = Some(Instant::now());
        self.finished_after = None;
        self.succeeded = None;
        self.cancelling = false;

        let (tx, rx) = mpsc::channel::<Msg>();
        self.rx = Some(rx);

        let tx_log = Mutex::new(tx.clone());
        let c2 = egui_ctx.clone();
        let ctx = Ctx::new(Arc::new(move |l: String| {
            let _ = tx_log.lock().unwrap().send(Msg::Line(l));
            c2.request_repaint();
        }))
        .with_session_log(self.product);
        self.log_path = ctx.log_path().map(PathBuf::from);
        self.ctx_run = Some(ctx.clone());

        let mut delphi = self.delphi.clone();
        delphi.root = PathBuf::from(self.delphi_dir.trim());
        let acbr = AcbrConfig {
            base: PathBuf::from(self.dir.trim()),
            ide: self.ide,
            delphi,
            acbr_dir: (!self.acbr_dir.trim().is_empty())
                .then(|| PathBuf::from(self.acbr_dir.trim())),
            acbr_action: self.acbr_action,
            acbr_packages: self.acbr_packages.clone(),
            all_components: self.all_components,
            fastreport_dir: (!self.fastreport_dir.trim().is_empty())
                .then(|| PathBuf::from(self.fastreport_dir.trim())),
        };
        let pascal = PascalConfig {
            base: PathBuf::from(self.dir.trim()),
            fpc: self.fpc.trim().into(),
            laz: self.laz.trim().into(),
            cross: self
                .cross
                .iter()
                .filter(|c| c.2)
                .map(|c| c.0.to_string())
                .collect(),
        };
        let product = self.product;
        std::thread::spawn(move || {
            let r = match product {
                Product::Acbr => installer::executar(&acbr, &ctx),
                Product::Pascal => install_core::pascal::executar(&pascal, &ctx),
            }
            .map_err(|e| format!("{e:#}"));
            if let Err(error) = &r {
                ctx.say(format!("ERRO: {error}"));
            }
            let _ = tx.send(Msg::Done(r));
            egui_ctx.request_repaint();
        });
    }

    fn download_sources(&mut self, egui_ctx: egui::Context) {
        self.install_attempted = false;
        self.log.clear();
        self.status = "Baixando fontes do ACBr...".into();
        self.running = true;
        self.page = Page::Activity;
        self.acbr_step = acbr_wizard::Step::Installation;
        self.phase = "Baixando fontes para selecionar componentes".into();
        self.started_at = Some(Instant::now());
        self.finished_after = None;
        self.succeeded = None;
        self.cancelling = false;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let tx_log = Mutex::new(tx.clone());
        let c2 = egui_ctx.clone();
        let ctx = Ctx::new(Arc::new(move |line| {
            let _ = tx_log.lock().unwrap().send(Msg::Line(line));
            c2.request_repaint();
        }))
        .with_session_log(self.product);
        self.log_path = ctx.log_path().map(PathBuf::from);
        self.ctx_run = Some(ctx.clone());
        let base = PathBuf::from(self.dir.trim());
        let destino = if self.acbr_dir.trim().is_empty() {
            base.join("ACBr")
        } else {
            PathBuf::from(self.acbr_dir.trim())
        };
        std::thread::spawn(move || {
            let result =
                installer::baixar_fontes_acbr(&ctx, &base, &destino).map_err(|e| format!("{e:#}"));
            if result.is_ok() {
                ctx.say("Fontes disponíveis. Selecione os componentes para instalar.");
            }
            if let Err(error) = &result {
                ctx.say(format!("ERRO: {error}"));
            }
            let _ = tx.send(Msg::SourcesReady(result));
            egui_ctx.request_repaint();
        });
    }

    fn drenar(&mut self) {
        self.log_export.poll();
        if let Some(rx) = &self.delphi_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(installations) => {
                        if self.delphi_dir.is_empty() {
                            if let Some(first) = installations.first() {
                                self.delphi_dir = first.root.to_string_lossy().into_owned();
                                self.delphi.registry_key = first.registry_key.clone();
                            }
                        }
                        self.detected_delphi = installations;
                    }
                    Err(e) => self.log.push(format!("AVISO: {e}")),
                }
                self.delphi_rx = None;
            }
        }
        let selected_base = if self.product == Product::Acbr && self.ide == Ide::Delphi {
            self.dir_picker.poll(&mut self.delphi_dir)
        } else {
            self.dir_picker.poll(&mut self.dir)
        };
        let selected_acbr = self.acbr_picker.poll(&mut self.acbr_dir);
        self.fastreport_picker.poll(&mut self.fastreport_dir);
        if selected_base || selected_acbr {
            self.inspection.invalidate(
                selected_base,
                selected_acbr || (selected_base && self.acbr_dir.trim().is_empty()),
            );
        }
        self.fpc_versions.poll();
        self.laz_versions.poll();
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                match m {
                    Msg::Line(l) => {
                        if let Some(phase) = l.strip_prefix("=== ") {
                            self.phase = phase.to_string();
                        }
                        self.log.push(l);
                    }
                    Msg::Done(Ok(())) => {
                        self.running = false;
                        self.status = "Instalação concluída.".into();
                        self.succeeded = Some(true);
                        self.acbr_step = acbr_wizard::Step::Finish;
                        self.finished_after = self.started_at.map(|start| start.elapsed());
                    }
                    Msg::SourcesReady(Ok(())) => {
                        self.running = false;
                        self.status =
                            "Fontes disponíveis. Selecione os componentes para instalar.".into();
                        self.phase = "Fontes disponíveis".into();
                        self.page = Page::Installation;
                        self.acbr_step = acbr_wizard::Step::Components;
                        self.finished_after = self.started_at.map(|start| start.elapsed());
                        self.inspection.invalidate(false, true);
                    }
                    Msg::Done(Err(_)) | Msg::SourcesReady(Err(_)) => {
                        self.running = false;
                        self.status = if self.cancelling {
                            "Instalação cancelada."
                        } else {
                            "Falha na instalação. Consulte o log."
                        }
                        .into();
                        self.succeeded = Some(false);
                        self.finished_after = self.started_at.map(|start| start.elapsed());
                    }
                }
            }
        }
        if self.log.len() > 20_000 {
            self.log.drain(..5_000);
        }
    }
}

impl App {
    fn inspect_folders(&mut self, ctx: &egui::Context) {
        if self.running || self.picking_folder() {
            return;
        }
        let base = if self.product == Product::Acbr && self.ide == Ide::Delphi {
            PathBuf::from(self.delphi_dir.trim())
        } else {
            PathBuf::from(self.dir.trim())
        };
        let acbr = if self.acbr_dir.trim().is_empty() {
            PathBuf::from(self.dir.trim()).join("ACBr")
        } else {
            PathBuf::from(self.acbr_dir.trim())
        };
        if let Some((_base_changed, acbr_changed)) =
            self.inspection
                .poll(base, acbr, ctx, self.product, self.ide)
        {
            if let Some(result) = &self.inspection.result {
                if acbr_changed {
                    if let Ok(info) = &result.acbr {
                        if let Some(action) = info.suggested_action() {
                            self.acbr_action = action;
                        }
                    }
                }
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let white = egui::Color32::WHITE;
        let soft = egui::Color32::from_rgb(182, 206, 240);
        let selected = egui::Color32::from_rgb(36, 80, 140);
        ui.visuals_mut().override_text_color = Some(white);
        ui.visuals_mut().widgets.active.fg_stroke.color = white;
        ui.visuals_mut().widgets.hovered.weak_bg_fill = selected;
        ui.visuals_mut().widgets.hovered.bg_fill = selected;
        ui.visuals_mut().widgets.hovered.bg_stroke = egui::Stroke::NONE;
        ui.visuals_mut().widgets.hovered.fg_stroke.color = white;
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(&self.logo).fit_to_exact_size(egui::vec2(40.0, 40.0)));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(self.product.title())
                            .size(22.0)
                            .color(white)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(format!("v{}", install_core::VERSION))
                            .small()
                            .color(soft),
                    );
                });
                ui.label(
                    egui::RichText::new(self.product.subtitle())
                        .size(12.0)
                        .color(soft),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.theme.dark {
                    "Tema claro"
                } else {
                    "Tema escuro"
                };
                let response = ui
                    .add(
                        egui::Button::new("")
                            .min_size(egui::vec2(36.0, 36.0))
                            .fill(selected)
                            .stroke(egui::Stroke::NONE),
                    )
                    .on_hover_text(format!("{label}. {}", if self.theme.follows_system {
                        "Seguindo o tema do sistema. Clique para trocar manualmente."
                    } else {
                        "Tema manual. Clique com o botão direito para voltar ao tema do sistema."
                    }));
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
                });
                paint_theme_icon(ui, response.rect, self.theme.dark);
                if response.clicked() {
                    self.theme.toggle(ui.ctx());
                    ui.ctx().request_repaint();
                }
                response.context_menu(|ui| {
                    if ui
                        .selectable_label(self.theme.follows_system, "Seguir tema do sistema")
                        .clicked()
                    {
                        self.theme.follow_system(ui.ctx());
                        ui.ctx().request_repaint();
                        ui.close_menu();
                    }
                });
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Sobre").color(white))
                            .fill(selected)
                            .stroke(egui::Stroke::NONE),
                    )
                    .clicked()
                {
                    self.about_open = true;
                }
                ui.label(
                    egui::RichText::new(
                        format!("{} / {}", std::env::consts::OS, std::env::consts::ARCH)
                            .to_uppercase(),
                    )
                    .size(10.0)
                    .color(soft),
                );
            });
        });
        if self.product == Product::Acbr {
            return;
        }
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            for (page, title) in [
                (Page::Installation, "Instalação"),
                (Page::Activity, "Acompanhamento"),
            ] {
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new(title).color(if self.page == page {
                            white
                        } else {
                            soft
                        }))
                        .fill(if self.page == page {
                            selected
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .stroke(egui::Stroke::NONE)
                        .min_size(egui::vec2(120.0, 34.0)),
                    )
                    .clicked()
                {
                    self.page = page;
                }
            }
        });
    }

    fn configuration(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(!self.running && !self.picking_folder(), |ui| {
            self.environment_card(ui);
            ui.add_space(4.0);
            self.compilers_card(ui);
        });
    }

    fn environment_card(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::of(ui);
        card(ui, |ui| {
            section_heading(
                ui,
                if self.product == Product::Acbr {
                    "02"
                } else {
                    "01"
                },
                "Ambiente de instalação",
                "Escolha o destino e o que deseja preparar.",
            );
            if self.product == Product::Acbr {
                let previous = self.ide;
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.ide, Ide::Lazarus, "Lazarus");
                    ui.add_enabled_ui(cfg!(windows), |ui| {
                        ui.selectable_value(&mut self.ide, Ide::Delphi, "Delphi");
                    });
                });
                if previous != self.ide {
                    self.acbr_packages = None;
                    self.all_components = false;
                    self.package_family = None;
                    self.inspection.invalidate(true, true);
                }
                if !cfg!(windows) {
                    ui.label(
                        egui::RichText::new("Delphi está disponível somente no Windows.")
                            .small()
                            .color(colors.muted),
                    );
                }
                if self.ide == Ide::Delphi {
                    egui::ComboBox::from_id_salt("delphi_version")
                        .selected_text(if self.delphi.registry_key.is_empty() {
                            "Selecionar Delphi"
                        } else {
                            &self.delphi.registry_key
                        })
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for installation in &self.detected_delphi {
                                if ui
                                    .selectable_label(
                                        self.delphi.registry_key == installation.registry_key,
                                        format!("Delphi / BDS {}", installation.version),
                                    )
                                    .clicked()
                                {
                                    self.delphi.registry_key = installation.registry_key.clone();
                                    self.delphi_dir =
                                        installation.root.to_string_lossy().into_owned();
                                }
                            }
                        });
                    field_label(ui, "Pasta do Delphi");
                    self.dir_picker.show(
                        ui,
                        &mut self.delphi_dir,
                        "",
                        "Selecionar instalação Delphi",
                    );
                    ui.label(egui::RichText::new("O Delphi deve estar instalado e fechado durante a instalação dos componentes.").small().color(colors.muted));
                    ui.label(
                        egui::RichText::new(
                            "Relatórios Fortes exigem FortesReport-CE já instalado no Delphi.",
                        )
                        .small()
                        .color(colors.muted),
                    );
                    return;
                }
                if !self.detected_lazarus.is_empty() {
                    egui::ComboBox::from_id_salt("lazarus_detected")
                        .selected_text("Instalações Lazarus detectadas")
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for path in &self.detected_lazarus {
                                if ui
                                    .selectable_label(false, path.display().to_string())
                                    .clicked()
                                {
                                    self.dir = path.to_string_lossy().into_owned();
                                }
                            }
                        });
                }
                ui.label(egui::RichText::new("Selecione um ambiente existente. Para instalar Lazarus, use PascalInstall.").small().color(colors.muted));
            }
            field_label(
                ui,
                if self.product == Product::Pascal {
                    "Destino do ambiente Pascal"
                } else {
                    "Pasta do Lazarus"
                },
            );
            self.dir_picker
                .show(ui, &mut self.dir, "", "Selecionar pasta do ambiente");
            let (message, color) = match self.inspection.result.as_ref().map(|r| &r.environment) {
                Some(Ok(info)) => (
                    info.summary(),
                    if info.complete() {
                        colors.accent
                    } else if info.partial {
                        colors.warning
                    } else {
                        colors.muted
                    },
                ),
                Some(Err(error)) => (error.as_str(), colors.error),
                None if self.inspection.pending() => {
                    ("Verificando a pasta de instalação...", colors.muted)
                }
                None => (
                    if self.product == Product::Acbr {
                        "Informe o ambiente existente que contém lazbuild."
                    } else {
                        "FPC, Lazarus e os compiladores serão instalados nesta pasta."
                    },
                    colors.muted,
                ),
            };
            ui.label(egui::RichText::new(message).small().color(color));
        });
    }

    fn compilers_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.set_min_height(260.0);
            section_heading(
                ui,
                "02",
                "Versões e compiladores",
                "Releases oficiais do FPC e Lazarus.",
            );
            ui.add_enabled_ui(true, |ui| {
                ui.columns(2, |columns| {
                    field_label(&mut columns[0], "Free Pascal");
                    self.fpc_versions
                        .show(&mut columns[0], "fpc_version", &mut self.fpc);
                    field_label(&mut columns[1], "Lazarus IDE");
                    self.laz_versions
                        .show(&mut columns[1], "laz_version", &mut self.laz);
                });
                ui.add_space(2.0);
                field_label(ui, "Compilar também para");
                egui::Grid::new("cross_targets")
                    .num_columns(2)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        ui.spacing_mut().interact_size.y = 24.0;
                        for (i, (_, label, enabled)) in self.cross.iter_mut().enumerate() {
                            ui.checkbox(enabled, *label);
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
            });
        });
    }

    fn acbr_card(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::of(ui);
        card(ui, |ui| {
            section_heading(
                ui,
                "03",
                "Fontes do ACBr",
                "Fontes e pacotes para a IDE selecionada.",
            );
            ui.add_enabled_ui(true, |ui| {
                field_label(ui, "O que deseja fazer?");
                egui::ComboBox::from_id_salt("acbr_action")
                    .width(ui.available_width())
                    .selected_text(self.acbr_action.label())
                    .show_ui(ui, |ui| {
                        for action in [
                            AcbrAction::Install,
                            AcbrAction::DownloadAndInstall,
                            AcbrAction::UpdateAndInstall,
                        ] {
                            ui.selectable_value(&mut self.acbr_action, action, action.label());
                        }
                    });
                ui.label(
                    egui::RichText::new(match self.acbr_action {
                        AcbrAction::Install => "Usa os fontes locais, sem baixar ou atualizar.",
                        AcbrAction::DownloadAndInstall => {
                            "Baixa via SVN para uma pasta nova ou vazia."
                        }
                        AcbrAction::UpdateAndInstall => {
                            "Atualiza a cópia SVN existente antes de instalar."
                        }
                    })
                    .small()
                    .color(colors.muted),
                );
                field_label(ui, "Pasta dos fontes do ACBr");
                let default_dir = PathBuf::from(self.dir.trim()).join("ACBr");
                self.acbr_picker.show(
                    ui,
                    &mut self.acbr_dir,
                    &default_dir.to_string_lossy(),
                    "Selecionar pasta dos fontes do ACBr",
                );
                let (message, color) = match self.inspection.result.as_ref().map(|r| &r.acbr) {
                    Some(Ok(info)) => (
                        info.summary(),
                        if info.sources {
                            colors.accent
                        } else if info.occupied {
                            colors.warning
                        } else {
                            colors.muted
                        },
                    ),
                    Some(Err(error)) => (error.as_str(), colors.error),
                    None if self.inspection.pending() => {
                        ("Verificando os fontes do ACBr...", colors.muted)
                    }
                    None => (
                        "Pasta que contém Pacotes/Lazarus. Em branco, usa o destino acima.",
                        colors.muted,
                    ),
                };
                ui.label(egui::RichText::new(message).small().color(color));
                egui::CollapsingHeader::new("FastReport (opcional)").show(ui,|ui|{
                    ui.label("Utiliza o FastReport já instalado na IDE. O produto pago não é baixado pelo instalador.");
                    field_label(ui,if self.ide==Ide::Lazarus {"Fontes FastReport (.lpk), se ainda não registrados"}else{"Bibliotecas FastReport (.dcp/.bpl), se necessárias"});
                    self.fastreport_picker.show(ui,&mut self.fastreport_dir,"","Selecionar FastReport adquirido");
                });
            });
        });
    }

    fn packages_card(&mut self, ui: &mut egui::Ui, list_height: f32) {
        let colors = Palette::of(ui);
        card(ui, |ui| {
            section_heading(ui, "04", "Pacotes disponíveis", "Marque os componentes que deseja instalar. As dependências são incluídas automaticamente.");
            ui.label(
                egui::RichText::new(
                    "FastReport é opcional: marque suas integrações apenas se já possui o produto e suas dependências.",
                )
                .small()
                .color(colors.muted),
            );
            let Some(result) = &self.inspection.result else {
                ui.label("Aguardando a verificação da pasta dos fontes...");
                return;
            };
            let catalog = match &result.catalog {
                Ok(catalog) => catalog,
                Err(error) => {
                    ui.label(
                        "Selecione os fontes locais do ACBr para carregar a lista de componentes.",
                    );
                    ui.label(egui::RichText::new("Em uma pasta nova, baixe os fontes primeiro para escolher os componentes antes de compilar.").small().color(colors.muted)).on_hover_text(error);
                    let can_download = self.acbr_action == AcbrAction::DownloadAndInstall
                        && !self.dir.trim().is_empty()
                        && result.acbr.as_ref().is_ok_and(|info| !info.occupied);
                    if ui
                        .add_enabled(
                            can_download,
                            egui::Button::new("Baixar fontes para selecionar componentes"),
                        )
                        .clicked()
                    {
                        self.download_sources(ui.ctx().clone());
                    }
                    return;
                }
            };
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("Seleção padrão").clicked() {
                    self.acbr_packages = None;
                    self.all_components = false;
                }
                if ui.small_button("Marcar todos").clicked() {
                    self.acbr_packages = None;
                    self.all_components = true;
                }
                if ui.small_button("Desmarcar todos").clicked() {
                    self.acbr_packages = Some(Vec::new());
                    self.all_components = false;
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.package_filter)
                        .hint_text("Buscar componente...")
                        .desired_width(180.0),
                );
                egui::ComboBox::from_id_salt("package_family")
                    .selected_text(self.package_family.unwrap_or("Todas as famílias"))
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.package_family, None, "Todas as famílias");
                        for &family in &acbr_wizard::FAMILIES {
                            if catalog
                                .items
                                .iter()
                                .any(|p| acbr_wizard::package_family(p) == family)
                            {
                                ui.selectable_value(&mut self.package_family, Some(family), family);
                            }
                        }
                    });
            });
            let plan =
                match catalog.resolve_with(self.acbr_packages.as_deref(), self.all_components) {
                    Ok(plan) => plan,
                    Err(error) => {
                        ui.colored_label(colors.error, error.to_string());
                        return;
                    }
                };
            ui.label(
                egui::RichText::new(format!(
                    "{} de {} pacotes selecionados, incluindo dependências",
                    plan.len(),
                    catalog.items.len()
                ))
                .small()
                .color(colors.muted),
            );
            if plan.is_empty() {
                ui.colored_label(
                    colors.warning,
                    "Selecione ao menos um componente para instalar o ACBr.",
                );
            }
            let required = catalog.required(&plan);
            let filter = self.package_filter.trim().to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("acbr_package_list")
                .min_scrolled_height(240.0)
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().interact_size.y = 24.0;
                    ui.spacing_mut().item_spacing.y = 4.0;
                    let mut shown = 0;
                    for &family in &acbr_wizard::FAMILIES {
                        if self
                            .package_family
                            .is_some_and(|selected| selected != family)
                        {
                            continue;
                        }
                        let indices: Vec<usize> = catalog
                            .items
                            .iter()
                            .enumerate()
                            .filter(|(_, package)| {
                                acbr_wizard::package_family(package) == family
                                    && (filter.is_empty()
                                        || format!(
                                            "{} {} {} {}",
                                            package.name,
                                            package.description,
                                            family,
                                            if package.requires_fastreport {
                                                "FastReport pago"
                                            } else {
                                                ""
                                            }
                                        )
                                        .to_lowercase()
                                        .contains(&filter))
                            })
                            .map(|(i, _)| i)
                            .collect();
                        if indices.is_empty() {
                            continue;
                        }
                        shown += indices.len();
                        let selected = indices.iter().filter(|i| plan.contains(i)).count();
                        egui::CollapsingHeader::new(format!(
                            "{family}  ·  {selected}/{}",
                            indices.len()
                        ))
                        .id_salt(("component_family", family))
                        .default_open(true)
                        .open((!filter.is_empty() || self.package_family.is_some()).then_some(true))
                        .show(ui, |ui| {
                            for i in indices {
                                let package = &catalog.items[i];
                                let mut checked = plan.contains(&i);
                                let locked = required.contains(&package.name.to_ascii_lowercase());
                                ui.horizontal_wrapped(|ui| {
                                    let response = ui.add_enabled(
                                        !locked,
                                        egui::Checkbox::new(&mut checked, &package.name),
                                    );
                                    if response.changed() {
                                        let selected =
                                            self.acbr_packages.get_or_insert_with(|| {
                                                if self.all_components {
                                                    catalog.all()
                                                } else {
                                                    catalog.defaults()
                                                }
                                            });
                                        selected.retain(|n| !n.eq_ignore_ascii_case(&package.name));
                                        if checked {
                                            selected.push(package.name.clone());
                                        }
                                        self.all_components = false;
                                    }
                                    response.on_hover_text(format!(
                                        "{}\n{}",
                                        package.description,
                                        package.path.display()
                                    ));
                                    if package.requires_fastreport {
                                        ui.label(
                                            egui::RichText::new("FastReport — requer licença")
                                                .small()
                                                .color(colors.muted),
                                        );
                                    } else if locked {
                                        ui.label(
                                            egui::RichText::new("Dependência necessária")
                                                .small()
                                                .color(colors.muted),
                                        );
                                    }
                                });
                                let description = package
                                    .description
                                    .split(" - (http")
                                    .next()
                                    .unwrap_or_default()
                                    .trim()
                                    .trim_start_matches("ACBr - ");
                                if !description.is_empty() {
                                    ui.label(
                                        egui::RichText::new(description)
                                            .small()
                                            .color(colors.muted),
                                    );
                                }
                                ui.add_space(5.0);
                            }
                        });
                        ui.add_space(6.0);
                    }
                    if shown == 0 {
                        ui.label("Nenhum componente corresponde à busca.");
                    }
                });
        });
    }

    fn activity(&mut self, ui: &mut egui::Ui, height: f32) {
        let colors = Palette::of(ui);
        let height = height.max(340.0);
        card(ui, |ui| {
            ui.set_min_height((height - 36.0).max(300.0));
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Acompanhe a instalação")
                        .size(17.0)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.log_export
                        .show(ui, self.log_path.as_deref(), &self.log, self.product);
                    if ui
                        .add_enabled(
                            !self.log.is_empty(),
                            egui::Button::new("Copiar log").small(),
                        )
                        .clicked()
                    {
                        ui.ctx().copy_text(self.log.join("\n"));
                    }
                });
            });
            self.log_export.status(ui);
            if let Some(path) = &self.log_path {
                ui.label(
                    egui::RichText::new(format!("Log completo: {}", path.display()))
                        .small()
                        .color(colors.muted),
                );
            }
            let (label, color) = self.state_label();
            ui.horizontal(|ui| {
                if self.running {
                    ui.add(egui::Spinner::new().size(14.0).color(colors.accent));
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 4.0, color);
                }
                ui.label(egui::RichText::new(label).small().color(color).strong());
                if let Some(started) = self.started_at {
                    let elapsed = self.finished_after.unwrap_or_else(|| started.elapsed());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}:{:02}",
                                elapsed.as_secs() / 60,
                                elapsed.as_secs() % 60
                            ))
                            .small()
                            .color(colors.muted),
                        );
                    });
                }
            });
            ui.separator();
            if self.log.is_empty() && !self.running {
                ui.add_space(12.0);
                egui::Frame::none().fill(colors.bg).rounding(10.0).inner_margin(18.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("Tudo começa por aqui").size(18.0).strong());
                    ui.label(egui::RichText::new("Revise as opções e inicie a instalação. As etapas e os detalhes aparecem neste painel.").color(colors.muted));
                    ui.add_space(10.0);
                    field_label(ui, "Nesta instalação");
                    ui.label(self.product.subtitle());
                    if self.product==Product::Acbr {ui.label(self.acbr_action.label());}
                });
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(
                        "A primeira compilação do ambiente pode levar bastante tempo.",
                    )
                    .small()
                    .color(colors.muted),
                );
            } else {
                if self.running {
                    ui.label(
                        egui::RichText::new(&self.phase)
                            .color(colors.accent)
                            .strong(),
                    );
                } else {
                    ui.label(egui::RichText::new(&self.status).strong());
                }
                let remaining = (height - 180.0).max(140.0);
                egui::Frame::none()
                    .fill(colors.bg)
                    .rounding(8.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                        egui::ScrollArea::both()
                            .id_salt("installation_log")
                            .max_height((remaining - 24.0).max(100.0))
                            .stick_to_bottom(true)
                            .auto_shrink([false, false])
                            .show_rows(ui, row_height, self.log.len(), |ui, range| {
                                for i in range {
                                    let line = &self.log[i];
                                    let color = if line.starts_with("ERRO:") {
                                        colors.error
                                    } else if line.to_lowercase().contains("aviso:") {
                                        colors.warning
                                    } else if line.starts_with("=== ") {
                                        colors.accent
                                    } else {
                                        colors.ink
                                    };
                                    ui.label(
                                        egui::RichText::new(line)
                                            .monospace()
                                            .color(color)
                                            .size(12.0)
                                            .text_style(egui::TextStyle::Monospace),
                                    );
                                }
                            });
                    });
            }
        });
    }

    fn state_label(&self) -> (&str, egui::Color32) {
        let colors = Palette::new(self.theme.dark);
        if self.running {
            (
                if self.cancelling {
                    "Cancelando..."
                } else {
                    "Em andamento"
                },
                colors.accent,
            )
        } else {
            match self.succeeded {
                Some(true) => ("Concluído", colors.accent),
                Some(false) if self.cancelling => ("Cancelado", colors.muted),
                Some(false) => ("Requer atenção", colors.error),
                None => ("Pronto para começar", colors.muted),
            }
        }
    }

    fn picking_folder(&self) -> bool {
        self.dir_picker.rx.is_some()
            || self.acbr_picker.rx.is_some()
            || self.fastreport_picker.rx.is_some()
    }

    fn about_dialog(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut open = true;
        let mut close = false;
        let colors = Palette::new(self.theme.dark);
        egui::Window::new(format!("Sobre o {}",self.product.title()))
            .id(egui::Id::new("program_information"))
            .open(&mut open).collapsible(false).resizable(false).default_width(440.0)
            .anchor(egui::Align2::CENTER_CENTER,egui::Vec2::ZERO)
            .show(ctx,|ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.logo).fit_to_exact_size(egui::vec2(64.0,64.0)));
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(self.product.title()).size(22.0).strong());
                        ui.label(egui::RichText::new(format!("Versão {}",install_core::VERSION)).color(colors.accent));
                    });
                });
                ui.add_space(12.0);
                ui.label(self.product.subtitle());
                ui.label(match self.product {
                    Product::Acbr => "Lazarus: Linux, Windows e macOS. Delphi: somente Windows.",
                    Product::Pascal => "FPC, Lazarus, cross-compilers, Online Package Manager e designer integrado.",
                });
                ui.add_space(12.0);
                ui.label(format!("Autor: {}",install_core::AUTHORS));
                ui.label(format!("Licença: {}",install_core::LICENSE));
                ui.label(egui::RichText::new(format!("Sistema: {} / {}",std::env::consts::OS,std::env::consts::ARCH)).small().color(colors.muted));
                ui.hyperlink_to("Código-fonte e informações do projeto",install_core::REPOSITORY);
                if self.product==Product::Acbr {
                    ui.label(egui::RichText::new("ACBr e seu ícone pertencem ao Projeto ACBr.").small().color(colors.muted));
                }
                egui::CollapsingHeader::new("Ver licença MIT").show(ui,|ui| {
                    egui::ScrollArea::vertical().id_salt("license_text").max_height(180.0).show(ui,|ui| {
                        ui.label(egui::RichText::new(install_core::LICENSE_TEXT).small());
                    });
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Copiar informações").clicked() {ui.ctx().copy_text(self.product.information());}
                    if ui.button("Fechar").clicked() {close=true;}
                });
            });
        self.about_open = open && !close;
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.theme.sync(ctx);
        self.drenar();
        self.inspect_folders(ctx);
        if self.running {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if self.product == Product::Acbr {
            self.acbr_wizard(ctx);
            self.about_dialog(ctx);
            return;
        }
        let colors = Palette::new(self.theme.dark);
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(16, 43, 81))
                    .inner_margin(egui::Margin::symmetric(22.0, 14.0)),
            )
            .show(ctx, |ui| self.header(ui));
        egui::TopBottomPanel::bottom("actions")
            .frame(
                egui::Frame::none()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::symmetric(22.0, 12.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        let (label, color) = self.state_label();
                        ui.label(egui::RichText::new(label).color(color).strong());
                        ui.label(
                            egui::RichText::new(if self.running {
                                "Você pode acompanhar os detalhes no log."
                            } else {
                                "Configure o ambiente e inicie quando estiver pronto."
                            })
                            .small()
                            .color(colors.muted),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let can_start = !self.running
                            && !self.inspection.pending()
                            && !self.picking_folder()
                            && (self.product == Product::Pascal
                                || self
                                    .inspection
                                    .result
                                    .as_ref()
                                    .and_then(|r| r.catalog.as_ref().ok())
                                    .map_or(true, |c| {
                                        c.resolve_with(
                                            self.acbr_packages.as_deref(),
                                            self.all_components,
                                        )
                                        .is_ok_and(|p| !p.is_empty())
                                    }))
                            && if self.product == Product::Acbr && self.ide == Ide::Delphi {
                                !self.delphi_dir.trim().is_empty()
                                    && !self.delphi.registry_key.is_empty()
                            } else {
                                !self.dir.trim().is_empty()
                            };
                        let title = if self.running {
                            "Instalando..."
                        } else if self.product == Product::Acbr {
                            "Instalar ACBr"
                        } else {
                            "Preparar Pascal"
                        };
                        let button = egui::Button::new(
                            egui::RichText::new(title)
                                .color(egui::Color32::WHITE)
                                .strong(),
                        )
                        .fill(PRIMARY)
                        .stroke(egui::Stroke::NONE)
                        .min_size(egui::vec2(168.0, 42.0));
                        if ui.add_enabled(can_start, button).clicked() {
                            self.start(ctx.clone());
                        }
                        if self.running
                            && ui
                                .add_enabled(
                                    !self.cancelling,
                                    egui::Button::new("Cancelar").min_size(egui::vec2(90.0, 42.0)),
                                )
                                .clicked()
                        {
                            self.cancelling = true;
                            if let Some(ctx) = &self.ctx_run {
                                ctx.cancel();
                            }
                        }
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(colors.bg).inner_margin(18.0))
            .show(ctx, |ui| {
                if self.page == Page::Activity {
                    self.activity(ui, ui.available_height());
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("configuration")
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.configuration(ui));
                }
            });
        self.about_dialog(ctx);
    }
}

pub fn run(product: Product) -> eframe::Result<()> {
    #[cfg(target_os = "linux")]
    if let Err(error) = crate::desktop::install(product) {
        eprintln!(
            "Não foi possível registrar o ícone de {} no desktop: {error}",
            product.title()
        );
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id(product.application_id())
            .with_icon(crate::icons::window_icon(product))
            .with_inner_size(if product == Product::Acbr {
                [1100.0, 800.0]
            } else {
                [980.0, 740.0]
            })
            .with_min_inner_size([760.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        product.title(),
        options,
        Box::new(move |cc| Ok(Box::new(App::new(&cc.egui_ctx, product)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_frame(ctx: &egui::Context, theme: &mut AppTheme, system: Option<egui::Theme>) {
        let input = egui::RawInput {
            system_theme: system,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| theme.sync(ctx));
    }

    #[test]
    fn follows_system_changes_and_keeps_the_custom_palette() {
        let ctx = egui::Context::default();
        let mut theme = AppTheme::with_system(&ctx, crate::system_theme::SystemTheme::default());
        system_frame(&ctx, &mut theme, Some(egui::Theme::Dark));
        assert!(theme.dark);
        assert!(ctx.style().visuals.dark_mode);
        assert_eq!(ctx.style().visuals.window_fill, Palette::new(true).surface);

        system_frame(&ctx, &mut theme, Some(egui::Theme::Light));
        assert!(!theme.dark);
        assert!(!ctx.style().visuals.dark_mode);
        assert_eq!(ctx.style().visuals.window_fill, Palette::new(false).surface);
    }

    #[test]
    fn manual_choice_overrides_system_until_following_is_restored() {
        let ctx = egui::Context::default();
        let mut theme = AppTheme::with_system(&ctx, crate::system_theme::SystemTheme::default());
        system_frame(&ctx, &mut theme, None);
        assert!(!theme.dark); // No OS preference: use light.

        theme.toggle(&ctx);
        system_frame(&ctx, &mut theme, Some(egui::Theme::Light));
        assert!(theme.dark);
        assert!(ctx.style().visuals.dark_mode);

        theme.follow_system(&ctx);
        assert!(!theme.dark);
        system_frame(&ctx, &mut theme, Some(egui::Theme::Dark));
        assert!(theme.dark);
    }

    #[test]
    fn linux_desktop_preference_applies_without_native_theme_and_preserves_manual_choice() {
        use crate::system_theme::SystemTheme;
        let ctx = egui::Context::default();
        assert!(ctx.system_theme().is_none());
        let mut theme = AppTheme::with_system(&ctx, SystemTheme::fixed(Some(egui::Theme::Dark)));
        assert!(theme.dark);
        assert!(ctx.style().visuals.dark_mode);
        system_frame(&ctx, &mut theme, None);
        assert!(theme.dark);
        theme.toggle(&ctx);
        system_frame(&ctx, &mut theme, None);
        assert!(!theme.dark);
        theme.follow_system(&ctx);
        assert!(theme.dark);
        theme.system = SystemTheme::fixed(Some(egui::Theme::Light));
        system_frame(&ctx, &mut theme, None);
        assert!(!theme.dark);
        assert_eq!(ctx.style().visuals.window_fill, Palette::new(false).surface);
    }
}
