//! process-stream is a thin wrapper around [`tokio::process`] to make it streamable
#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

use async_stream::stream;
use futures::Stream;
use io::Result;
use std::{
    ffi::OsStr,
    io,
    ops::{Deref, DerefMut},
    process::Stdio,
};
use tap::Pipe;
use {
    tokio::{
        io::{AsyncBufReadExt, AsyncRead, BufReader},
        process::Command,
        sync::mpsc::{channel, Sender},
    },
    tokio_stream::wrappers::LinesStream,
};

mod item;
pub use futures::StreamExt;
pub use futures::TryStreamExt;
pub use item::ProcessItem;

/// Thin Wrapper around [`Command`] to make it streamable
///
/// ## Example usage:
///
/// ### From `Vec<String>` or `Vec<&str>`
///
/// ```rust
/// use process_stream::Process;
/// use process_stream::StreamExt;
/// use std::io;
///
/// #[tokio::main]
/// async fn main() -> io::Result<()> {
///     let mut ls_home: Process = vec!["/bin/ls", "."].into();
///     let mut ls_stream = ls_home.stream()?;
///
///     while let Some(output) = ls_stream.next().await {
///         println!("{output}")
///     }
///
///     Ok(())
/// }
/// ```
///
/// ### New
///
/// ```rust
/// use process_stream::Process;
/// use process_stream::StreamExt;
/// use std::io;
///
/// #[tokio::main]
/// async fn main() -> io::Result<()> {
///     let mut ls_home = Process::new("/bin/ls");
///     ls_home.arg("~/");
///
///     let mut stream = ls_home.stream()?;
///
///     while let Some(output) = stream.next().await {
///         println!("{output}")
///     }
///
///     Ok(())
/// }
/// ```
pub struct Process {
    inner: Command,
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
    kill_send: Option<Sender<()>>,
}

impl Process {
    /// Create new process with a program
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            inner: Command::new(program),
            stdin: Some(Stdio::null()),
            stdout: Some(Stdio::piped()),
            stderr: Some(Stdio::piped()),
            kill_send: None,
        }
    }

    /// Spawn and stream [`Command`] outputs
    pub fn stream(&mut self) -> Result<impl Stream<Item = ProcessItem> + Send> {
        self.stdin.take().map(|out| self.inner.stdin(out));
        self.stdout.take().map(|out| self.inner.stdout(out));
        self.stderr.take().map(|out| self.inner.stderr(out));

        let mut child = self.inner.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

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
        self.stdin = stdin.into();
    }

    /// Kill Running process.
    /// returns false if the call is already made.
    pub async fn kill(&mut self) -> bool {
        match self.kill_send.take() {
            Some(tx) => {
                tx.send(()).await.ok();
                true
            }
            None => false,
        }
    }

    /// Set the process's stdout.
    pub fn set_stdout(&mut self, stdout: Stdio) {
        self.stdout = stdout.into();
    }

    /// Set the process's stderr.
    pub fn set_stderr(&mut self, stderr: Stdio) {
        self.stderr = stderr.into();
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
            stdout: None,
            stderr: None,
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

fn into_stream<R: AsyncRead>(out: R, is_stdout: bool) -> impl Stream<Item = ProcessItem> {
    out.pipe(BufReader::new)
        .lines()
        .pipe(LinesStream::new)
        .map(move |v| ProcessItem::from((is_stdout, v)))
}

#[cfg(test)]
mod tests {
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
        .stream()
        .unwrap();

        while let Some(output) = stream.next().await {
            println!("{output}")
        }
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

        let mut stream = process.stream()?;

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

        let mut stream = process.stream()?;
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
}
