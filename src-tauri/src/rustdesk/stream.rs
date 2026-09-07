//! Framed TCP carrying rendezvous and session messages, encrypted once the
//! handshake installs a session key.
use crate::rustdesk::{codec, crypto::Cipher, proto};
use prost::Message as _;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);

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

    async fn send_bytes(&mut self, payload: Vec<u8>) -> Result<(), String> {
        let payload = match self.cipher.as_mut() {
            Some(cipher) => cipher.seal(&payload)?,
            None => payload,
        };
        self.socket
            .write_all(&codec::encode(&payload)?)
            .await
            .map_err(|_| "A conexão RustDesk caiu ao enviar.".to_string())
    }

    async fn recv_bytes(&mut self) -> Result<Vec<u8>, String> {
        loop {
            if let Some((head, len)) = codec::decode_header(&self.buffer)? {
                if self.buffer.len() >= head + len {
                    let payload: Vec<u8> = self.buffer.drain(..head + len).skip(head).collect();
                    return match self.cipher.as_mut() {
                        Some(cipher) => cipher.open(&payload),
                        None => Ok(payload),
                    };
                }
            }
            let mut chunk = [0u8; 16 * 1024];
            let read = tokio::time::timeout(READ_TIMEOUT, self.socket.read(&mut chunk))
                .await
                .map_err(|_| "O par RustDesk parou de responder.".to_string())?
                .map_err(|_| "A conexão RustDesk caiu.".to_string())?;
            if read == 0 {
                return Err("O par RustDesk encerrou a conexão.".into());
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }

    pub async fn send_rendezvous(&mut self, message: proto::RendezvousMessage) -> Result<(), String> {
        self.send_bytes(message.encode_to_vec()).await
    }

    pub async fn recv_rendezvous(&mut self) -> Result<proto::RendezvousMessage, String> {
        let bytes = self.recv_bytes().await?;
        proto::RendezvousMessage::decode(&bytes[..])
            .map_err(|_| "Resposta ilegível do servidor de encontro.".to_string())
    }

    pub async fn send(&mut self, message: proto::Message) -> Result<(), String> {
        self.send_bytes(message.encode_to_vec()).await
    }

    /// Reads the next session message, skipping the empty keepalive frames the
    /// peer sends between real messages.
    pub async fn recv(&mut self) -> Result<proto::Message, String> {
        loop {
            let bytes = self.recv_bytes().await?;
            if bytes.is_empty() {
                continue;
            }
            let message = proto::Message::decode(&bytes[..])
                .map_err(|_| "Mensagem ilegível do par RustDesk.".to_string())?;
            if message.union.is_some() {
                return Ok(message);
            }
        }
    }
}
