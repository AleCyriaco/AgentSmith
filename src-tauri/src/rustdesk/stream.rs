//! Framed TCP carrying rendezvous and session messages, encrypted once the
//! handshake installs a session key.
use crate::rustdesk::{codec, crypto::Cipher, proto};
use prost::Message as _;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn frame(payload: Vec<u8>, cipher: Option<&mut Cipher>) -> Result<Vec<u8>, String> {
    let payload = match cipher {
        Some(cipher) => cipher.seal(&payload)?,
        None => payload,
    };
    codec::encode(&payload)
}

/// Pulls one framed payload, decrypting it when the session is secured.
async fn next_payload<R: AsyncReadExt + Unpin>(
    half: &mut R,
    buffer: &mut Vec<u8>,
    cipher: Option<&mut Cipher>,
) -> Result<Vec<u8>, String> {
    loop {
        if let Some((head, len)) = codec::decode_header(buffer)? {
            if buffer.len() >= head + len {
                let payload: Vec<u8> = buffer.drain(..head + len).skip(head).collect();
                return match cipher {
                    Some(cipher) => cipher.open(&payload),
                    None => Ok(payload),
                };
            }
        }
        let mut chunk = [0u8; 32 * 1024];
        let read = tokio::time::timeout(READ_TIMEOUT, half.read(&mut chunk))
            .await
            .map_err(|_| "O par RustDesk parou de responder.".to_string())?
            .map_err(|_| "A conexão RustDesk caiu.".to_string())?;
        if read == 0 {
            return Err("O par RustDesk encerrou a conexão.".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

fn parse(payload: &[u8]) -> Result<Option<proto::Message>, String> {
    if payload.is_empty() {
        return Ok(None);
    }
    let message = proto::Message::decode(payload)
        .map_err(|_| "Mensagem ilegível do par RustDesk.".to_string())?;
    Ok(message.union.is_some().then_some(message))
}

/// A connection driven in lockstep, as the rendezvous exchange and the
/// handshake require.
pub struct Stream {
    socket: TcpStream,
    buffer: Vec<u8>,
    cipher: Option<Cipher>,
}

impl Stream {
    pub async fn connect(address: &str) -> Result<Self, String> {
        let socket = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
            .await
            .map_err(|_| format!("Tempo esgotado ao conectar em {address}."))?
            .map_err(|_| format!("Não foi possível conectar em {address}."))?;
        let _ = socket.set_nodelay(true);
        Ok(Self {
            socket,
            buffer: Vec::new(),
            cipher: None,
        })
    }

    pub fn secure(&mut self, cipher: Cipher) {
        self.cipher = Some(cipher);
    }

    pub fn is_secure(&self) -> bool {
        self.cipher.is_some()
    }

    /// Separates the directions so input never waits on the video stream.
    /// Each half keeps its own nonce counter, matching the protocol.
    pub fn split(self) -> (Reader, Writer) {
        let (read, write) = self.socket.into_split();
        (
            Reader {
                half: read,
                buffer: self.buffer,
                cipher: self.cipher.clone(),
            },
            Writer {
                half: write,
                cipher: self.cipher,
            },
        )
    }

    async fn send_bytes(&mut self, payload: Vec<u8>) -> Result<(), String> {
        let framed = frame(payload, self.cipher.as_mut())?;
        self.socket
            .write_all(&framed)
            .await
            .map_err(|_| "A conexão RustDesk caiu ao enviar.".to_string())
    }

    pub async fn send_rendezvous(
        &mut self,
        message: proto::RendezvousMessage,
    ) -> Result<(), String> {
        self.send_bytes(message.encode_to_vec()).await
    }

    pub async fn recv_rendezvous(&mut self) -> Result<proto::RendezvousMessage, String> {
        let bytes = next_payload(&mut self.socket, &mut self.buffer, self.cipher.as_mut()).await?;
        proto::RendezvousMessage::decode(&bytes[..])
            .map_err(|_| "Resposta ilegível do servidor de encontro.".to_string())
    }

    pub async fn send(&mut self, message: proto::Message) -> Result<(), String> {
        self.send_bytes(message.encode_to_vec()).await
    }

    /// Reads the next session message, skipping the empty keepalive frames.
    pub async fn recv(&mut self) -> Result<proto::Message, String> {
        loop {
            let payload =
                next_payload(&mut self.socket, &mut self.buffer, self.cipher.as_mut()).await?;
            if let Some(message) = parse(&payload)? {
                return Ok(message);
            }
        }
    }
}

pub struct Reader {
    half: OwnedReadHalf,
    buffer: Vec<u8>,
    cipher: Option<Cipher>,
}

impl Reader {
    pub async fn recv(&mut self) -> Result<proto::Message, String> {
        loop {
            let payload =
                next_payload(&mut self.half, &mut self.buffer, self.cipher.as_mut()).await?;
            if let Some(message) = parse(&payload)? {
                return Ok(message);
            }
        }
    }
}

pub struct Writer {
    half: OwnedWriteHalf,
    cipher: Option<Cipher>,
}

impl Writer {
    pub async fn send(&mut self, message: proto::Message) -> Result<(), String> {
        let framed = frame(message.encode_to_vec(), self.cipher.as_mut())?;
        self.half
            .write_all(&framed)
            .await
            .map_err(|_| "A conexão RustDesk caiu ao enviar.".to_string())
    }
}
