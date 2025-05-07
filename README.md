# process-stream

Wraps `tokio::process::Command` to `future::stream`.

This library provide ProcessExt to create your own custom process

## Install

```toml
process-stream = "0.3.1"
```

## Example usage:

### From `Vec<String>` or `Vec<&str>`

```rust
use process_stream::{Process, ProcessExt, StreamExt};
use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut ls_home: Process = vec!["/bin/ls", "."].into();

    let mut stream = ls_home.spawn_and_stream()?;

    while let Some(output) = stream.next().await {
        println!("{output}")
    }

    Ok(())
}
```

### From `Path/PathBuf/str`

```rust
use process_stream::{Process, ProcessExt, StreamExt};
use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut process: Process = "/bin/ls".into();

    // block until process completes
    let outputs = process.spawn_and_stream()?.collect::<Vec<_>>().await;

    println!("{outputs:#?}");

    Ok(())
}
```

### New

```rust
use process_stream::{Process, ProcessExt, StreamExt};
use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut ls_home = Process::new("/bin/ls");
    ls_home.arg("~/");

    let mut stream = ls_home.spawn_and_stream()?;

    while let Some(output) = stream.next().await {
        println!("{output}")
    }

    Ok(())
}
```

### Kill

```rust
use process_stream::{Process, ProcessExt, StreamExt};
use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut long_process = Process::new("cat");

    let mut stream = long_process.spawn_and_stream()?;

    tokio::spawn(async move {
      while let Some(output) = stream.next().await {
        println!("{output}")
      }
    });

    // process some outputs
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // close the process
    long_process.abort();

    Ok(())
}
```

### Communicate with running process
```rust
use process_stream::{Process, ProcessExt, StreamExt};
use tokio::io::AsyncWriteExt;
use std::process::Stdio;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut process: Process = Process::new("sort");

    // Set stdin (by default is set to null)
    process.stdin(Stdio::piped());

    // Get Stream;
    let mut stream = process.spawn_and_stream().unwrap();

    // Get writer from stdin;
    let mut writer = process.take_stdin().unwrap();

    // Spawn new async task and move stream to it
    let reader_thread = tokio::spawn(async move {
        while let Some(output) = stream.next().await {
            if output.is_exit() {
                println!("DONE")
            } else {
                println!("{output}")
            }
        }
    });

    // Spawn new async task and move writer to it
    let writer_thread = tokio::spawn(async move {
        writer.write(b"b\nc\na\n").await.unwrap();
        writer.write(b"f\ne\nd\n").await.unwrap();
    });

    // Wait till all threads finish
    writer_thread.await.unwrap();
    reader_thread.await.unwrap();

    // Result
    // a
    // b
    // c
    // d
    // e
    // f
    // DONE
    Ok(())
}
```
