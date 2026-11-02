//! Execução de processos, cancelamento, logs e downloads compartilhados.
use crate::platform::EXE;
use anyhow::{anyhow, bail, Context, Result};
use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Contexto de execução: log, cancelamento e processo filho em andamento.
#[derive(Clone)]
pub struct Ctx {
    log: Arc<dyn Fn(String) + Send + Sync>,
    cancel: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

impl Ctx {
    pub fn new(log: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        Ctx {
            log,
            cancel: Arc::new(AtomicBool::new(false)),
            child: Arc::new(Mutex::new(None)),
        }
    }

    pub fn say(&self, s: impl AsRef<str>) {
        (self.log)(s.as_ref().to_string());
    }

    pub fn step(&self, titulo: &str) {
        self.say("");
        self.say(format!("=== {titulo}"));
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut g) = self.child.lock() {
            if let Some(c) = g.as_mut() {
                let _ = c.kill();
            }
        }
    }

    fn pump<R: Read + Send + 'static>(&self, r: R) -> JoinHandle<()> {
        let log = self.log.clone();
        thread::spawn(move || {
            for chunk in BufReader::new(r).split(b'\n') {
                if let Ok(b) = chunk {
                    let l = String::from_utf8_lossy(&b);
                    log(l.trim_end_matches('\r').to_string());
                }
            }
        })
    }

    /// Executa um programa transmitindo stdout+stderr para o log.
    pub fn run<P: AsRef<OsStr>>(
        &self,
        prog: P,
        args: &[String],
        cwd: Option<&Path>,
        check: bool,
    ) -> Result<i32> {
        if self.cancelled() {
            bail!("Cancelado pelo usuário.");
        }
        let prog = prog.as_ref();
        self.say(format!("  $ {} {}", prog.to_string_lossy(), args.join(" ")));

        let mut cmd = Command::new(prog);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(d) = cwd {
            cmd.current_dir(d);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd
            .spawn()
            .with_context(|| format!("não consegui executar {}", prog.to_string_lossy()))?;
        let out = child.stdout.take().expect("stdout");
        let err = child.stderr.take().expect("stderr");
        *self.child.lock().unwrap() = Some(child);
        let h1 = self.pump(out);
        let h2 = self.pump(err);

        let code = loop {
            let st = {
                let mut g = self.child.lock().unwrap();
                match g.as_mut() {
                    Some(c) => c.try_wait()?,
                    None => None,
                }
            };
            if let Some(s) = st {
                break s.code().unwrap_or(-1);
            }
            thread::sleep(Duration::from_millis(100));
        };
        *self.child.lock().unwrap() = None;
        let _ = h1.join();
        let _ = h2.join();

        if self.cancelled() {
            bail!("Cancelado pelo usuário.");
        }
        if check && code != 0 {
            bail!("comando falhou (código {code}): {}", prog.to_string_lossy());
        }
        Ok(code)
    }
}

// ------------------------------- utilitários -------------------------------

pub(crate) fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

pub(crate) fn which(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    let file = format!("{name}{EXE}");
    env::split_paths(&path)
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

fn request(url: &str) -> Result<ureq::Response> {
    ureq::get(url)
        .set("User-Agent", "ACBrInstall")
        .call()
        .map_err(|e| anyhow!("falha HTTP em {url}: {e}"))
}

pub(crate) fn http_bytes(url: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    request(url)?.into_reader().read_to_end(&mut buf)?;
    Ok(buf)
}

pub(crate) fn http_file(url: &str, dest: &Path) -> Result<()> {
    let mut r = request(url)?.into_reader();
    let mut f = fs::File::create(dest).with_context(|| format!("criando {}", dest.display()))?;
    io::copy(&mut r, &mut f)?;
    Ok(())
}
