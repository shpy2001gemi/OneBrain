//! Fixed SSH bridge for the P5 V2 agent socket.

#[cfg(unix)]
use std::io::{Read, Write};

#[cfg(unix)]
const SOCKET: &str = "/run/onebrain/p5-v2/agent.sock";
#[cfg(unix)]
const MAX_FRAME: usize = 65_536;
#[cfg(unix)]
const FRAME_WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
// Reservation admission can legitimately use a shared 20-second deadline for
// two or three relays.  The local SSH bridge must outlive that bounded command
// without making an unbounded Unix-socket read possible.
#[cfg(unix)]
const RESPONSE_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn main() {
    if std::env::args().len() != 1 {
        eprintln!("p5_agent_ctl_v2 accepts no arguments");
        std::process::exit(2);
    }
    if let Err(error) = bridge() {
        eprintln!("P5 V2 bridge failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(unix)]
fn bridge() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    use std::os::unix::net::UnixStream;
    let parent = std::fs::symlink_metadata("/run/onebrain/p5-v2")?;
    let socket = std::fs::symlink_metadata(SOCKET)?;
    if parent.uid() != 0
        || parent.mode() & 0o022 != 0
        || socket.file_type().is_symlink()
        || !socket.file_type().is_socket()
        || socket.uid() != 0
        || socket.mode() & 0o002 != 0
    {
        return Err("agent socket authority check failed".into());
    }
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    loop {
        let Some(frame) = read_frame(&mut input)? else {
            break;
        };
        let mut stream = UnixStream::connect(SOCKET)?;
        stream.set_read_timeout(Some(RESPONSE_READ_TIMEOUT))?;
        stream.set_write_timeout(Some(FRAME_WRITE_TIMEOUT))?;
        stream.write_all(&(frame.len() as u32).to_be_bytes())?;
        stream.write_all(&frame)?;
        let response = read_frame(&mut stream)?.ok_or("agent closed before response")?;
        output.write_all(&(response.len() as u32).to_be_bytes())?;
        output.write_all(&response)?;
        output.flush()?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn bridge() -> Result<(), Box<dyn std::error::Error>> {
    Err("P5 V2 bridge requires Unix".into())
}

#[cfg(unix)]
fn read_frame(input: &mut impl Read) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    let mut length = [0u8; 4];
    match input.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err("frame outside fixed bound".into());
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    Ok(Some(bytes))
}

#[cfg(all(test, unix))]
mod tests {
    use super::{FRAME_WRITE_TIMEOUT, RESPONSE_READ_TIMEOUT};
    use std::time::Duration;

    #[test]
    fn response_budget_covers_the_bounded_reservation_window() {
        assert_eq!(FRAME_WRITE_TIMEOUT, Duration::from_secs(5));
        assert!(RESPONSE_READ_TIMEOUT >= Duration::from_secs(20));
        assert!(RESPONSE_READ_TIMEOUT <= Duration::from_secs(30));
    }
}
