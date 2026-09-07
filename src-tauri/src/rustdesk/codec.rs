//! RustDesk frame boundaries. Every payload is prefixed by a little-endian
//! header whose two low bits give the number of extra header bytes; the
//! remaining bits are the payload length.

/// Largest payload the four-byte header can express.
pub const MAX_FRAME: usize = 0x3FFF_FFFF;
/// Refuse to buffer more than one screen-sized frame from an unauthenticated peer.
pub const MAX_ACCEPTED_FRAME: usize = 16 * 1024 * 1024;

pub fn encode(payload: &[u8]) -> Result<Vec<u8>, String> {
    let n = payload.len();
    let mut out = Vec::with_capacity(n + 4);
    match n {
        0..=0x3F => out.push((n << 2) as u8),
        0x40..=0x3FFF => out.extend_from_slice(&(((n << 2) | 0x1) as u16).to_le_bytes()),
        0x4000..=0x3F_FFFF => {
            let h = ((n << 2) | 0x2) as u32;
            out.extend_from_slice(&(h as u16).to_le_bytes());
            out.push((h >> 16) as u8);
        }
        _ if n <= MAX_FRAME => {
            out.extend_from_slice(&(((n << 2) | 0x3) as u32).to_le_bytes())
        }
        _ => return Err("Quadro maior do que o protocolo permite.".into()),
    }
    out.extend_from_slice(payload);
    Ok(out)
}

/// Header length and payload length, or `None` while the header is incomplete.
pub fn decode_header(buffer: &[u8]) -> Result<Option<(usize, usize)>, String> {
    let Some(first) = buffer.first() else {
        return Ok(None);
    };
    let head = (first & 0x3) as usize + 1;
    if buffer.len() < head {
        return Ok(None);
    }
    let mut n = 0usize;
    for (i, byte) in buffer[..head].iter().enumerate() {
        n |= (*byte as usize) << (8 * i);
    }
    n >>= 2;
    if n > MAX_ACCEPTED_FRAME {
        return Err("O par anunciou um quadro grande demais.".into());
    }
    Ok(Some((head, n)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(n: usize) -> usize {
        let frame = encode(&vec![7u8; n]).unwrap();
        let (head, len) = decode_header(&frame).unwrap().unwrap();
        assert_eq!(len, n);
        assert_eq!(frame.len(), head + n);
        assert!(frame[head..].iter().all(|b| *b == 7));
        head
    }

    #[test]
    fn header_width_follows_payload_size() {
        assert_eq!(roundtrip(0), 1);
        assert_eq!(roundtrip(0x3F), 1);
        assert_eq!(roundtrip(0x40), 2);
        assert_eq!(roundtrip(0x3FFF), 2);
        assert_eq!(roundtrip(0x4000), 3);
        assert_eq!(roundtrip(0x3F_FFFF), 3);
        assert_eq!(roundtrip(0x40_0000), 4);
    }

    #[test]
    fn partial_headers_wait_and_oversized_frames_are_refused() {
        let frame = encode(&vec![0u8; 0x4000]).unwrap();
        assert!(decode_header(&[]).unwrap().is_none());
        assert!(decode_header(&frame[..2]).unwrap().is_none());
        assert!(decode_header(&frame[..3]).unwrap().is_some());
        // 0x3FFFFFFF payload announced by a four-byte header.
        assert!(decode_header(&u32::MAX.to_le_bytes()).is_err());
        assert!(encode(&vec![]).is_ok());
    }
}
