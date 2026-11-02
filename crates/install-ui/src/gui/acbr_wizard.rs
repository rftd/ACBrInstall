//! ACBr's familiar installation wizard, styled with the shared desktop palette.
use super::*;

pub(super) const FAMILIES: [&str; 10] = [
    "Componentes básicos",
    "Automação e comunicação",
    "Documentos fiscais eletrônicos",
    "Financeiro",
    "Arquivos e obrigações fiscais",
    "Relatórios FortesReport",
    "Relatórios em PDF",
    "LazReport e PowerPDF",
    "FastReport",
    "Outros componentes",
];

pub(super) fn package_family(package: &install_core::packages::Package) -> &'static str {
    let name = package.name.to_ascii_lowercase();
    if package.requires_fastreport {
        return FAMILIES[8];
    }
    if name.contains("lazreport") {
        return FAMILIES[7];
    }
    if name.contains("fpdf") {
        return FAMILIES[6];
    }
    if name.ends_with("rl") || name.contains("fortes") {
        return FAMILIES[5];
    }
    match package.group.to_ascii_lowercase().as_str() {
        "acbrcomum" | "acbrlibxml2" | "acbropenssl" | "pcncomum" | "synapse" => FAMILIES[0],
        "acbrserial" | "acbrtcp" | "acbrtefd" | "acbrsat" => FAMILIES[1],
        "acbrdfe" => FAMILIES[2],
        "acbrboleto" | "acbrpixcd" | "acbrbaas" | "acbrpagfor" | "acbrdebitoautomatico" => {
            FAMILIES[3]
        }
        "acbrtxt" => FAMILIES[4],
        _ => FAMILIES[9],
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Welcome,
    Environment,
    Configuration,
    Components,
    Installation,
    Finish,
}

impl Step {
    const ALL: [Self; 6] = [
        Self::Welcome,
        Self::Environment,
        Self::Configuration,
        Self::Components,
        Self::Installation,
        Self::Finish,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap()
    }

    fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Bem-vindo",
            Self::Environment => "Ambiente",
            Self::Configuration => "Configurações",
            Self::Components => "Componentes",
            Self::Installation => "Instalação",
            Self::Finish => "Conclusão",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Welcome => "Conheça o assistente",
            Self::Environment => "Escolha sua IDE",
            Self::Configuration => "Fontes e dependências",
            Self::Components => "Selecione os pacotes",
            Self::Installation => "Revise e acompanhe",
            Self::Finish => "Tudo pronto",
        }
    }
}

fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            egui::RichText::new(label)
                .color(egui::Color32::WHITE)
                .strong(),
        )
        .fill(PRIMARY)
        .stroke(egui::Stroke::NONE)
        .min_size(egui::vec2(150.0, 42.0)),
    )
}

fn note(ui: &mut egui::Ui, title: &str, description: &str) {
    let colors = Palette::of(ui);
    egui::Frame::none()
        .fill(colors.tint)
        .rounding(10.0)
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).strong().color(colors.accent));
            ui.label(egui::RichText::new(description).small().color(colors.muted));
        });
}

impl App {
    pub(super) fn acbr_wizard(&mut self, ctx: &egui::Context) {
        let colors = Palette::new(self.theme.dark);
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(16, 43, 81))
                    .inner_margin(egui::Margin::symmetric(24.0, 18.0)),
            )
            .show(ctx, |ui| self.header(ui));
        egui::TopBottomPanel::bottom("wizard_actions")
            .frame(
                egui::Frame::none()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::symmetric(24.0, 14.0)),
            )
            .show(ctx, |ui| self.acbr_actions(ui));
        egui::SidePanel::left("wizard_steps")
            .exact_width(if ctx.screen_rect().width() < 980.0 {
                180.0
            } else {
                220.0
            })
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::symmetric(14.0, 24.0)),
            )
            .show(ctx, |ui| self.acbr_steps(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(colors.bg).inner_margin(24.0))
            .show(ctx, |ui| {
                let content_height = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt(("wizard_content", self.acbr_step.index()))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if self.acbr_step != Step::Welcome {
                            ui.label(
                                egui::RichText::new(format!(
                                    "PASSO {:02} / 06",
                                    self.acbr_step.index() + 1
                                ))
                                .small()
                                .strong()
                                .color(colors.accent),
                            );
                            ui.label(
                                egui::RichText::new(self.acbr_step.title())
                                    .size(27.0)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(self.acbr_step.description())
                                    .color(colors.muted),
                            );
                            ui.add_space(16.0);
                        }
                        ui.add_enabled_ui(!self.running && !self.picking_folder(), |ui| match self
                            .acbr_step
                        {
                            Step::Welcome => self.acbr_welcome(ui),
                            Step::Environment => self.environment_card(ui),
                            Step::Configuration => self.acbr_options(ui),
                            Step::Components => {
                                self.packages_card(ui, (content_height - 270.0).max(240.0))
                            }
                            Step::Installation | Step::Finish => {}
                        });
                        match self.acbr_step {
                            Step::Installation
                                if self.running
                                    || self.install_attempted
                                    || self.succeeded == Some(false) =>
                            {
                                self.activity(ui, (content_height - 92.0).max(340.0))
                            }
                            Step::Installation => self.acbr_review(ui),
                            Step::Finish => self.acbr_finish(ui),
                            _ => {}
                        }
                    });
            });
    }

    fn acbr_steps(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::of(ui);
        ui.label(
            egui::RichText::new("ASSISTENTE ACBr")
                .small()
                .strong()
                .color(colors.muted),
        );
        ui.add_space(16.0);
        for step in Step::ALL {
            let selected = self.acbr_step == step;
            let enabled = !self.running
                && !self.picking_folder()
                && (step != Step::Finish || self.succeeded == Some(true));
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 62.0),
                if enabled {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, step.title())
            });
            if selected || response.hovered() && enabled {
                ui.painter().rect_filled(rect, 9.0, colors.tint);
            }
            let circle = egui::pos2(rect.left() + 23.0, rect.top() + 24.0);
            ui.painter()
                .circle_filled(circle, 13.0, if selected { PRIMARY } else { colors.field });
            ui.painter().circle_stroke(
                circle,
                13.0,
                egui::Stroke::new(1.0_f32, if selected { PRIMARY } else { colors.border }),
            );
            let ink = if !enabled {
                colors.muted
            } else if selected {
                colors.accent
            } else {
                colors.ink
            };
            ui.painter().text(
                circle,
                egui::Align2::CENTER_CENTER,
                (step.index() + 1).to_string(),
                egui::FontId::proportional(12.0),
                if selected {
                    egui::Color32::WHITE
                } else {
                    colors.muted
                },
            );
            ui.painter().text(
                egui::pos2(rect.left() + 46.0, rect.top() + 15.0),
                egui::Align2::LEFT_TOP,
                step.title(),
                egui::FontId::proportional(14.0),
                ink,
            );
            ui.painter().text(
                egui::pos2(rect.left() + 46.0, rect.top() + 35.0),
                egui::Align2::LEFT_TOP,
                step.description(),
                egui::FontId::proportional(10.0),
                colors.muted,
            );
            if enabled && response.clicked() {
                self.acbr_step = step;
            }
            ui.add_space(3.0);
        }
        ui.add_space(18.0);
        ui.separator();
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("IDE DE DESTINO")
                .small()
                .color(colors.muted),
        );
        ui.label(
            egui::RichText::new(if self.ide == Ide::Lazarus {
                "Lazarus"
            } else {
                "Delphi"
            })
            .strong(),
        );
        if let Some(count) = self.acbr_selection_count() {
            ui.label(
                egui::RichText::new(format!(
                    "{count} pacote{} selecionado{}",
                    if count == 1 { "" } else { "s" },
                    if count == 1 { "" } else { "s" }
                ))
                .small()
                .color(colors.muted),
            );
        }
    }

    fn acbr_selection_count(&self) -> Option<usize> {
        self.inspection
            .result
            .as_ref()?
            .catalog
            .as_ref()
            .ok()?
            .resolve_with(self.acbr_packages.as_deref(), self.all_components)
            .ok()
            .map(|plan| plan.len())
    }

    fn acbr_environment_ready(&self) -> bool {
        if self.ide == Ide::Delphi {
            cfg!(windows)
                && !self.delphi.registry_key.is_empty()
                && PathBuf::from(self.delphi_dir.trim())
                    .join("bin/dcc32.exe")
                    .is_file()
        } else {
            !self.dir.trim().is_empty()
                && install_core::platform::lazbuild(&PathBuf::from(self.dir.trim())).is_file()
        }
    }

    fn acbr_sources_ready(&self) -> bool {
        if self.inspection.pending() {
            return false;
        }
        self.inspection
            .result
            .as_ref()
            .and_then(|r| r.acbr.as_ref().ok())
            .is_some_and(|info| match self.acbr_action {
                AcbrAction::Install => info.sources,
                AcbrAction::UpdateAndInstall => info.sources && info.svn,
                AcbrAction::DownloadAndInstall => !info.occupied,
            })
    }

    fn acbr_can_install(&self) -> bool {
        !self.running
            && !self.picking_folder()
            && self.acbr_environment_ready()
            && self.acbr_sources_ready()
            && self.acbr_selection_count().is_some_and(|n| n > 0)
    }

    fn acbr_actions(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::of(ui);
        let step = self.acbr_step;
        let ready = match step {
            Step::Welcome => true,
            Step::Environment => self.acbr_environment_ready(),
            Step::Configuration => self.acbr_sources_ready(),
            Step::Components => self.acbr_selection_count().is_some_and(|n| n > 0),
            Step::Installation => self.acbr_can_install(),
            Step::Finish => true,
        } && !self.running
            && !self.picking_folder();
        let hint = if self.running {
            "Acompanhe os detalhes da instalação."
        } else if self.inspection.pending() {
            "Verificando as pastas selecionadas..."
        } else if !ready {
            match step {
                Step::Environment => "Selecione uma instalação existente da IDE.",
                Step::Configuration => "Revise a pasta dos fontes e a ação escolhida.",
                Step::Components => "Selecione os componentes ou baixe os fontes primeiro.",
                _ => "Revise o ambiente, os fontes e os componentes.",
            }
        } else if step == Step::Installation {
            "Tudo pronto para iniciar a instalação."
        } else if step == Step::Finish {
            "Reabra sua IDE para usar os componentes."
        } else {
            "Suas escolhas são mantidas ao mudar de etapa."
        };
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2((ui.available_width() - 360.0).max(170.0), 44.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    let (label, color) = if self.running || self.succeeded.is_some() {
                        self.state_label()
                    } else {
                        (step.title(), colors.ink)
                    };
                    ui.label(egui::RichText::new(label).strong().color(color));
                    ui.label(egui::RichText::new(hint).small().color(colors.muted));
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.running {
                    if ui
                        .add_enabled(
                            !self.cancelling,
                            egui::Button::new("Cancelar").min_size(egui::vec2(120.0, 42.0)),
                        )
                        .clicked()
                    {
                        self.cancelling = true;
                        if let Some(ctx) = &self.ctx_run {
                            ctx.cancel();
                        }
                    }
                    ui.add(egui::Spinner::new().color(colors.accent));
                    return;
                }
                let label = match step {
                    Step::Welcome => "Começar",
                    Step::Components => "Revisar instalação",
                    Step::Installation => {
                        if self.succeeded == Some(false) {
                            "Tentar novamente"
                        } else {
                            "Instalar ACBr"
                        }
                    }
                    Step::Finish => "Finalizar",
                    _ => "Próximo",
                };
                if primary_button(ui, label, ready).clicked() {
                    match step {
                        Step::Installation => self.start(ui.ctx().clone()),
                        Step::Finish => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                        _ => self.acbr_step = Step::ALL[step.index() + 1],
                    }
                }
                if step != Step::Welcome
                    && ui
                        .add_enabled(
                            !self.picking_folder(),
                            egui::Button::new("Voltar").min_size(egui::vec2(95.0, 42.0)),
                        )
                        .clicked()
                {
                    self.acbr_step = Step::ALL[step.index() - 1];
                }
            });
        });
    }

    fn acbr_welcome(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::of(ui);
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                egui::Frame::none()
                    .fill(colors.tint)
                    .rounding(18.0)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Image::new(&self.logo).fit_to_exact_size(egui::vec2(72.0, 72.0)),
                        );
                    });
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("PROJETO ACBr")
                            .small()
                            .strong()
                            .color(colors.accent),
                    );
                    ui.label(
                        egui::RichText::new("Bem-vindo ao ACBrInstall")
                            .size(27.0)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new("Seus componentes, na sua IDE.").color(colors.muted),
                    );
                });
            });
            ui.add_space(20.0);
            ui.label("Este assistente prepara os componentes ACBr para uma instalação existente do Lazarus ou Delphi. Escolha o ambiente, revise as opções e selecione os pacotes que deseja usar.");
            ui.add_space(18.0);
            for (number, title, description) in [
                (
                    "01",
                    "Escolha o ambiente",
                    "Selecione sua IDE e a pasta dos fontes do ACBr.",
                ),
                (
                    "02",
                    "Selecione os componentes",
                    "Marque os pacotes e suas integrações de relatórios.",
                ),
                (
                    "03",
                    "Instale e acompanhe",
                    "Revise a seleção e acompanhe a compilação pelo log.",
                ),
            ] {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(32.0, 40.0), egui::Sense::hover());
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        number,
                        egui::FontId::proportional(14.0),
                        colors.accent,
                    );
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(title).strong());
                        ui.label(egui::RichText::new(description).small().color(colors.muted));
                    });
                });
                ui.add_space(8.0);
            }
        });
        ui.add_space(14.0);
        note(ui,"Dependências junto do ACBr",
            "FortesReport-CE e PowerPDF são preparados nesta instalação quando exigidos pelos componentes Lazarus selecionados. FastReport usa a sua cópia adquirida.");
        ui.add_space(12.0);
        ui.label(egui::RichText::new("As últimas opções são salvas ao iniciar a instalação. Para preparar FPC e Lazarus, use o PascalInstall.").small().color(colors.muted));
    }

    fn acbr_options(&mut self, ui: &mut egui::Ui) {
        self.acbr_card(ui);
        ui.add_space(14.0);
        if self.ide == Ide::Delphi {
            card(ui, |ui| {
                ui.label(egui::RichText::new("Opções de compilação Delphi").strong());
                ui.checkbox(&mut self.delphi.win64, "Compilar bibliotecas Win64");
                ui.checkbox(&mut self.delphi.debug, "Compilar também em Debug");
                ui.checkbox(
                    &mut self.delphi.ide64,
                    "Instalar também na IDE Delphi de 64 bits",
                );
                ui.label(egui::RichText::new("FortesReport-CE e FastReport devem estar previamente instalados no Delphi.").small().color(Palette::of(ui).muted));
            });
            ui.add_space(14.0);
        }
        note(
            ui,
            "Relatórios e dependências",
            if self.ide == Ide::Lazarus {
                "O ACBrInstall instala FortesReport-CE e PowerPDF antes de compilar os pacotes que os utilizam. Pacotes já registrados são reutilizados. FastReport permanece opcional."
            } else {
                "Selecione apenas as integrações de relatórios disponíveis na sua instalação Delphi. O download automático pelo OPM é exclusivo do Lazarus."
            },
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(if self.ide == Ide::Lazarus {
                "A recompilação preserva um backup dos artefatos antigos da plataforma atual."
            } else {
                "A instalação preserva um backup das configurações alteradas no registro."
            })
            .small()
            .color(Palette::of(ui).muted),
        );
    }

    fn acbr_summary(&self, ui: &mut egui::Ui) {
        let colors = Palette::of(ui);
        egui::Grid::new("acbr_review")
            .num_columns(2)
            .spacing([22.0, 14.0])
            .show(ui, |ui| {
                let target = if self.ide == Ide::Lazarus {
                    &self.dir
                } else {
                    &self.delphi_dir
                };
                for (label, value) in [
                    (
                        "IDE de destino",
                        if self.ide == Ide::Lazarus {
                            "Lazarus".to_string()
                        } else {
                            "Delphi".to_string()
                        },
                    ),
                    ("Ambiente", target.clone()),
                    (
                        "Fontes do ACBr",
                        if self.acbr_dir.trim().is_empty() {
                            PathBuf::from(&self.dir).join("ACBr").display().to_string()
                        } else {
                            self.acbr_dir.clone()
                        },
                    ),
                    ("Ação", self.acbr_action.label().to_string()),
                    ("Componentes", {
                        let count = self.acbr_selection_count().unwrap_or(0);
                        format!(
                            "{count} pacote{}, incluindo dependências",
                            if count == 1 { "" } else { "s" }
                        )
                    }),
                ] {
                    ui.label(egui::RichText::new(label).small().color(colors.muted));
                    ui.add(egui::Label::new(value).wrap());
                    ui.end_row();
                }
            });
    }

    fn acbr_review(&self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("Revise sua instalação")
                    .size(18.0)
                    .strong(),
            );
            ui.add_space(12.0);
            self.acbr_summary(ui);
        });
        ui.add_space(14.0);
        note(ui,"Pronto para compilar", "Feche a IDE selecionada antes de iniciar. Os detalhes de cada etapa aparecerão aqui durante a instalação.");
    }

    fn acbr_finish(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::of(ui);
        card(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 30.0, colors.tint);
            let center = rect.center();
            ui.painter().line_segment(
                [
                    center + egui::vec2(-12.0, 0.0),
                    center + egui::vec2(-3.0, 9.0),
                ],
                egui::Stroke::new(3.0_f32, colors.accent),
            );
            ui.painter().line_segment(
                [
                    center + egui::vec2(-3.0, 9.0),
                    center + egui::vec2(14.0, -10.0),
                ],
                egui::Stroke::new(3.0_f32, colors.accent),
            );
            ui.label(
                egui::RichText::new("Instalação concluída!")
                    .size(25.0)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("Reabra sua IDE para usar os componentes ACBr selecionados.")
                    .color(colors.muted),
            );
            ui.add_space(20.0);
            self.acbr_summary(ui);
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                if ui.button("Ver log da instalação").clicked() {
                    self.acbr_step = Step::Installation;
                }
                if ui.button("Copiar log").clicked() {
                    ui.ctx().copy_text(self.log.join("\n"));
                }
                self.log_export
                    .show(ui, self.log_path.as_deref(), &self.log, self.product);
            });
            self.log_export.status(ui);
        });
    }
}
