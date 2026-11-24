#![forbid(unsafe_code)]

//! Bounded length-prefixed JSON framing and child-process transport shared by
//! persistent Marqdo workers.
//!
//! The crate owns only what is genuinely common to a plugin worker and a
//! resource-provider worker: the four-byte big-endian length prefix, the
//! maximum frame size, clean-EOF versus truncated-frame classification, a
//! single reader and writer thread, request deadlines, and process teardown.
//! Each caller still owns its own frame vocabulary, size limit, authority,
//! capability bridge and error codes. Sharing this transport does not share
//! capabilities or trust.
//!
//! Foreign kernels and CLI Tools do not use this transport: a foreign kernel
//! is a one-shot stdin/stdout wire value, and a CLI Tool is an independent
//! executable owning its standard streams.

use std::io::{self, Read, Write};
use std::process::Child;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::{Duration, Instant};

use serde_json::Value;
use thiserror::Error;

/// Read a bounded four-byte big-endian length and JSON value.
#[derive(Debug, Error)]
pub enum FrameError {
    #[error("worker frame I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("worker stream ended inside a frame header")]
    PartialHeader,
    #[error("worker frame length {0} is invalid")]
    Length(usize),
    #[error("worker frame declared {expected} bytes but only {actual} arrived")]
    PartialPayload { expected: usize, actual: usize },
    #[error("worker frame is not valid JSON: {0}")]
    InvalidJson(serde_json::Error),
    #[error("cannot encode worker frame: {0}")]
    Encode(serde_json::Error),
}

/// Read one bounded frame.
///
/// `Ok(None)` is a clean EOF at a frame boundary. A partial header, a
/// truncated payload, an invalid length or invalid JSON is a protocol error and
/// never EOF. `limit` is the caller's protocol frame budget, in bytes.
pub fn read_frame(reader: &mut impl Read, limit: usize) -> Result<Option<Value>, FrameError> {
    let mut header = [0; 4];
    let mut offset = 0;
    while offset < header.len() {
        match reader.read(&mut header[offset..]) {
            Ok(0) if offset == 0 => return Ok(None),
            Ok(0) => return Err(FrameError::PartialHeader),
            Ok(count) => offset += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let expected = u32::from_be_bytes(header) as usize;
    if expected == 0 || expected > limit {
        return Err(FrameError::Length(expected));
    }
    let mut payload = Vec::with_capacity(expected.min(64 * 1024));
    let actual = reader.take(expected as u64).read_to_end(&mut payload)?;
    if actual != expected {
        return Err(FrameError::PartialPayload { expected, actual });
    }
    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(FrameError::InvalidJson)
}

/// Write and flush one frame under the peer's configured size limit.
pub fn write_frame(writer: &mut impl Write, value: &Value, limit: usize) -> Result<(), FrameError> {
    let payload = serde_json::to_vec(value).map_err(FrameError::Encode)?;
    if payload.len() > limit {
        return Err(FrameError::Length(payload.len()));
    }
    let length = u32::try_from(payload.len()).map_err(|_| FrameError::Length(payload.len()))?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

/// Failure of one configured request/response exchange.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("worker transport reached its deadline")]
    Timeout,
    #[error("worker transport closed")]
    Closed,
    #[error(transparent)]
    Frame(#[from] FrameError),
}

type WriteRequest = (Value, SyncSender<Result<(), FrameError>>);

/// Owns one configured child and its two dedicated protocol pipes.
///
/// A connection is sequential: one caller drives one exchange at a time, and
/// the caller supplies the deadline for that exchange. Any transport error
/// kills and reaps the child, so a dead worker is never reused.
pub struct Connection {
    child: Option<Child>,
    writes: Option<SyncSender<WriteRequest>>,
    reads: Option<Receiver<Result<Value, FrameError>>>,
}

impl Connection {
    /// Attach to a child with piped stdin/stdout, closing the child on failure.
    pub fn attach(child: Child, limit: usize) -> io::Result<Self> {
        let mut connection = Self {
            child: Some(child),
            writes: None,
            reads: None,
        };
        if let Err(error) = connection.attach_pipes(limit) {
            connection.abort();
            return Err(error);
        }
        Ok(connection)
    }

    fn attach_pipes(&mut self, limit: usize) -> io::Result<()> {
        let instance = self.child.as_mut().expect("child present");
        let mut stdin = instance
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("worker stdin unavailable"))?;
        let mut stdout = instance
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("worker stdout unavailable"))?;

        let (writes, pending) = mpsc::sync_channel::<WriteRequest>(1);
        std::thread::Builder::new()
            .name("marqdo-worker-writer".into())
            .spawn(move || {
                while let Ok((value, reply)) = pending.recv() {
                    if reply.send(write_frame(&mut stdin, &value, limit)).is_err() {
                        break;
                    }
                }
            })?;
        self.writes = Some(writes);

        let (received, reads) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("marqdo-worker-reader".into())
            .spawn(move || loop {
                match read_frame(&mut stdout, limit) {
                    Ok(Some(value)) => {
                        if received.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = received.send(Err(error));
                        break;
                    }
                }
            })?;
        self.reads = Some(reads);
        Ok(())
    }

    /// Process id of the attached child, if it is still owned here.
    pub fn id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    /// Send one frame and wait for the write to complete before the deadline.
    pub fn send(&mut self, value: Value, deadline: Instant) -> Result<(), TransportError> {
        let result = (|| {
            let channel = self.writes.as_ref().ok_or(TransportError::Closed)?;
            let (response, result) = mpsc::sync_channel(1);
            let mut pending = Some((value, response));
            loop {
                match channel.try_send(pending.take().expect("pending frame")) {
                    Ok(()) => break,
                    Err(TrySendError::Full(returned)) => {
                        pending = Some(returned);
                        if Instant::now() >= deadline {
                            return Err(TransportError::Timeout);
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(TrySendError::Disconnected(_)) => return Err(TransportError::Closed),
                }
            }
            wait(&result, deadline)?.map_err(Into::into)
        })();
        if result.is_err() {
            self.abort();
        }
        result
    }

    /// Receive one frame before the deadline.
    pub fn receive(&mut self, deadline: Instant) -> Result<Value, TransportError> {
        let result = self
            .reads
            .as_ref()
            .ok_or(TransportError::Closed)
            .and_then(|channel| wait(channel, deadline))
            .and_then(|frame| frame.map_err(Into::into));
        if result.is_err() {
            self.abort();
        }
        result
    }

    /// Make this peer unusable and reap the process.
    pub fn abort(&mut self) {
        self.writes.take();
        self.reads.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.abort();
    }
}

fn wait<T>(channel: &Receiver<T>, deadline: Instant) -> Result<T, TransportError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(TransportError::Timeout);
    }
    channel
        .recv_timeout(remaining)
        .map_err(|error| match error {
            RecvTimeoutError::Timeout => TransportError::Timeout,
            RecvTimeoutError::Disconnected => TransportError::Closed,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn framing_distinguishes_boundary_eof_from_a_partial_record() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &json!({"ok": true}), 64).unwrap();
        let mut input = bytes.as_slice();
        assert_eq!(
            read_frame(&mut input, 64).unwrap(),
            Some(json!({"ok": true}))
        );
        assert_eq!(read_frame(&mut input, 64).unwrap(), None);
        assert!(matches!(
            read_frame(&mut &bytes[..3], 64),
            Err(FrameError::PartialHeader)
        ));
        assert!(matches!(
            read_frame(&mut &bytes[..6], 64),
            Err(FrameError::PartialPayload { .. })
        ));
        assert!(matches!(
            read_frame(&mut &[0; 4][..], 64),
            Err(FrameError::Length(0))
        ));
        assert!(matches!(
            read_frame(&mut &bytes[..4], 4),
            Err(FrameError::Length(_))
        ));
    }

    #[test]
    fn writing_past_the_peer_limit_is_a_frame_error() {
        let mut bytes = Vec::new();
        let error = write_frame(&mut bytes, &json!({"payload": "long"}), 4).unwrap_err();
        assert!(matches!(error, FrameError::Length(_)));
        assert!(bytes.is_empty());
    }

    #[cfg(unix)]
    mod child {
        use super::*;
        use std::process::{Command, Stdio};

        fn connection(script: &str) -> Connection {
            let child = Command::new("python3")
                .arg("-c")
                .arg(script)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("python3 is required for worker transport tests");
            Connection::attach(child, 1024).expect("attach child")
        }

        const ECHO: &str = r#"
import sys, json, struct
def read_frame():
    header = sys.stdin.buffer.read(4)
    if len(header) < 4:
        return None
    return json.loads(sys.stdin.buffer.read(struct.unpack(">I", header)[0]))
def write_frame(value):
    payload = json.dumps(value).encode()
    sys.stdout.buffer.write(struct.pack(">I", len(payload)) + payload)
    sys.stdout.buffer.flush()
while True:
    frame = read_frame()
    if frame is None:
        break
    write_frame({"echo": frame})
"#;

        #[test]
        fn connection_round_trips_frames_from_one_child() {
            let mut connection = connection(ECHO);
            let deadline = Instant::now() + Duration::from_secs(10);
            connection.send(json!({"n": 1}), deadline).unwrap();
            assert_eq!(
                connection.receive(deadline).unwrap(),
                json!({"echo": {"n": 1}})
            );
            connection.send(json!({"n": 2}), deadline).unwrap();
            assert_eq!(
                connection.receive(deadline).unwrap(),
                json!({"echo": {"n": 2}})
            );
        }

        #[test]
        fn a_silent_child_hits_the_request_deadline_and_is_reaped() {
            let mut connection = connection("import time; time.sleep(30)");
            let deadline = Instant::now() + Duration::from_millis(200);
            assert!(matches!(
                connection.receive(deadline),
                Err(TransportError::Timeout)
            ));
            assert!(connection.id().is_none());
            assert!(matches!(
                connection.receive(Instant::now() + Duration::from_secs(1)),
                Err(TransportError::Closed)
            ));
        }

        #[test]
        fn clean_eof_is_closed_and_a_truncated_frame_is_a_frame_error() {
            let mut silent = connection("pass");
            assert!(matches!(
                silent.receive(Instant::now() + Duration::from_secs(10)),
                Err(TransportError::Closed)
            ));

            let mut truncated = connection(
                "import sys, struct; sys.stdout.buffer.write(struct.pack('>I', 10) + b'{}'); sys.stdout.buffer.flush()",
            );
            assert!(matches!(
                truncated.receive(Instant::now() + Duration::from_secs(10)),
                Err(TransportError::Frame(FrameError::PartialPayload { .. }))
            ));
        }
    }
}
