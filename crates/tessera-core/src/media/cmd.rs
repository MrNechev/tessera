//! Running external tools (ffmpeg, ffprobe, vips) from blocking job threads: cancellation,
//! timeouts, `nice` for background work, and Go-style error messages.
//!
//! The functions here must be called from a blocking context (a job worker thread or
//! `spawn_blocking`), never directly from async code: they drive the child process on the
//! Tokio runtime with `Handle::block_on`.

use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

use crate::util::first_line;

/// Execution context for one external command.
#[derive(Clone)]
pub struct Exec {
    pub rt: Handle,
    pub cancel: CancellationToken,
    /// The whole job's deadline (Go: a context with timeout shared by its commands).
    pub deadline: Instant,
    /// Run under `nice -n 10` (background work).
    pub nice: bool,
}

/// Why a command did not succeed.
#[derive(Debug)]
pub enum CmdError {
    /// The job was cancelled (Go: "context canceled").
    Cancelled,
    /// The timeout passed (Go: "context deadline exceeded").
    Timeout,
    /// Could not start the program.
    Start(std::io::Error),
    /// Non-zero exit; stderr (and stdout for combined output) captured.
    Exit {
        status: ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
}

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CmdError::Cancelled => f.write_str("context canceled"),
            CmdError::Timeout => f.write_str("context deadline exceeded"),
            CmdError::Start(e) => write!(f, "{e}"),
            CmdError::Exit { status, .. } => f.write_str(&exit_string(status)),
        }
    }
}

impl std::error::Error for CmdError {}

/// Go's `ExitError.Error()`: "exit status 1" or "signal: killed".
pub fn exit_string(st: &ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    if let Some(c) = st.code() {
        return format!("exit status {c}");
    }
    if let Some(sig) = st.signal() {
        let name = match sig {
            libc::SIGKILL => "killed",
            libc::SIGTERM => "terminated",
            libc::SIGSEGV => "segmentation fault",
            libc::SIGABRT => "aborted",
            libc::SIGINT => "interrupt",
            libc::SIGBUS => "bus error",
            libc::SIGPIPE => "broken pipe",
            _ => return format!("signal: {sig}"),
        };
        return format!("signal: {name}");
    }
    "exit status unknown".into()
}

fn command(nice: bool, name: &str, args: &[String]) -> tokio::process::Command {
    let mut c = if nice && crate::util::look_path("nice").is_some() {
        let mut c = tokio::process::Command::new("nice");
        c.arg("-n").arg("10").arg(name);
        c
    } else {
        tokio::process::Command::new(name)
    };
    c.args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    c
}

impl Exec {
    pub fn new(rt: Handle, cancel: CancellationToken, timeout: Duration, nice: bool) -> Exec {
        Exec {
            rt,
            cancel,
            deadline: Instant::now() + timeout,
            nice,
        }
    }

    /// Runs a command and captures stdout and stderr.
    pub fn output(&self, name: &str, args: &[String]) -> Result<(Vec<u8>, Vec<u8>), CmdError> {
        if self.cancel.is_cancelled() {
            return Err(CmdError::Cancelled);
        }
        let mut cmd = command(self.nice, name, args);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let cancel = self.cancel.clone();
        let deadline = tokio::time::Instant::from_std(self.deadline);
        self.rt.block_on(async move {
            let child = cmd.spawn().map_err(CmdError::Start)?;
            tokio::select! {
                out = child.wait_with_output() => {
                    let out = out.map_err(CmdError::Start)?;
                    if out.status.success() {
                        Ok((out.stdout, out.stderr))
                    } else {
                        Err(CmdError::Exit { status: out.status, stdout: out.stdout, stderr: out.stderr })
                    }
                }
                _ = cancel.cancelled() => Err(CmdError::Cancelled),
                _ = tokio::time::sleep_until(deadline) => Err(CmdError::Timeout),
            }
        })
    }

    /// Like Go's `run`: runs a command, and on failure returns
    /// "<name>: exit status N: <first line of the combined output>".
    pub fn run(&self, name: &str, args: &[String]) -> anyhow::Result<()> {
        match self.output(name, args) {
            Ok(_) => Ok(()),
            Err(CmdError::Exit {
                status,
                stdout,
                stderr,
            }) => {
                let mut combined = stdout;
                combined.extend_from_slice(&stderr);
                Err(anyhow!(
                    "{name}: {}: {}",
                    exit_string(&status),
                    first_line(&String::from_utf8_lossy(&combined))
                ))
            }
            Err(e @ (CmdError::Cancelled | CmdError::Timeout)) => Err(anyhow!(e)),
            Err(CmdError::Start(e)) => Err(anyhow!("{name}: {e}: ")),
        }
    }

    /// Runs a command, calling `on_line` for every stdout line (ffmpeg `-progress pipe:1`).
    /// Stderr is kept (first 4 KiB) for the error message.
    pub fn run_lines(
        &self,
        name: &str,
        args: &[String],
        mut on_line: impl FnMut(&str) + Send,
    ) -> anyhow::Result<()> {
        if self.cancel.is_cancelled() {
            return Err(anyhow!(CmdError::Cancelled));
        }
        let mut cmd = command(self.nice, name, args);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let cancel = self.cancel.clone();
        let deadline = tokio::time::Instant::from_std(self.deadline);
        let res: Result<(), CmdError> = self.rt.block_on(async move {
            let mut child = cmd.spawn().map_err(CmdError::Start)?;
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let err_task = tokio::spawn(async move {
                let mut buf = Vec::new();
                if let Some(mut e) = stderr {
                    let mut chunk = [0u8; 8192];
                    loop {
                        match e.read(&mut chunk).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if buf.len() < 4096 {
                                    let take = n.min(4096 - buf.len());
                                    buf.extend_from_slice(&chunk[..take]);
                                }
                            }
                        }
                    }
                }
                buf
            });
            let work = async {
                if let Some(out) = stdout {
                    let mut lines = tokio::io::BufReader::new(out).lines();
                    while let Ok(Some(l)) = lines.next_line().await {
                        on_line(&l);
                    }
                }
                child.wait().await
            };
            let status = tokio::select! {
                st = work => st.map_err(CmdError::Start)?,
                _ = cancel.cancelled() => return Err(CmdError::Cancelled),
                _ = tokio::time::sleep_until(deadline) => return Err(CmdError::Timeout),
            };
            let stderr = err_task.await.unwrap_or_default();
            if status.success() {
                Ok(())
            } else {
                Err(CmdError::Exit {
                    status,
                    stdout: Vec::new(),
                    stderr,
                })
            }
        });
        match res {
            Ok(()) => Ok(()),
            Err(CmdError::Exit { status, stderr, .. }) => Err(anyhow!(
                "{name}: {}: {}",
                exit_string(&status),
                first_line(&String::from_utf8_lossy(&stderr))
            )),
            Err(e) => Err(anyhow!(e)),
        }
    }
}

/// Whether an error came from cancellation (pool shutdown) or a timeout.
pub fn is_cancelled(e: &anyhow::Error) -> bool {
    matches!(
        e.downcast_ref::<CmdError>(),
        Some(CmdError::Cancelled | CmdError::Timeout)
    )
}
