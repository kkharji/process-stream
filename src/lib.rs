//! process-stream is a thin wrapper around [`tokio::process`] to make it streamable
#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![doc = include_str!("../README.md")]

use async_stream::stream;
use io::Result;
use std::{
    ffi::OsStr,
    io,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    process::Stdio,
};
use tap::Pipe;
use {
    tokio::{
        io::{AsyncBufReadExt, AsyncRead, BufReader},
        process::{ChildStdin, Command},
        sync::mpsc::{channel, Sender},
    },
    tokio_stream::wrappers::LinesStream,
};

mod item;
pub use futures::Stream;
pub use futures::StreamExt;
pub use futures::TryStreamExt;
pub use item::ProcessItem;

/// Thin Wrapper around [`Command`] to make it streamable
pub struct Process {
    inner: Command,
    stdin: Option<ChildStdin>,
    set_stdin: Option<Stdio>,
    set_stdout: Option<Stdio>,
    set_stderr: Option<Stdio>,
    kill_send: Option<Sender<()>>,
}

impl Process {
    /// Create new process with a program
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            inner: Command::new(program),
            set_stdin: Some(Stdio::null()),
            set_stdout: Some(Stdio::piped()),
            set_stderr: Some(Stdio::piped()),
            stdin: None,
            kill_send: None,
        }
    }

    #[deprecated(since = "0.2", note = "use Process::spawn_and_stream instread")]
    /// Spawn and stream [`Command`] outputs
    pub fn stream(&mut self) -> Result<impl Stream<Item = ProcessItem> + Send> {
        self.spawn_and_stream()
    }

    /// Spawn and stream [`Command`] outputs
    pub fn spawn_and_stream(&mut self) -> Result<impl Stream<Item = ProcessItem> + Send> {
        self.set_stdin.take().map(|out| self.inner.stdin(out));
        self.set_stdout.take().map(|out| self.inner.stdout(out));
        self.set_stderr.take().map(|out| self.inner.stderr(out));

        let mut child = self.inner.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        self.stdin = child.stdin.take();

        let (kill_send, mut kill_recv) = channel::<()>(1);
        self.kill_send = kill_send.into();

        let stdout_stream = into_stream(stdout, true);
        let stderr_stream = into_stream(stderr, false);
        let mut out = tokio_stream::StreamExt::merge(stdout_stream, stderr_stream);

        let stream = stream! {
            loop {
                use ProcessItem::*;
                tokio::select! {
                    Some(out) = out.next() => yield out,
                    status = child.wait() => match status {
                        Err(err) => yield Error(err.to_string()),
                        Ok(status) => {
                            match status.code() {
                                Some(code) => yield Exit(format!("{code}")),
                                None => yield Error("Unable to get exit code".into()),
                            }
                            break;

                        }
                    },
                    Some(_) = kill_recv.recv() => {
                        match child.start_kill() {
                            Ok(()) => yield Exit("0".into()),
                            Err(err) => yield Error(format!("Kill Process Error: {err}")),
                        };
                        break;
                    }
                }
            }
        };

        Ok(stream.boxed())
    }

    /// Set the process's stdin.
    pub fn stdin(&mut self, stdin: Stdio) {
        self.set_stdin = stdin.into();
    }

    /// Kill Running process.
    /// returns false if the call is already made.
    pub async fn kill(&self) -> bool {
        if let Some(tx) = &self.kill_send {
            tx.send(()).await.is_ok()
        } else {
            false
        }
    }

    /// Get an owned instance of kill single sender.
    ///
    /// This Convinent to be able to kill process from different threads.
    ///
    /// Returns None if process is already killed
    pub fn clone_kill_sender(&self) -> Option<Sender<()>> {
        self.kill_send.clone()
    }

    /// Set the process's stdout.
    pub fn stdout(&mut self, stdout: Stdio) {
        self.set_stdout = stdout.into();
    }

    /// Set the process's stderr.
    pub fn stderr(&mut self, stderr: Stdio) {
        self.set_stderr = stderr.into();
    }

    /// Get a reference to the process's stdin.
    #[must_use]
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.stdin.take()
    }
}

impl Deref for Process {
    type Target = Command;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Process {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<Command> for Process {
    fn from(mut command: Command) -> Self {
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(Stdio::null());
        Self {
            inner: command,
            stdin: None,
            set_stdin: None,
            set_stdout: None,
            set_stderr: None,
            kill_send: None,
        }
    }
}

impl<S: AsRef<OsStr>> From<Vec<S>> for Process {
    fn from(mut command_args: Vec<S>) -> Self {
        let command = command_args.remove(0);
        let mut inner = Command::new(command);
        inner.args(command_args);

        Self::from(inner)
    }
}

impl From<&Path> for Process {
    fn from(path: &Path) -> Self {
        let command = Command::new(path);
        Self::from(command)
    }
}

impl From<&str> for Process {
    fn from(path: &str) -> Self {
        let command = Command::new(path);
        Self::from(command)
    }
}

impl From<&PathBuf> for Process {
    fn from(path: &PathBuf) -> Self {
        let command = Command::new(path);
        Self::from(command)
    }
}

fn into_stream<R: AsyncRead>(out: R, is_stdout: bool) -> impl Stream<Item = ProcessItem> {
    out.pipe(BufReader::new)
        .lines()
        .pipe(LinesStream::new)
        .map(move |v| ProcessItem::from((is_stdout, v)))
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt;

    use crate::*;
    use std::io::Result;

    #[tokio::test]
    async fn test_from_vector() -> Result<()> {
        let mut stream = Process::from(vec![
            "xcrun",
            "simctl",
            "launch",
            "--terminate-running-process",
            "--console",
            "booted",
            "tami5.Wordle",
        ])
        .spawn_and_stream()
        .unwrap();

        while let Some(output) = stream.next().await {
            println!("{output}")
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_from_path() -> Result<()> {
        let mut process: Process = "/bin/ls".into();

        let outputs = process.spawn_and_stream()?.collect::<Vec<_>>().await;
        println!("{outputs:#?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_new() -> Result<()> {
        let mut process = Process::new("xcrun");

        process.args(&[
            "simctl",
            "launch",
            "--terminate-running-process",
            "--console",
            "booted",
            "tami5.Wordle",
        ]);

        let mut stream = process.spawn_and_stream()?;

        while let Some(output) = stream.next().await {
            println!("{output:#?}")
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_kill() -> Result<()> {
        let mut process = Process::new("xcrun");

        process.args(&[
            "/Users/tami5/Library/Caches/Xbase/swift/Control/Control.app/Contents/MacOS/Control",
        ]);

        let mut stream = process.spawn_and_stream()?;
        tokio::spawn(async move {
            while let Some(output) = stream.next().await {
                println!("{output}")
            }
        });

        tokio::time::sleep(std::time::Duration::new(5, 0)).await;

        process.kill().await;

        tokio::time::sleep(std::time::Duration::new(5, 0)).await;

        Ok(())
    }

    #[tokio::test]
    async fn test_dref_item_as_str() {
        use ProcessItem::*;
        let items = vec![
            Output("Hello".into()),
            Error("XXXXXXXXXX".into()),
            Exit("0".into()),
        ];
        for item in items {
            println!("{:?}", item.as_bytes())
        }
    }

    #[tokio::test]
    async fn communicate_with_running_process() -> Result<()> {
        let mut process: Process = Process::new("sort");

        // Set stdin (by default is set to null)
        process.stdin(Stdio::piped());

        // Get Stream;
        let mut stream = process.spawn_and_stream().unwrap();

        // Get writer from stdin;
        let mut writer = process.take_stdin().unwrap();

        // Start new running process
        let reader_thread = tokio::spawn(async move {
            while let Some(output) = stream.next().await {
                if output.is_exit() {
                    println!("DONE")
                } else {
                    println!("{output}")
                }
            }
        });

        let writer_thread = tokio::spawn(async move {
            writer.write(b"b\nc\na\n").await.unwrap();
            writer.write(b"f\ne\nd\n").await.unwrap();
        });

        writer_thread.await?;
        reader_thread.await?;

        Ok(())
    }
}
