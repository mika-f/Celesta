//! Fault-injection relay for the real-process decode prefetch tests.

use std::io::{self, BufReader, BufWriter, Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

type Message = (Vec<u8>, Vec<u8>);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let codec = args.next().expect("test codec executable");
    let marker = args.next().expect("speculative request marker");
    let mode = args.next().expect("error or hang");
    let mut child = Command::new(codec)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut requests = BufWriter::new(child.stdin.take().unwrap());
    let mut answers = BufReader::new(child.stdout.take().unwrap());
    let mut count = 0;
    while let Some(message) = read_message(&mut input)? {
        let header: serde_json::Value = serde_json::from_slice(&message.0)?;
        if header["type"] == "decodeVideo" {
            count += 1;
        }
        if count == 4 {
            std::fs::write(&marker, [])?;
            if mode == "hang" {
                std::thread::sleep(Duration::from_secs(60));
            }
            let error = serde_json::json!({
                "type": "error", "code": "codec", "message": "speculative failure"
            });
            write_message(&mut output, &(serde_json::to_vec(&error)?, Vec::new()))?;
            count += 1;
            continue;
        }
        write_message(&mut requests, &message)?;
        let answer = read_message(&mut answers)?.expect("test codec answer");
        write_message(&mut output, &answer)?;
    }
    drop(requests);
    child.wait()?;
    Ok(())
}

fn read_message(reader: &mut impl Read) -> io::Result<Option<Message>> {
    let mut length = [0; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let mut header = vec![0; u32::from_le_bytes(length) as usize];
    reader.read_exact(&mut header)?;
    reader.read_exact(&mut length)?;
    let mut payload = vec![0; u32::from_le_bytes(length) as usize];
    reader.read_exact(&mut payload)?;
    Ok(Some((header, payload)))
}

fn write_message(writer: &mut impl Write, message: &Message) -> io::Result<()> {
    for part in [&message.0, &message.1] {
        writer.write_all(&u32::try_from(part.len()).unwrap().to_le_bytes())?;
        writer.write_all(part)?;
    }
    writer.flush()
}
