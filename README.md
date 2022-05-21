# process-stream

Wraps `tokio::process::Command` to `future::stream`.

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
