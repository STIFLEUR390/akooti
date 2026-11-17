//! Length-prefixed JSON framing.
//!
//! One frame is a 4-byte little-endian length followed by exactly that many
//! bytes of UTF-8 JSON — the same shape as the Native Messaging framing, so the
//! browser bridge and the daemon share one mental model.

use std::io::{Read, Write};

use crate::IpcError;

/// Hard cap on one frame (1 MiB), matching the Native Messaging limit.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Writes one frame and flushes it.
///
/// # Errors
///
/// Returns [`IpcError::Protocol`] when the payload is larger than
/// [`MAX_FRAME_BYTES`], and [`IpcError::Io`] when the writer fails.
pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), IpcError> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(IpcError::Protocol);
    }
    let length = u32::try_from(payload.len()).map_err(|_| IpcError::Protocol)?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}

/// Reads exactly one frame.
///
/// # Errors
///
/// Returns [`IpcError::Protocol`] for a declared length above
/// [`MAX_FRAME_BYTES`] and [`IpcError::Io`] for a truncated stream.
pub fn read_frame<R: Read>(reader: &mut R) -> Result<Vec<u8>, IpcError> {
    let mut header = [0u8; 4];
    reader.read_exact(&mut header)?;
    let declared = u32::from_le_bytes(header);
    let length = usize::try_from(declared).map_err(|_| IpcError::Protocol)?;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(IpcError::Protocol);
    }
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::{MAX_FRAME_BYTES, read_frame, write_frame};
    use crate::IpcError;

    #[test]
    fn round_trips_one_frame() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, b"{\"hello\":true}").expect("write");
        assert_eq!(&buffer[..4], &(14u32).to_le_bytes());
        let payload = read_frame(&mut buffer.as_slice()).expect("read");
        assert_eq!(payload, b"{\"hello\":true}");
    }

    #[test]
    fn reads_two_frames_in_a_row() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, b"one").expect("write");
        write_frame(&mut buffer, b"two").expect("write");
        let mut reader = buffer.as_slice();
        assert_eq!(read_frame(&mut reader).expect("first"), b"one");
        assert_eq!(read_frame(&mut reader).expect("second"), b"two");
        assert!(read_frame(&mut reader).is_err());
    }

    #[test]
    fn rejects_an_empty_or_oversized_payload() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &[]).expect("write");
        assert!(matches!(
            read_frame(&mut buffer.as_slice()),
            Err(IpcError::Protocol)
        ));

        let oversized = vec![0u8; 4];
        assert!(matches!(
            read_frame(&mut oversized.as_slice()),
            Err(IpcError::Protocol)
        ));

        let mut too_big = Vec::new();
        assert!(matches!(
            write_frame(&mut too_big, &vec![0u8; MAX_FRAME_BYTES + 1]),
            Err(IpcError::Protocol)
        ));
    }

    #[test]
    fn rejects_a_truncated_frame() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, b"payload").expect("write");
        buffer.truncate(buffer.len() - 2);
        assert!(read_frame(&mut buffer.as_slice()).is_err());
    }
}
