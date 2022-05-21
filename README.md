# process-stream

Wraps `tokio::process::Command` to `future::stream`.

## Install

```toml 
process-stream = "0.1.0"
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

    let mut stream = ls_home.stream()?;

    while let Some(output) = stream.next().await {
        println!("{output}")
    }

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

    let mut stream = ls_home.stream()?;

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

    let mut stream = long_process.stream()?;

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
