//! Complete per-run logs, independent of the bounded GUI history.
use crate::Product;
use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

pub(crate) struct SessionLog {
    pub path: PathBuf,
    file: Mutex<fs::File>,
    failed: AtomicBool,
}

impl SessionLog {
    pub fn create(product: Product) -> io::Result<Self> {
        Self::in_directory(&directory(product)?, product)
    }

    fn in_directory(directory: &Path, product: Product) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = directory.join(format!(
            "{}-{stamp}-{}.log",
            product.executable(),
            std::process::id()
        ));
        let mut options = fs::OpenOptions::new();
        options.create_new(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        let log = Self {
            path,
            file: Mutex::new(file),
            failed: AtomicBool::new(false),
        };
        log.append(&product.information())?;
        log.append("")?;
        Ok(log)
    }

    pub fn append(&self, line: &str) -> io::Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("Log indisponível"))?;
        writeln!(file, "{line}")
    }

    pub fn first_failure(&self) -> bool {
        !self.failed.swap(true, Ordering::Relaxed)
    }
}

pub fn directory(product: Product) -> io::Result<PathBuf> {
    let required = |name| {
        env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let path = if cfg!(windows) {
        required("LOCALAPPDATA")
            .or_else(|| required("APPDATA"))
            .map(|p| p.join(product.title()))
    } else if cfg!(target_os = "macos") {
        required("HOME").map(|p| p.join("Library/Logs").join(product.title()))
    } else {
        required("XDG_STATE_HOME")
            .filter(|p| p.is_absolute())
            .or_else(|| required("HOME").map(|p| p.join(".local/state")))
            .map(|p| p.join(product.executable()))
    }
    .ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Pasta de logs do usuário não encontrada",
        )
    })?;
    Ok(if cfg!(target_os = "macos") {
        path
    } else {
        path.join("logs")
    })
}

/// Export through a temporary file so an I/O failure does not truncate an
/// existing destination. Selecting the original log leaves its writer intact.
pub fn export(source: Option<&Path>, fallback: &str, destination: &Path) -> io::Result<()> {
    if let Some(source) = source {
        if source.canonicalize()? == destination.canonicalize().unwrap_or_default() {
            return Ok(());
        }
    }
    let bytes = match source {
        Some(p) => {
            let file = fs::File::open(p)?;
            let length = file.metadata()?.len();
            let mut bytes = Vec::new();
            file.take(length).read_to_end(&mut bytes)?;
            bytes
        }
        None => fallback.as_bytes().to_vec(),
    };
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".log-export-{}-{stamp}.tmp", std::process::id()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_full_history_exports_and_keeps_active_source_appendable() {
        let root = env::temp_dir().join(format!(
            "installer-logs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let log = SessionLog::in_directory(&root, Product::Acbr).unwrap();
        let second = SessionLog::in_directory(&root, Product::Acbr).unwrap();
        assert_ne!(log.path, second.path);
        for line in 0..25_001 {
            log.append(&format!("linha {line}")).unwrap();
        }
        let saved = root.join("saved.log");
        export(Some(&log.path), "truncated GUI", &saved).unwrap();
        let text = fs::read_to_string(&saved).unwrap();
        assert!(text.contains("linha 0\n") && text.contains("linha 25000\n"));
        assert!(text.contains(crate::VERSION));
        export(Some(&log.path), "", &log.path).unwrap();
        log.append("ERRO: exemplo").unwrap();
        assert!(fs::read_to_string(&log.path)
            .unwrap()
            .ends_with("ERRO: exemplo\n"));
        assert!(export(Some(&root.join("missing")), "fallback", &saved).is_err());
        assert_eq!(fs::read_to_string(&saved).unwrap(), text);
        export(None, "GUI only", &saved).unwrap();
        assert_eq!(fs::read_to_string(&saved).unwrap(), "GUI only");
        drop(log);
        drop(second);
        fs::remove_dir_all(root).unwrap();
    }
}
