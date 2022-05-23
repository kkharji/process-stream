# process-stream

Wraps `tokio::process::Command` to `future::stream`.

## Install

```toml 
process-stream = "0.2.1"
```

## Example usage:

### From `Vec<String>` or `Vec<&str>`

```rust
use process_stream::Process;
use process_stream::StreamExt;
use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    let ls_home: Process = vec!["/bin/ls", "."].into();

    let mut stream = ls_home.spawn_and_stream()?;

    while let Some(output) = stream.next().await {
        println!("{output}")
    }

    Ok(())
}
```

### From `Path/PathBuf/str`

```rust
use process_stream::Process;
use process_stream::StreamExt;
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
use process_stream::Process;
use process_stream::StreamExt;
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
use process_stream::Process;
use process_stream::StreamExt;
use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut long_process = Process::new("/bin/app");

    let mut stream = long_process.spawn_and_stream()?;

    tokio::spawn(async move {
      while let Some(output) = stream.next().await {
        println!("{output}")
      }
    })

    // process some outputs
    tokio::time::sleep(std::time::Duration::new(10, 0)).await;

    // close the process
    long_process.kill().await;

    Ok(())
}
```
