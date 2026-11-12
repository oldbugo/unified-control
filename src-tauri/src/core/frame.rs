//! Newline-delimited, size-bounded framing shared by the supervised child
//! processes (cooler engine, dock worker, source worker). One frame is one line
//! of bytes ending in `\n`; each caller keeps its own limit and error texts.
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{BufRead, Read, Write},
    sync::{mpsc, Arc, Mutex},
};

/// Waiting RPC callers keyed by request id.
pub type Pending<E> = Arc<Mutex<HashMap<u64, mpsc::Sender<Result<Value, E>>>>>;

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The pipe could not be read or written.
    Io,
    /// The frame was larger than the limit, or ended without a newline.
    Limit,
}

/// Reads one frame (including its trailing newline). `Ok(None)` is a clean end
/// of stream; a frame over `max` bytes or without a final newline is `Limit`.
pub fn read_frame(reader: &mut impl BufRead, max: usize) -> Result<Option<Vec<u8>>, FrameError> {
    let mut bytes = Vec::new();
    let n = reader
        .take((max + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| FrameError::Io)?;
    if n == 0 {
        return Ok(None);
    }
    if n > max || bytes.last() != Some(&b'\n') {
        return Err(FrameError::Limit);
    }
    Ok(Some(bytes))
}

/// Writes `payload` plus a newline and flushes. A payload of `max` bytes or
/// more is refused before anything is written.
pub fn write_frame(writer: &mut impl Write, payload: &[u8], max: usize) -> Result<(), FrameError> {
    if payload.len() >= max {
        return Err(FrameError::Limit);
    }
    writer
        .write_all(payload)
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|_| FrameError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trip_and_clean_eof() {
        let mut wire = Vec::new();
        write_frame(&mut wire, b"{\"a\":1}", 64).unwrap();
        write_frame(&mut wire, b"[]", 64).unwrap();
        assert_eq!(wire, b"{\"a\":1}\n[]\n");
        let mut reader = Cursor::new(wire);
        assert_eq!(read_frame(&mut reader, 64).unwrap().unwrap(), b"{\"a\":1}\n");
        assert_eq!(read_frame(&mut reader, 64).unwrap().unwrap(), b"[]\n");
        assert_eq!(read_frame(&mut reader, 64), Ok(None));
    }

    #[test]
    fn missing_newline_is_a_limit_error() {
        assert_eq!(
            read_frame(&mut Cursor::new(b"{}".to_vec()), 64),
            Err(FrameError::Limit)
        );
    }

    #[test]
    fn oversize_frames_are_refused_on_both_sides() {
        // A frame of exactly `max` bytes (with newline) is accepted; one more is not.
        assert!(read_frame(&mut Cursor::new(b"abc\n".to_vec()), 4).is_ok());
        assert_eq!(
            read_frame(&mut Cursor::new(b"abcd\n".to_vec()), 4),
            Err(FrameError::Limit)
        );
        let mut wire = Vec::new();
        assert_eq!(write_frame(&mut wire, b"abcd", 4), Err(FrameError::Limit));
        assert!(wire.is_empty());
        assert!(write_frame(&mut wire, b"abc", 4).is_ok());
    }
}
