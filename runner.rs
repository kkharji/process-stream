use crate::parser::{BuildSettings, Step};
use anyhow::Result;
use futures::{stream::Stream, stream::StreamExt};
use std::ffi;
use std::path::Path;
use std::process::Stdio;
use tap::Pipe;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio_stream::wrappers::LinesStream;

#[derive(Clone, Debug)]
pub enum ProcessUpdate {
    Stdout(String),
    Stderr(String),
    Exit(String),
    Error(String),
}

impl std::ops::Deref for ProcessUpdate {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        match self {
            ProcessUpdate::Stdout(s) => s,
            ProcessUpdate::Stderr(s) => s,
            ProcessUpdate::Exit(s) => s,
            ProcessUpdate::Error(s) => s,
        }
    }
}

fn get_readers(
    build: &mut tokio::process::Child,
) -> Result<(
    LinesStream<BufReader<ChildStdout>>,
    LinesStream<BufReader<ChildStderr>>,
)> {
    Ok((
        build
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("child did not have a handle to stdout"))?
            .pipe(BufReader::new)
            .lines()
            .pipe(LinesStream::new),
        build
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("child did not have a handle to stdout"))?
            .pipe(BufReader::new)
            .lines()
            .pipe(LinesStream::new),
    ))
}

/// Simple stream converter
async fn to_stream(mut child: Child) -> Result<impl Stream<Item = ProcessUpdate>> {
    let (stdout, stderr) = get_readers(&mut child)?;
    let stdout_reader = stdout.map(|line| match line {
        Ok(s) => ProcessUpdate::Stdout(s),
        Err(e) => ProcessUpdate::Error(format!("io: {e}")),
    });
    let stderr_reader = stderr.map(|line| match line {
        Ok(s) => ProcessUpdate::Stderr(s),
        Err(e) => ProcessUpdate::Error(format!("io: {e}")),
    });

    let exit_status = tokio::spawn(async move { child.wait().await });

    tokio_stream::StreamExt::merge(stdout_reader, stderr_reader)
        .chain(futures::stream::once(async {
            match exit_status.await {
                Ok(x) => match x {
                    Ok(x) => ProcessUpdate::Exit(x.code().unwrap_or(0).to_string()),
                    Err(e) => ProcessUpdate::Error(e.to_string()),
                },
                Err(e) => ProcessUpdate::Error(e.to_string()),
            }
        }))
        .boxed()
        .pipe(Ok)
}

#[allow(dead_code)]
async fn spawn_stream<P, I, S>(
    root: P,
    args: I,
) -> Result<(
    LinesStream<BufReader<ChildStdout>>,
    LinesStream<BufReader<ChildStderr>>,
)>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = S>,
    S: AsRef<ffi::OsStr>,
{
    let mut child = Command::new("/usr/bin/xcodebuild")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(root)
        .spawn()?;

    let readers = get_readers(&mut child)?;

    tokio::spawn(async move {
        let status = child.wait().await.unwrap();

        #[cfg(feature = "tracing")]
        tracing::info!("build status: {status}");
    });

    Ok(readers)
}

pub async fn spawn<P, I, S>(root: P, args: I) -> Result<impl Stream<Item = Step>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<ffi::OsStr>,
    P: AsRef<Path>,
{
    let mut reader = to_stream(
        Command::new("/usr/bin/xcodebuild")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(root)
            .spawn()?,
    )
    .await?;

    async_stream::stream! {
        while let Some(update) = reader.next().await {
            match update {
                ProcessUpdate::Stdout(line) => {
                    if !line.is_empty() {
                        match crate::parser::parse_step_from_stream(line, &mut reader).await {
                            Ok(v) => if let Some(step) = v { yield step; }
                            Err(e) => yield crate::parser::Step::Error(e.to_string())
                        }
                    }
                }
                ProcessUpdate::Exit(status) => yield crate::parser::Step::Exit(status),
                ProcessUpdate::Error(e) => yield crate::parser::Step::Error(e),
                ProcessUpdate::Stderr(e) => yield crate::parser::Step::Error(e),
            }
        }
    }
    .boxed()
    .pipe(Ok)
}

pub async fn run<X>(program: X) -> Result<impl Stream<Item = ProcessUpdate>>
where
    X: AsRef<ffi::OsStr>,
{
    to_stream(
        Command::new(program)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    )
    .await
}

pub async fn run_with_args<X, I, S>(
    program: X,
    args: I,
) -> Result<impl Stream<Item = ProcessUpdate>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<ffi::OsStr>,
    X: AsRef<ffi::OsStr>,
{
    to_stream(
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    )
    .await
}

pub async fn spawn_once<P, I, S>(root: P, args: I) -> Result<()>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = S>,
    S: AsRef<ffi::OsStr>,
{
    Command::new("/usr/bin/xcodebuild")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?
        .wait()
        .await?;
    Ok(())
}

pub async fn build_settings<P, I, S>(root: P, args: I) -> Result<BuildSettings>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = S>,
    S: AsRef<ffi::OsStr>,
{
    let output = Command::new("/usr/bin/xcodebuild")
        .args(args)
        .arg("-showBuildSettings")
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()
        .await?;

    if output.status.success() {
        BuildSettings::new(String::from_utf8(output.stdout)?.split("\n"))
    } else {
        anyhow::bail!(String::from_utf8(output.stderr)?)
    }
}

#[tokio::test]
#[tracing_test::traced_test]
async fn test_build_settings() {
    let root = "/Users/tami5/repos/swift/wordle";

    // spawn_once(root, &["clean"]).await.unwrap();

    let data = build_settings(
        root,
        &[
            "build",
            "-configuration",
            "Debug",
            "-target",
            "Wordle",
            "-showBuildSettings",
            "-sdk",
            "iphonesimulator",
        ],
    )
    .await
    .unwrap();

    tracing::info!("{:#?}", data);
}

// https://github.com/ThatAnnoyingKid/pikadick-rs/blob/cecd1a88882fe3c07a9f8c52e81a97ca6e5f013e/lib/tokio-ffmpeg-cli-rs/src/lib.rs
// https://github.com/zhaofengli/colmena/blob/09a8a72b0c5113aa40648949986278040487c9bd/src/nix/evaluator/nix_eval_jobs.rs
// https://github.com/ezclap-tv/shit-chat-says/blob/c34be8edd12ade50c04ea879403a1a5d8db745d4/scs-manage-api/src/v1.rs
// https://github.com/MrRobu/concurrent-and-distributed-computing/blob/7292cc1188b3a66cf26f756d40b47894fc1c631a/homework1/src/bin/rce-agent.rs
