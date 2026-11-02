use super::*;

#[derive(Default)]
pub(super) struct LogExport {
    rx: Option<Receiver<Result<Option<PathBuf>, String>>>,
    message: Option<(String, bool)>,
}

impl LogExport {
    pub fn poll(&mut self) {
        if let Some(rx) = &self.rx {
            let result = match rx.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err("A exportação foi interrompida.".into())
                }
            };
            self.message = match result {
                Ok(Some(path)) => Some((format!("Log salvo em {}", path.display()), false)),
                Ok(None) => None,
                Err(error) => Some((format!("Não foi possível salvar o log: {error}"), true)),
            };
            self.rx = None;
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        source: Option<&std::path::Path>,
        lines: &[String],
        product: Product,
    ) {
        if ui
            .add_enabled(
                self.rx.is_none() && (source.is_some() || !lines.is_empty()),
                egui::Button::new(if self.rx.is_some() {
                    "Salvando..."
                } else {
                    "Salvar log"
                })
                .small(),
            )
            .clicked()
        {
            let source = source.map(PathBuf::from);
            let fallback = if source.is_none() {
                lines.join("\n")
            } else {
                String::new()
            };
            let filename = source
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| {
                    format!("{}-{}.log", product.executable(), install_core::VERSION)
                });
            let ctx = ui.ctx().clone();
            let (tx, rx) = mpsc::channel();
            self.rx = Some(rx);
            self.message = None;
            std::thread::spawn(move || {
                let destination = rfd::FileDialog::new()
                    .set_title("Salvar log da instalação")
                    .set_file_name(filename)
                    .add_filter("Arquivo de log", &["log", "txt"])
                    .save_file();
                let result = destination
                    .map(|path| {
                        install_core::logs::export(source.as_deref(), &fallback, &path)
                            .map(|_| path)
                            .map_err(|e| e.to_string())
                    })
                    .transpose();
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        }
    }

    pub fn status(&self, ui: &mut egui::Ui) {
        if let Some((message, error)) = &self.message {
            ui.label(egui::RichText::new(message).small().color(if *error {
                Palette::of(ui).error
            } else {
                Palette::of(ui).accent
            }));
        }
    }
}
