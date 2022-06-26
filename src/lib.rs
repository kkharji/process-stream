//! process-stream is a thin wrapper around [`tokio::process`] to make it streamable
#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![doc = include_str!("../README.md")]

/// Alias for a stream of process items
pub type ProcessStream = Pin<Box<dyn Stream<Item = ProcessItem> + Send>>;

pub use async_stream::stream;
use io::Result;
use std::{
    ffi::OsStr,
    io,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    sync::Arc,
};
use tap::Pipe;
use {
    tokio::{
        io::{AsyncBufReadExt, AsyncRead, BufReader},
        process::{ChildStdin, Command},
        sync::Notify,
    },
    tokio_stream::wrappers::LinesStream,
};

mod item;
pub use async_trait::async_trait;
pub use futures::Stream;
pub use futures::StreamExt;
pub use futures::TryStreamExt;
pub use item::ProcessItem;
pub use tokio_stream;

#[async_trait]
/// ProcessExt trait that needs to be implemented to make something streamable
pub trait ProcessExt {
    /// Get command that will be used to create a child process from
    fn get_command(&mut self) -> &mut Command;

    /// Get command after settings the required pipes;
    fn command(&mut self) -> &mut Command {
        let stdin = self.get_stdin().take().unwrap();
        let stdout = self.get_stdout().take().unwrap();
        let stderr = self.get_stderr().take().unwrap();
        let command = self.get_command();

        command.stdin(stdin);
        command.stderr(stdout);
        command.stdout(stderr);
        command
    }

    /// Spawn and stream process
    fn spawn_and_stream(&mut self) -> Result<ProcessStream> {
        self._spawn_and_stream()
    }

    /// Spawn and stream process (avoid custom implementation, use spawn_and_stream instead)
    fn _spawn_and_stream(&mut self) -> Result<ProcessStream> {
        let abort = Arc::new(Notify::new());

        let mut child = self.command().spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        self.set_child_stdin(child.stdin.take());
        self.set_aborter(Some(abort.clone()));

        let stdout_stream = into_stream(stdout, true);
        let stderr_stream = into_stream(stderr, false);
        let mut std_stream = tokio_stream::StreamExt::merge(stdout_stream, stderr_stream);

        let stream = stream! {
            loop {
                use ProcessItem::*;
                tokio::select! {
                    Some(output) = std_stream.next() => yield output,
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
                    _ = abort.notified() => {
                        match child.start_kill() {
                            Ok(()) => yield Exit("0".into()),
                            Err(err) => yield Error(format!("abort Process Error: {err}")),
                        };
                        break;
                    }
                }
            }
        };

        Ok(stream.boxed())
    }
    /// Get a notifier that can be used to abort the process
    fn aborter(&self) -> Option<Arc<Notify>>;
    /// Set the notifier that should be used to abort the process
    fn set_aborter(&mut self, aborter: Option<Arc<Notify>>);
    /// Get process stdin
    fn take_stdin(&mut self) -> Option<ChildStdin> {
        None
    }
    /// Set process stdin
    fn set_child_stdin(&mut self, _child_stdin: Option<ChildStdin>) {}
    /// Get process stdin pipe
    fn get_stdin(&mut self) -> Option<Stdio> {
        Some(Stdio::null())
    }
    /// get process stdout pipe
    fn get_stdout(&mut self) -> Option<Stdio> {
        Some(Stdio::piped())
    }
    /// get process stderr pipe
    fn get_stderr(&mut self) -> Option<Stdio> {
        Some(Stdio::piped())
    }
}

/// Thin Wrapper around [`Command`] to make it streamable
pub struct Process {
    inner: Command,
    stdin: Option<ChildStdin>,
    set_stdin: Option<Stdio>,
    set_stdout: Option<Stdio>,
    set_stderr: Option<Stdio>,
    abort: Option<Arc<Notify>>,
}

impl ProcessExt for Process {
    fn get_command(&mut self) -> &mut Command {
        &mut self.inner
    }

    fn aborter(&self) -> Option<Arc<Notify>> {
        self.abort.clone()
    }

    fn set_aborter(&mut self, aborter: Option<Arc<Notify>>) {
        self.abort = aborter
    }

    fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.stdin.take()
    }

    fn set_child_stdin(&mut self, child_stdin: Option<ChildStdin>) {
        self.stdin = child_stdin;
    }

    fn get_stdin(&mut self) -> Option<Stdio> {
        self.set_stdin.take()
    }

    fn get_stdout(&mut self) -> Option<Stdio> {
        self.set_stdout.take()
    }

    fn get_stderr(&mut self) -> Option<Stdio> {
        self.set_stderr.take()
    }
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
            abort: None,
        }
    }

    /// Set the process's stdin.
    pub fn stdin(&mut self, stdin: Stdio) {
        self.set_stdin = stdin.into();
    }

    /// Set the process's stdout.
    pub fn stdout(&mut self, stdout: Stdio) {
        self.set_stdout = stdout.into();
    }

    /// Set the process's stderr.
    pub fn stderr(&mut self, stderr: Stdio) {
        self.set_stderr = stderr.into();
    }

    /// Abort the process
    pub fn abort(&self) {
        self.aborter().map(|k| k.notify_waiters());
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
    fn from(command: Command) -> Self {
        Self {
            inner: command,
            stdin: None,
            set_stdin: Some(Stdio::null()),
            set_stdout: Some(Stdio::piped()),
            set_stderr: Some(Stdio::piped()),
            abort: None,
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

/// Convert std_stream to a stream of T
pub fn into_stream<T, R>(std: R, is_stdout: bool) -> impl Stream<Item = T>
where
    T: From<(bool, Result<String>)>,
    R: AsyncRead,
{
    std.pipe(BufReader::new)
        .lines()
        .pipe(LinesStream::new)
        .map(move |v| T::from((is_stdout, v)))
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
    async fn test_abort() -> Result<()> {
        let mut process = Process::new("xcrun");

        process.args(&[
            "/Users/tami5/Library/Caches/Xbase/swift_Control/Debug/Control.app/Contents/MacOS/Control",
        ]);

        let mut stream = process.spawn_and_stream()?;
        tokio::spawn(async move {
            while let Some(output) = stream.next().await {
                println!("{output}")
            }
        });

        tokio::time::sleep(std::time::Duration::new(5, 0)).await;

        process.abort();

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
