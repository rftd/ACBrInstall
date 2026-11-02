//! Linux's winit backend does not report a system theme. Read the desktop
//! preference separately; Windows/macOS continue using native window events.
use eframe::egui;

#[derive(Default)]
pub struct SystemTheme {
    value: Option<egui::Theme>,
    #[cfg(target_os = "linux")]
    updates: Option<std::sync::mpsc::Receiver<Option<egui::Theme>>>,
    #[cfg(target_os = "linux")]
    stop: Option<std::sync::mpsc::Sender<()>>,
}

impl SystemTheme {
    pub fn new(ctx: &egui::Context) -> Self {
        #[cfg(target_os = "linux")]
        {
            use std::{sync::mpsc, time::Duration};
            // Resolve before the first frame so dark desktops do not flash light.
            let value = linux::detect();
            let (tx, updates) = mpsc::channel();
            let (stop, stopped) = mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let mut previous = value;
                while let Err(mpsc::RecvTimeoutError::Timeout) =
                    stopped.recv_timeout(Duration::from_secs(2))
                {
                    let next = linux::detect();
                    if next != previous {
                        if tx.send(next).is_err() {
                            break;
                        }
                        previous = next;
                        ctx.request_repaint();
                    }
                }
            });
            Self {
                value,
                updates: Some(updates),
                stop: Some(stop),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = ctx;
            Self::default()
        }
    }

    pub fn current(&mut self) -> Option<egui::Theme> {
        #[cfg(target_os = "linux")]
        if let Some(updates) = &self.updates {
            for value in updates.try_iter() {
                self.value = value;
            }
        }
        self.value
    }

    #[cfg(test)]
    pub fn fixed(value: Option<egui::Theme>) -> Self {
        let mut system = Self::default();
        system.value = value;
        system
    }
}

#[cfg(target_os = "linux")]
impl Drop for SystemTheme {
    fn drop(&mut self) {
        if let Some(stop) = &self.stop {
            let _ = stop.send(());
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use eframe::egui::Theme;
    use std::{
        env, fs,
        path::PathBuf,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    fn output(program: &str, args: &[&str]) -> Option<String> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let deadline = Instant::now() + Duration::from_millis(750);
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => {
                    return String::from_utf8(child.wait_with_output().ok()?.stdout).ok();
                }
                Ok(Some(_)) => return None,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        }
    }

    fn portal_value(text: &str) -> Option<u32> {
        let mut words = text
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty());
        words.find(|word| *word == "uint32")?;
        words.next()?.parse().ok()
    }

    fn portal() -> Option<Theme> {
        // Settings v2 offers ReadOne; Read also supports older portal versions.
        // Both utilities are optional, and no desktop settings are modified.
        for method in ["ReadOne", "Read"] {
            let dbus_method = format!("org.freedesktop.portal.Settings.{method}");
            let response = output(
                "dbus-send",
                &[
                    "--session",
                    "--reply-timeout=500",
                    "--print-reply",
                    "--dest=org.freedesktop.portal.Desktop",
                    "/org/freedesktop/portal/desktop",
                    &dbus_method,
                    "string:org.freedesktop.appearance",
                    "string:color-scheme",
                ],
            )
            .and_then(|s| portal_value(&s))
            .or_else(|| {
                output(
                    "gdbus",
                    &[
                        "call",
                        "--session",
                        "--timeout",
                        "1",
                        "--dest",
                        "org.freedesktop.portal.Desktop",
                        "--object-path",
                        "/org/freedesktop/portal/desktop",
                        "--method",
                        &dbus_method,
                        "org.freedesktop.appearance",
                        "color-scheme",
                    ],
                )
                .and_then(|s| portal_value(&s))
            });
            if let Some(value) = response {
                return match value {
                    1 => Some(Theme::Dark),
                    2 => Some(Theme::Light),
                    _ => None,
                };
            }
        }
        None
    }

    fn named_preference(text: &str) -> Option<Theme> {
        match text.trim().trim_matches('\'').trim_matches('"') {
            "prefer-dark" => Some(Theme::Dark),
            "prefer-light" => Some(Theme::Light),
            _ => None,
        }
    }

    fn kde_theme(text: &str) -> Option<Theme> {
        let mut window = false;
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') {
                window = line == "[Colors:Window]";
            } else if window {
                if let Some(color) = line.strip_prefix("BackgroundNormal=") {
                    let rgb: Vec<u32> = color
                        .split(',')
                        .map(str::trim)
                        .map(str::parse)
                        .collect::<Result<_, _>>()
                        .ok()?;
                    if rgb.len() != 3 || rgb.iter().any(|&v| v > 255) {
                        return None;
                    }
                    let brightness = (299 * rgb[0] + 587 * rgb[1] + 114 * rgb[2]) / 1000;
                    return Some(if brightness < 128 {
                        Theme::Dark
                    } else {
                        Theme::Light
                    });
                }
            }
        }
        None
    }

    fn kde() -> Option<Theme> {
        let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        if !desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("KDE"))
        {
            return None;
        }
        let directory = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))?;
        kde_theme(&fs::read_to_string(directory.join("kdeglobals")).ok()?)
    }

    pub fn detect() -> Option<Theme> {
        portal()
            .or_else(kde)
            .or_else(|| {
                output(
                    "gsettings",
                    &["get", "org.gnome.desktop.interface", "color-scheme"],
                )
                .and_then(|text| named_preference(&text))
            })
            .or_else(|| {
                let theme = env::var("GTK_THEME").ok()?.to_ascii_lowercase();
                theme.contains("dark").then_some(Theme::Dark)
            })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_portal_variants_without_confusing_reply_serial_with_color_scheme() {
            assert_eq!(
                portal_value("method return serial=2 reply_serial=1\nvariant uint32 2"),
                Some(2)
            );
            assert_eq!(portal_value("(<<uint32 1>>,)"), Some(1));
            assert_eq!(portal_value("(<uint32 0>,)"), Some(0));
            assert_eq!(portal_value("Error: service unavailable"), None);
            assert_eq!(named_preference("'prefer-dark'\n"), Some(Theme::Dark));
            assert_eq!(named_preference("'prefer-light'\n"), Some(Theme::Light));
            assert_eq!(named_preference("'default'"), None);
        }

        #[test]
        fn kde_custom_schemes_use_window_colors_instead_of_themes_name() {
            assert_eq!(
                kde_theme(
                    "[General]\nColorScheme=Custom\n[Colors:Window]\nBackgroundNormal=54,54,54\n"
                ),
                Some(Theme::Dark)
            );
            assert_eq!(kde_theme("[Colors:Window]\nBackgroundNormal=243,247,254\n[Other]\nBackgroundNormal=0,0,0"),Some(Theme::Light));
            assert_eq!(kde_theme("[Colors:Window]\nBackgroundNormal=invalid"), None);
        }
    }
}
