//! Peer authentication and the session cipher.
//!
//! The rendezvous server signs `IdPk{id, pk}` with its Ed25519 key, so a peer
//! that cannot present a blob verifying under that key for the requested id is
//! not the machine the operator asked for. AgentSmith then seals a fresh
//! symmetric key to the peer's X25519 key; every later frame travels under
//! XSalsa20-Poly1305.
use crate::rustdesk::proto;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use crypto_box::{aead::Aead, PublicKey as BoxPublicKey, SalsaBox, SecretKey as BoxSecretKey};
use crypto_secretbox::{aead::KeyInit, XSalsa20Poly1305};
use ed25519_dalek::{Signature, VerifyingKey};
use prost::Message as _;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Public rendezvous key of rustdesk.com, used to check signed peer identities.
pub const PUBLIC_RENDEZVOUS_KEY: &str = "OeVuKk5nlHiXp+APNn0Y3pC1Iwpwn44JGqrQCsWqmBw=";
pub const RENDEZVOUS_PORT: u16 = 21116;
pub const RELAY_PORT: u16 = 21117;

const SIGNATURE_LEN: usize = 64;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;

/// Decodes a base64 Ed25519 public key, as configured for a rendezvous server.
pub fn signing_key(value: &str) -> Result<[u8; KEY_LEN], String> {
    let raw = STANDARD
        .decode(value.trim())
        .map_err(|_| "A chave do servidor de encontro não está em base64.".to_string())?;
    raw.try_into()
        .map_err(|_| "A chave do servidor de encontro não tem 32 bytes.".to_string())
}

/// Verifies a signed `IdPk` blob and returns the key it carries.
///
/// Used twice per session: the rendezvous server signs the peer's signing key,
/// and the peer then signs its own session key. `expected_id` is the RustDesk ID
/// the operator asked for; a blob that verifies for a different id is a
/// redirected session, not the requested machine.
pub fn verify_signed_identity(
    signed: &[u8],
    signer: &[u8; KEY_LEN],
    expected_id: &str,
) -> Result<[u8; KEY_LEN], String> {
    if signed.len() <= SIGNATURE_LEN {
        return Err("O par não apresentou identidade assinada.".into());
    }
    let verifying = VerifyingKey::from_bytes(signer)
        .map_err(|_| "Chave de assinatura inválida.".to_string())?;
    let (signature, body) = signed.split_at(SIGNATURE_LEN);
    let signature = Signature::from_bytes(
        signature
            .try_into()
            .map_err(|_| "Assinatura de identidade malformada.".to_string())?,
    );
    verifying
        .verify_strict(body, &signature)
        .map_err(|_| "A identidade assinada do par não confere.".to_string())?;
    let identity = proto::IdPk::decode(body)
        .map_err(|_| "Identidade assinada ilegível.".to_string())?;
    if identity.id != expected_id {
        return Err("O par autenticado tem outro ID; a sessão foi redirecionada.".into());
    }
    identity
        .pk
        .as_slice()
        .try_into()
        .map_err(|_| "A chave pública do par não tem 32 bytes.".to_string())
}

/// Our half of the key exchange: the values to send plus the resulting cipher.
pub struct KeyExchange {
    pub asymmetric_value: Vec<u8>,
    pub symmetric_value: Vec<u8>,
    pub cipher: Cipher,
}

/// Seals a fresh session key to the peer's X25519 public key under a zero nonce,
/// which is safe here because the sealing keypair is generated per session.
pub fn seal_session_key(peer_public_key: [u8; KEY_LEN]) -> Result<KeyExchange, String> {
    let mut secret = [0u8; KEY_LEN];
    let mut session = [0u8; KEY_LEN];
    rand::rngs::OsRng.fill_bytes(&mut secret);
    rand::rngs::OsRng.fill_bytes(&mut session);
    let our_secret = BoxSecretKey::from_bytes(secret);
    let our_public = our_secret.public_key();
    let sealed = SalsaBox::new(&BoxPublicKey::from_bytes(peer_public_key), &our_secret)
        .encrypt(&[0u8; NONCE_LEN].into(), &session[..])
        .map_err(|_| "Não foi possível proteger a chave da sessão.".to_string())?;
    Ok(KeyExchange {
        asymmetric_value: our_public.as_bytes().to_vec(),
        symmetric_value: sealed,
        cipher: Cipher::new(session),
    })
}

/// XSalsa20-Poly1305 over the frame payloads, with independent counters per
/// direction. Counters are incremented before use, so the first frame carries
/// sequence 1, and frames of one byte or less pass through untouched.
///
/// Cloning splits a session by direction: each half keeps its own counter, and
/// the two never share one, so a clone is not a nonce reuse.
#[derive(Clone)]
pub struct Cipher {
    key: XSalsa20Poly1305,
    sent: u64,
    received: u64,
}

impl Cipher {
    pub fn new(key: [u8; KEY_LEN]) -> Self {
        Self {
            key: XSalsa20Poly1305::new(&key.into()),
            sent: 0,
            received: 0,
        }
    }

    fn nonce(sequence: u64) -> [u8; NONCE_LEN] {
        let mut nonce = [0u8; NONCE_LEN];
        nonce[..8].copy_from_slice(&sequence.to_le_bytes());
        nonce
    }

    pub fn seal(&mut self, payload: &[u8]) -> Result<Vec<u8>, String> {
        self.sent += 1;
        self.key
            .encrypt(&Self::nonce(self.sent).into(), payload)
            .map_err(|_| "Falha ao cifrar a mensagem da sessão.".to_string())
    }

    pub fn open(&mut self, payload: &[u8]) -> Result<Vec<u8>, String> {
        if payload.len() <= 1 {
            return Ok(payload.to_vec());
        }
        self.received += 1;
        self.key
            .decrypt(&Self::nonce(self.received).into(), payload)
            .map_err(|_| "Mensagem da sessão rejeitada na decifragem.".to_string())
    }
}

/// The peer stores `sha256(password || salt)` and challenges each connection,
/// so the password itself never travels.
pub fn login_password(password: &str, salt: &str, challenge: &str) -> Vec<u8> {
    let mut stored = Sha256::new();
    stored.update(password.as_bytes());
    stored.update(salt.as_bytes());
    let mut answer = Sha256::new();
    answer.update(stored.finalize());
    answer.update(challenge.as_bytes());
    answer.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_identity(signing: &SigningKey, id: &str, pk: [u8; 32]) -> Vec<u8> {
        let body = proto::IdPk {
            id: id.into(),
            pk: pk.to_vec(),
            dtls_fingerprint: String::new(),
        }
        .encode_to_vec();
        let mut signed = signing.sign(&body).to_bytes().to_vec();
        signed.extend_from_slice(&body);
        signed
    }

    #[test]
    fn only_the_requested_peer_signed_by_the_server_is_accepted() {
        let signing = SigningKey::from_bytes(&[9u8; 32]);
        let server = signing.verifying_key().to_bytes();
        let peer_pk = [4u8; 32];
        assert_eq!(
            verify_signed_identity(&signed_identity(&signing, "123456789", peer_pk), &server, "123456789")
                .unwrap(),
            peer_pk
        );
        // Right signature, wrong machine: a redirected session must not connect.
        assert!(verify_signed_identity(
            &signed_identity(&signing, "999999999", peer_pk),
            &server,
            "123456789"
        )
        .is_err());
        // Correctly formed identity signed by somebody else.
        let attacker = SigningKey::from_bytes(&[1u8; 32]);
        assert!(verify_signed_identity(
            &signed_identity(&attacker, "123456789", peer_pk),
            &server,
            "123456789"
        )
        .is_err());
        assert_eq!(signing_key(&STANDARD.encode(server)).unwrap(), server);
        assert!(signing_key("nao-e-base64!!").is_err());
        assert!(signing_key(&STANDARD.encode([1u8; 8])).is_err());
        assert!(verify_signed_identity(&[0u8; 40], &server, "123456789").is_err());
    }

    #[test]
    fn sealed_key_reaches_the_peer_and_only_the_peer() {
        let mut secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        let peer_secret = BoxSecretKey::from_bytes(secret);
        let exchange = seal_session_key(peer_secret.public_key().to_bytes()).unwrap();
        let opened = SalsaBox::new(
            &BoxPublicKey::from_bytes(exchange.asymmetric_value.clone().try_into().unwrap()),
            &peer_secret,
        )
        .decrypt(&[0u8; NONCE_LEN].into(), &exchange.symmetric_value[..])
        .unwrap();
        assert_eq!(opened.len(), 32);
    }

    #[test]
    fn frames_decrypt_in_order_and_short_frames_pass_through() {
        let mut writer = Cipher::new([3u8; 32]);
        let mut reader = Cipher::new([3u8; 32]);
        for message in [b"primeira".to_vec(), b"segunda".to_vec(), vec![0u8; 4096]] {
            let sealed = writer.seal(&message).unwrap();
            assert_ne!(sealed, message);
            assert_eq!(reader.open(&sealed).unwrap(), message);
        }
        // Keepalive-sized frames are not encrypted by the peer either.
        assert_eq!(reader.open(&[7]).unwrap(), vec![7]);
        // A replayed frame no longer matches its sequence number.
        let sealed = writer.seal(b"nova").unwrap();
        let mut replay = Cipher::new([3u8; 32]);
        assert!(replay.open(&sealed).is_err());
    }

    #[test]
    fn first_frame_uses_sequence_one() {
        let mut cipher = Cipher::new([5u8; 32]);
        let sealed = cipher.seal(b"x").unwrap();
        let mut direct = XSalsa20Poly1305::new(&[5u8; 32].into());
        assert_eq!(
            direct
                .decrypt(&Cipher::nonce(1).into(), &sealed[..])
                .unwrap(),
            b"x"
        );
        let _ = &mut direct;
    }

    #[test]
    fn password_never_travels_in_the_clear() {
        let hash = login_password("segredo", "sal", "desafio");
        assert_eq!(hash.len(), 32);
        assert!(!hash.starts_with(b"segredo"));
        assert_ne!(hash, login_password("segredo", "sal", "outro"));
        assert_ne!(hash, login_password("segredo", "outro", "desafio"));
    }
}
