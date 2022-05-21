//! process-stream is a thin wrapper around [`tokio::process`] to make process output streamable
#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

use futures::{
    stream::{once, Map, Once},
    Future, Stream,
};
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
        process::{Child, Command},
    },
    tokio_stream::wrappers::LinesStream,
};

mod item;
pub use futures::StreamExt;
pub use futures::TryStreamExt;
pub use item::ProcessItem;

/// Thin Wrapper around [`Command`] to make it streamable
pub struct Process {
    inner: Command,
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
}

impl Process {
    /// Create new process with a program
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            inner: Command::new(program),
            stdin: Some(Stdio::null()),
            stdout: Some(Stdio::piped()),
            stderr: Some(Stdio::piped()),
        }
    }

    /// Spawn and stream [`Command`] outputs
    pub fn stream(&mut self) -> Result<impl Stream<Item = ProcessItem> + Send> {
        self.inner.stdin(self.stdin.take().unwrap());
        self.inner.stdout(self.stdout.take().unwrap());
        self.inner.stderr(self.stderr.take().unwrap());

        let mut child = self.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdout_stream = into_stream(stdout, true);
        let stderr_stream = into_stream(stderr, false);
        let exit_stream = exit_stream(child);

        tokio_stream::StreamExt::merge(stdout_stream, stderr_stream)
            .chain(exit_stream)
            .boxed()
            .pipe(Ok)
    }

    /// Set the process's stdin.
    pub fn stdin(&mut self, stdin: Stdio) {
        self.stdin = stdin.into();
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

type Streamable<R, F> = Map<LinesStream<BufReader<R>>, F>;

fn into_stream<R: AsyncRead>(
    out: R,
    is_stdout: bool,
) -> Streamable<R, impl FnMut(io::Result<String>) -> ProcessItem> {
    out.pipe(BufReader::new)
        .lines()
        .pipe(LinesStream::new)
        .map(move |v| ProcessItem::from((is_stdout, v)))
}

fn exit_stream(mut child: Child) -> Once<impl Future<Output = ProcessItem>> {
    let exit_status = tokio::spawn(async move { child.wait().await });
    once(async {
        match exit_status.await {
            Err(err) => ProcessItem::Error(err.to_string()),
            Ok(Ok(status)) => {
                if let Some(code) = status.code() {
                    ProcessItem::Exit(code)
                } else {
                    ProcessItem::Error("Unable to get exit code".into())
                }
            }
            Ok(Err(err)) => ProcessItem::Error(err.to_string()),
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[tokio::test]
    async fn it_works() {
        let mut process = Process::new("xcrun");
        process.args(&[
            "simctl",
            "launch",
            "--terminate-running-process",
            "--console",
            "booted",
            "tami5.Wordle",
        ]);

        let mut stream = process.stream().unwrap();
        while let Some(output) = stream.next().await {
            println!("{output}")
        }
    }
}
