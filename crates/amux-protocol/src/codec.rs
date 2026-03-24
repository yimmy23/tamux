use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::{ClientMessage, DaemonMessage};

/// Length-prefixed bincode codec for amux IPC.
///
/// Wire format:
/// ```text
/// [4 bytes: payload length (little-endian u32)] [payload bytes (bincode)]
/// ```
///
/// This codec can encode both `ClientMessage` and `DaemonMessage` – the
/// concrete types are selected via generics.
#[derive(Debug, Default)]
pub struct AmuxCodec;

// Maximum allowed frame size: 16 MiB (generous for scrollback dumps).
const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Decoder: bytes -> DaemonMessage  (used by clients reading daemon replies)
// ---------------------------------------------------------------------------

impl Decoder for AmuxCodec {
    type Item = DaemonMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }

        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_le_bytes(length_bytes) as usize;

        if length as u32 > MAX_FRAME_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("frame too large: {length} bytes"),
            ));
        }

        if src.len() < 4 + length {
            src.reserve(4 + length - src.len());
            return Ok(None);
        }

        src.advance(4);
        let data = src.split_to(length);
        let msg: DaemonMessage = bincode::deserialize(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(Some(msg))
    }
}

// ---------------------------------------------------------------------------
// Encoder: ClientMessage -> bytes  (used by clients writing requests)
// ---------------------------------------------------------------------------

impl Encoder<ClientMessage> for AmuxCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: ClientMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let payload = bincode::serialize(&item)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        dst.put_u32_le(payload.len() as u32);
        dst.extend_from_slice(&payload);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mirrored codec for daemon side (decodes Client, encodes Daemon)
// ---------------------------------------------------------------------------

/// Codec used on the **daemon** side: decodes `ClientMessage`, encodes
/// `DaemonMessage`.
#[derive(Debug, Default)]
pub struct DaemonCodec;

impl Decoder for DaemonCodec {
    type Item = ClientMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }

        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_le_bytes(length_bytes) as usize;

        if length as u32 > MAX_FRAME_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("frame too large: {length} bytes"),
            ));
        }

        if src.len() < 4 + length {
            src.reserve(4 + length - src.len());
            return Ok(None);
        }

        src.advance(4);
        let data = src.split_to(length);
        let msg: ClientMessage = bincode::deserialize(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(Some(msg))
    }
}

impl Encoder<DaemonMessage> for DaemonCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: DaemonMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let payload = bincode::serialize(&item)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        dst.put_u32_le(payload.len() as u32);
        dst.extend_from_slice(&payload);
        Ok(())
    }
}
