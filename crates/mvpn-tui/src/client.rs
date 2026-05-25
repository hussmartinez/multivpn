use anyhow::{Context, Result};
use mvpn_core::ipc::{self, Request, Response, SOCKET_PATH};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

pub fn send(request: &Request) -> Result<Response> {
    let mut stream = UnixStream::connect(SOCKET_PATH).with_context(|| {
        format!("cannot connect to mvpn-daemon at {SOCKET_PATH}; try: sudo systemctl start multivpn")
    })?;

    let encoded = ipc::encode(request)?;
    stream.write_all(encoded.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    ipc::decode_response(&line)
}
