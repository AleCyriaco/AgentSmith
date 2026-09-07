//! The rendezvous server obfuscates peer addresses with a timestamp before
//! putting them on the wire, so they have to be unmangled the same way.
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};

/// Decodes a mangled peer address. Malformed input yields an unusable
/// address rather than a panic, and the caller rejects port 0.
pub fn decode(bytes: &[u8]) -> Option<SocketAddr> {
    if bytes.len() > 16 {
        if bytes.len() != 18 {
            return None;
        }
        let ip: [u8; 16] = bytes[..16].try_into().ok()?;
        let port = u16::from_le_bytes(bytes[16..].try_into().ok()?);
        return (port > 0).then(|| SocketAddr::new(IpAddr::V6(Ipv6Addr::from(ip)), port));
    }
    if bytes.is_empty() {
        return None;
    }
    let mut padded = [0u8; 16];
    padded[..bytes.len()].copy_from_slice(bytes);
    let number = u128::from_le_bytes(padded);
    let stamp = (number >> 17) & u32::MAX as u128;
    // The peer truncates both values, so the high bits the timestamp leaves
    // behind are discarded rather than treated as an error.
    let ip = (number >> 49).wrapping_sub(stamp) as u32;
    let port = (number & 0xFF_FFFF).wrapping_sub(stamp & 0xFFFF) as u16;
    (port > 0).then(|| {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(ip.to_le_bytes()), port))
    })
}

/// Mangles an address the way a peer would; used to exercise [`decode`].
#[cfg(test)]
pub fn encode(addr: SocketAddr, stamp: u32) -> Vec<u8> {
    match addr {
        SocketAddr::V4(v4) => {
            let stamp = stamp as u128;
            let ip = u32::from_le_bytes(v4.ip().octets()) as u128;
            let value = ((ip + stamp) << 49) | (stamp << 17) | (v4.port() as u128 + (stamp & 0xFFFF));
            let bytes = value.to_le_bytes();
            let keep = 16 - bytes.iter().rev().take_while(|b| **b == 0).count();
            bytes[..keep].to_vec()
        }
        SocketAddr::V6(v6) => {
            let mut out = v6.ip().octets().to_vec();
            out.extend_from_slice(&v6.port().to_le_bytes());
            out
        }
    }
}

/// Appends the protocol's default port when the operator gave a bare host.
pub fn with_default_port(host: &str, port: u16) -> String {
    let host = host.trim();
    if host.starts_with('[') || host.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()).is_none()
    {
        format!("{host}:{port}")
    } else {
        host.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mangled_addresses_survive_a_round_trip() {
        for stamp in [1u32, 1_000_003, u32::MAX / 2] {
            for text in ["192.0.2.228:21118", "1.2.3.4:65535", "203.0.113.9:1"] {
                let addr: SocketAddr = text.parse().unwrap();
                assert_eq!(decode(&encode(addr, stamp)), Some(addr), "{text} @ {stamp}");
            }
        }
        let v6: SocketAddr = "[2001:db8::1]:21118".parse().unwrap();
        assert_eq!(decode(&encode(v6, 0)), Some(v6));
    }

    #[test]
    fn malformed_addresses_are_rejected_instead_of_panicking() {
        for bytes in [
            vec![],
            vec![0u8; 17],
            vec![0u8; 19],
            vec![0xFF; 16],
            vec![1, 2, 3],
        ] {
            let _ = decode(&bytes);
        }
        // A peer with no reachable port must not be dialled.
        assert_eq!(decode(&encode("1.2.3.4:0".parse().unwrap(), 7)), None);
    }

    #[test]
    fn default_ports_apply_only_when_missing() {
        assert_eq!(with_default_port("rs.example.com", 21116), "rs.example.com:21116");
        assert_eq!(with_default_port("rs.example.com:9", 21116), "rs.example.com:9");
        assert_eq!(with_default_port(" 10.0.0.1 ", 21117), "10.0.0.1:21117");
        assert_eq!(with_default_port("[2001:db8::1]", 21116), "[2001:db8::1]:21116");
    }
}
