//! Establishes an authenticated RustDesk session: rendezvous, hole punch or
//! relay, signed-identity handshake, and password login.
use crate::rustdesk::{
    address,
    crypto::{self, RELAY_PORT, RENDEZVOUS_PORT},
    input::Input,
    proto::{self, message, rendezvous_message},
    stream::{Reader, Stream, Writer},
};

/// RustDesk protocol level AgentSmith implements and announces to the peer.
pub const PROTOCOL_VERSION: &str = "1.3.0";
pub const DEFAULT_RENDEZVOUS: &str = "rs-ny.rustdesk.com";
/// Returned when the machine wants a second factor and none was supplied, so
/// the interface can ask for a code instead of showing a dead end.
pub const SECOND_FACTOR_REQUIRED: &str = "second-factor";
/// How long to try the machine's own address before falling back to a relay.
pub const DIRECT_ATTEMPT: std::time::Duration = std::time::Duration::from_secs(3);
/// Same, for a machine that does not keep trusted devices: a code will be
/// wanted on every connection, and saying so beats a checkbox that does nothing.
pub const SECOND_FACTOR_WITHOUT_TRUST: &str = "second-factor-no-trust";

#[derive(Clone, Debug)]
pub struct Options {
    /// The peer's RustDesk ID.
    pub id: String,
    pub password: String,
    /// Rendezvous host, with optional port. Empty uses RustDesk's public server.
    pub rendezvous: String,
    /// Base64 Ed25519 key of that rendezvous server. Empty uses the public one.
    pub key: String,
    /// Current six-digit code, for a machine with two-factor enabled. It is
    /// time-based, so it is supplied per connection and never stored.
    pub two_factor_code: String,
    /// Ask the machine to remember this Mac, so later connections skip the
    /// second factor. It weakens that machine's protection, so it is only ever
    /// set from an explicit choice.
    pub trust_device: bool,
}

impl Options {
    pub fn rendezvous_address(&self) -> String {
        let host = if self.rendezvous.trim().is_empty() {
            DEFAULT_RENDEZVOUS
        } else {
            self.rendezvous.trim()
        };
        address::with_default_port(host, RENDEZVOUS_PORT)
    }

    pub fn signing_key(&self) -> Result<[u8; 32], String> {
        crypto::signing_key(if self.key.trim().is_empty() {
            crypto::PUBLIC_RENDEZVOUS_KEY
        } else {
            self.key.trim()
        })
    }
}

/// What the peer told us about itself once logged in.
#[derive(Clone, Debug, Default)]
pub struct Peer {
    pub hostname: String,
    pub username: String,
    pub platform: String,
    pub version: String,
    pub width: u32,
    pub height: u32,
    /// On a Windows machine with more than one session, the one this session
    /// attached to. `None` when the machine offered no choice.
    pub session: Option<(u32, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Vp8,
    Vp9,
}

#[derive(Debug)]
pub enum Event {
    /// One encoded video frame for the decoder.
    Video {
        codec: Codec,
        data: Vec<u8>,
        key: bool,
    },
    /// The peer's latency probe, which the caller answers through [`Commands`].
    Ping(proto::TestDelay),
    /// A message with nothing for the caller to do.
    Idle,
    Closed(String),
}

pub struct Session {
    stream: Stream,
    pub peer: Peer,
    /// How the machine was reached, for the operator and for diagnosis.
    pub path: &'static str,
}

/// Turns the rendezvous server's refusal into something an operator can act on.
fn punch_failure(response: &proto::PunchHoleResponse) -> String {
    if !response.other_failure.is_empty() {
        return response.other_failure.clone();
    }
    match response.failure() {
        proto::punch_hole_response::Failure::IdNotExist => {
            "Este ID RustDesk não existe no servidor de encontro.".into()
        }
        proto::punch_hole_response::Failure::Offline => {
            "A máquina RustDesk está offline.".into()
        }
        proto::punch_hole_response::Failure::LicenseMismatch => {
            "A chave do servidor RustDesk não confere.".into()
        }
        proto::punch_hole_response::Failure::LicenseOveruse => {
            "A licença do servidor RustDesk está esgotada.".into()
        }
    }
}

/// A stable controller identity. The machine remembers a trusted device by
/// this id together with the hardware hash, so it must not change between
/// the connection that trusted and the ones after it — which rules out
/// anything taken from the environment, since a windowed app may not have it.
fn controller_id() -> String {
    let digest = crate::rustdesk::device::identity();
    let value = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    format!("{:09}", value % 1_000_000_000)
}

/// Announces only the codecs AgentSmith can actually decode, so the peer does
/// not send a stream that would arrive as a blank screen.
fn login_options() -> proto::OptionMessage {
    use proto::option_message::BoolOption;
    proto::OptionMessage {
        image_quality: proto::ImageQuality::Balanced as i32,
        // AgentSmith reads the screen; audio, clipboard and file transfer are not used.
        disable_audio: BoolOption::Yes as i32,
        disable_clipboard: BoolOption::Yes as i32,
        enable_file_transfer: BoolOption::No as i32,
        show_remote_cursor: BoolOption::Yes as i32,
        supported_decoding: Some(proto::SupportedDecoding {
            ability_vp8: 1,
            ability_vp9: 1,
            ability_h264: 0,
            ability_h265: 0,
            ability_av1: 0,
            prefer: proto::supported_decoding::PreferCodec::Vp9 as i32,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn login_request(options: &Options, hash: &proto::Hash) -> proto::LoginRequest {
    proto::LoginRequest {
        username: options.id.clone(),
        password: crypto::login_password(&options.password, &hash.salt, &hash.challenge),
        my_id: controller_id(),
        my_name: "AgentSmith".into(),
        option: Some(login_options()),
        video_ack_required: false,
        session_id: rand::random(),
        version: PROTOCOL_VERSION.into(),
        my_platform: "Mac OS".into(),
        // Presented on every login. A machine that was asked to trust this Mac
        // recognises it here and skips the second factor; one that was not
        // simply ignores it, as it already knows the controller id.
        hwid: crate::rustdesk::device::identity(),
    }
}

/// A Windows machine can offer the console and one or more remote sessions.
/// Unattended work cannot show a dialog, so the active session is taken when
/// the machine names one, and otherwise the first it offers.
fn choose_session(info: &proto::PeerInfo) -> Option<(u32, String)> {
    let sessions = info.windows_sessions.as_ref()?;
    let pick = sessions
        .sessions
        .iter()
        .find(|session| session.sid == sessions.current_sid)
        .or_else(|| sessions.sessions.first())?;
    Some((pick.sid, pick.name.clone()))
}

/// The peer reports every display; AgentSmith drives the one it marked current.
fn active_display(info: &proto::PeerInfo) -> Option<&proto::DisplayInfo> {
    info.displays
        .get(info.current_display.max(0) as usize)
        .or_else(|| info.displays.first())
}

impl Session {
    pub async fn connect(options: &Options) -> Result<Self, String> {
        // Accepts the grouped form RustDesk displays, as the machine form does.
        let id: String = options.id.chars().filter(|c| !c.is_whitespace()).collect();
        let id = id.as_str();
        if id.is_empty() {
            return Err("Informe o ID RustDesk da máquina.".into());
        }
        let signer = options.signing_key()?;
        crate::diag!("encontro em {} para o ID {id}", options.rendezvous_address());
        let (mut stream, signed_peer_key, path) = Self::reach_peer(options, id)
            .await
            .map_err(|error| {
                crate::diag!("encontro falhou: {error}");
                format!("Encontro: {error}")
            })?;
        crate::diag!("par alcançado via {path}, identidade assinada de {} bytes", signed_peer_key.len());
        Self::handshake(&mut stream, &signed_peer_key, &signer, id)
            .await
            .map_err(|error| {
                crate::diag!("handshake falhou: {error}");
                format!("Handshake ({path}): {error}")
            })?;
        crate::diag!("handshake concluído, sessão cifrada");
        // The second-factor prompt travels unwrapped so the interface can
        // recognise it; every other failure names its stage.
        let peer = match Self::login(&mut stream, options).await {
            Ok(peer) => peer,
            Err(error)
                if error == SECOND_FACTOR_REQUIRED || error == SECOND_FACTOR_WITHOUT_TRUST =>
            {
                return Err(error)
            }
            Err(error) => return Err(format!("Login ({path}): {error}")),
        };
        Self::attach_session(&mut stream, &peer)
            .await
            .map_err(|error| format!("Sessão do Windows ({path}): {error}"))?;
        Ok(Self { stream, peer, path })
    }

    /// Asks the rendezvous server for the peer and returns a stream to it plus
    /// the server-signed blob that carries the peer's signing key.
    async fn reach_peer(
        options: &Options,
        id: &str,
    ) -> Result<(Stream, Vec<u8>, &'static str), String> {
        let rendezvous = options.rendezvous_address();
        let mut server = Stream::connect(&rendezvous).await?;
        server
            .send_rendezvous(proto::RendezvousMessage {
                union: Some(rendezvous_message::Union::PunchHoleRequest(
                    proto::PunchHoleRequest {
                        id: id.into(),
                        licence_key: options.key.trim().into(),
                        conn_type: proto::ConnType::DefaultConn as i32,
                        version: PROTOCOL_VERSION.into(),
                        nat_type: proto::NatType::Asymmetric as i32,
                        ..Default::default()
                    },
                )),
            })
            .await?;
        match server.recv_rendezvous().await?.union {
            Some(rendezvous_message::Union::PunchHoleResponse(response)) => {
                crate::diag!(
                    "resposta de encontro: endereço {} bytes, relay '{}', nat {:?}, chave {} bytes",
                    response.socket_addr.len(), response.relay_server, response.union, response.pk.len()
                );
                if response.socket_addr.is_empty() {
                    return Err(punch_failure(&response));
                }
                let signed = response.pk.clone();
                if let Some(peer) = address::decode(&response.socket_addr) {
                    // The address is often on the machine's own LAN and cannot
                    // be reached from here; a short attempt keeps the fall
                    // back to the relay from costing every connection twelve
                    // seconds.
                    crate::diag!("tentando conexão direta em {peer}");
                    match Stream::connect_within(&peer.to_string(), DIRECT_ATTEMPT).await {
                        Ok(direct) => return Ok((direct, signed, "direto")),
                        Err(error) => crate::diag!("direta falhou: {error}"),
                    }
                }
                // The direct path is blocked by NAT; fall back to the relay.
                let relay = Self::open_relay(options, id, &response.relay_server).await?;
                Ok((relay, signed, "retransmissão"))
            }
            // The peer itself asked for a relay and the server already paired one.
            Some(rendezvous_message::Union::RelayResponse(response)) => {
                crate::diag!("o par pediu retransmissão por '{}'", response.relay_server);
                if !response.refuse_reason.is_empty() {
                    return Err(response.refuse_reason);
                }
                let signed = match response.union {
                    Some(proto::relay_response::Union::Pk(pk)) => pk,
                    _ => Vec::new(),
                };
                let stream =
                    Self::join_relay(options, id, &response.relay_server, &response.uuid).await?;
                Ok((stream, signed, "retransmissão pedida pelo par"))
            }
            _ => Err("O servidor de encontro respondeu de forma inesperada.".into()),
        }
    }

    /// Negotiates a fresh relay pairing, then joins it.
    async fn open_relay(options: &Options, id: &str, relay: &str) -> Result<Stream, String> {
        if relay.trim().is_empty() {
            return Err("A máquina não está acessível e o servidor não ofereceu retransmissão.".into());
        }
        let uuid = uuid::Uuid::new_v4().to_string();
        let mut server = Stream::connect(&options.rendezvous_address()).await?;
        server
            .send_rendezvous(proto::RendezvousMessage {
                union: Some(rendezvous_message::Union::RequestRelay(
                    proto::RequestRelay {
                        id: id.into(),
                        uuid: uuid.clone(),
                        relay_server: relay.into(),
                        secure: true,
                        licence_key: options.key.trim().into(),
                        conn_type: proto::ConnType::DefaultConn as i32,
                        ..Default::default()
                    },
                )),
            })
            .await?;
        match server.recv_rendezvous().await?.union {
            Some(rendezvous_message::Union::RelayResponse(response))
                if response.refuse_reason.is_empty() =>
            {
                Self::join_relay(options, id, relay, &uuid).await
            }
            Some(rendezvous_message::Union::RelayResponse(response)) => Err(response.refuse_reason),
            _ => Err("A retransmissão RustDesk não foi aceita.".into()),
        }
    }

    async fn join_relay(
        options: &Options,
        id: &str,
        relay: &str,
        uuid: &str,
    ) -> Result<Stream, String> {
        let mut stream = Stream::connect(&address::with_default_port(relay, RELAY_PORT)).await?;
        stream
            .send_rendezvous(proto::RendezvousMessage {
                union: Some(rendezvous_message::Union::RequestRelay(
                    proto::RequestRelay {
                        id: id.into(),
                        uuid: uuid.into(),
                        licence_key: options.key.trim().into(),
                        conn_type: proto::ConnType::DefaultConn as i32,
                        ..Default::default()
                    },
                )),
            })
            .await?;
        Ok(stream)
    }

    /// Binds the session to the machine the operator named, then encrypts it.
    ///
    /// AgentSmith refuses an unauthenticated peer: an AI loop that types and
    /// clicks must not run over a session that anyone on the path could read or
    /// redirect, so there is no plaintext fallback here.
    async fn handshake(
        stream: &mut Stream,
        signed_peer_key: &[u8],
        signer: &[u8; 32],
        id: &str,
    ) -> Result<(), String> {
        if signed_peer_key.is_empty() {
            return Err(
                "O servidor de encontro não assinou a identidade desta máquina; AgentSmith não abre sessão sem autenticação."
                    .into(),
            );
        }
        let peer_signing_key = crypto::verify_signed_identity(signed_peer_key, signer, id)?;
        let signed = loop {
            match stream.recv().await?.union {
                Some(message::Union::SignedId(signed)) => break signed,
                // Same probe as above, should it ever arrive this early.
                Some(message::Union::TestDelay(delay)) if !delay.from_client => {
                    stream
                        .send(proto::Message {
                            union: Some(message::Union::TestDelay(delay)),
                        })
                        .await?;
                }
                _ => return Err("O par não iniciou o handshake com sua identidade.".into()),
            }
        };
        let peer_session_key =
            crypto::verify_signed_identity(&signed.id, &peer_signing_key, id)?;
        let exchange = crypto::seal_session_key(peer_session_key)?;
        stream
            .send(proto::Message {
                union: Some(message::Union::PublicKey(proto::PublicKey {
                    asymmetric_value: exchange.asymmetric_value,
                    symmetric_value: exchange.symmetric_value,
                })),
            })
            .await?;
        stream.secure(exchange.cipher);
        Ok(())
    }

    async fn login(stream: &mut Stream, options: &Options) -> Result<Peer, String> {
        let Some(message::Union::Hash(hash)) = stream.recv().await?.union else {
            return Err("O par não pediu autenticação como esperado.".into());
        };
        stream
            .send(proto::Message {
                union: Some(message::Union::LoginRequest(login_request(options, &hash))),
            })
            .await?;
        // The peer may answer the password with a second-factor challenge; it is
        // sent once, so a repeated demand means the code did not satisfy it.
        let mut answered_second_factor = false;
        loop {
            match stream.recv().await?.union {
                // The machine starts probing latency the moment the link is up,
                // so its first probe usually lands here, before the login is
                // answered. It sends the next only once this one comes back;
                // drop it and the machine never probes again, the session goes
                // quiet, and half a minute later it closes for timeout.
                Some(message::Union::TestDelay(delay)) if !delay.from_client => {
                    crate::diag!("ping durante o login, ecoando");
                    stream
                        .send(proto::Message {
                            union: Some(message::Union::TestDelay(delay)),
                        })
                        .await?;
                }
                Some(message::Union::LoginResponse(response)) => {
                  // The machine says on this same answer whether it can be
                  // asked to remember a device.
                  let keeps_trusted = response.enable_trusted_devices;
                  match response.union {
                    Some(proto::login_response::Union::PeerInfo(info)) => {
                        return Ok(Self::describe(&info))
                    }
                    Some(proto::login_response::Union::Error(error))
                        if error == "2FA Required" && !answered_second_factor =>
                    {
                        let code = options.two_factor_code.trim();
                        if code.is_empty() {
                            return Err(if keeps_trusted {
                                SECOND_FACTOR_REQUIRED
                            } else {
                                SECOND_FACTOR_WITHOUT_TRUST
                            }
                            .into());
                        }
                        answered_second_factor = true;
                        crate::diag!("enviando segundo fator, confiar={}", options.trust_device);
                        stream
                            .send(proto::Message {
                                union: Some(message::Union::Auth2fa(proto::Auth2Fa {
                                    code: code.into(),
                                    // Sent only when the operator chose it: the
                                    // machine then trusts this Mac and stops
                                    // asking for a code.
                                    hwid: if options.trust_device {
                                        crate::rustdesk::device::identity()
                                    } else {
                                        Vec::new()
                                    },
                                })),
                            })
                            .await?;
                    }
                    Some(proto::login_response::Union::Error(error)) => {
                        return Err(Self::login_error(&error))
                    }
                    None => return Err("O par recusou o acesso sem explicar.".into()),
                  }
                }
                Some(message::Union::PeerInfo(info)) => return Ok(Self::describe(&info)),
                Some(message::Union::Misc(misc)) => {
                    if let Some(proto::misc::Union::CloseReason(reason)) = misc.union {
                        return Err(Self::login_error(&reason));
                    }
                }
                _ => continue,
            }
        }
    }

    fn login_error(error: &str) -> String {
        match error {
            "Wrong Password" => "Senha RustDesk incorreta.".into(),
            "2FA Required" => "Esta máquina exige verificação em duas etapas. Informe o código atual do autenticador.".into(),
            "Wrong 2FA Code" => "Código de verificação incorreto ou expirado. Ele muda a cada 30 segundos.".into(),
            "No Password Access" => {
                "A máquina exige aprovação manual e não aceita senha.".into()
            }
            "" => "O par RustDesk recusou o acesso sem dizer o motivo.".into(),
            other => format!("O par RustDesk recusou o acesso: {other}"),
        }
    }

    fn describe(info: &proto::PeerInfo) -> Peer {
        let display = active_display(info);
        Peer {
            hostname: info.hostname.clone(),
            username: info.username.clone(),
            platform: info.platform.clone(),
            version: info.version.clone(),
            width: display.map(|d| d.width.max(0) as u32).unwrap_or(0),
            height: display.map(|d| d.height.max(0) as u32).unwrap_or(0),
            session: choose_session(info),
        }
    }

    /// Tells a multi-session Windows machine which session to stream. Without
    /// this the machine waits for a choice and no picture ever arrives.
    async fn attach_session(stream: &mut Stream, peer: &Peer) -> Result<(), String> {
        let Some((sid, _)) = peer.session else {
            return Ok(());
        };
        crate::diag!("escolhendo sessão do Windows sid {sid}");
        stream
            .send(proto::Message {
                union: Some(message::Union::Misc(proto::Misc {
                    union: Some(proto::misc::Union::SelectedSid(sid)),
                })),
            })
            .await
    }

    /// Separates video from input so neither waits on the other.
    pub fn split(self) -> (Peer, Events, Commands) {
        let (reader, writer) = self.stream.split();
        (self.peer, Events { reader }, Commands { writer })
    }

    pub fn is_secure(&self) -> bool {
        self.stream.is_secure()
    }
}

fn video(frame: proto::VideoFrame) -> Event {
    let (codec, frames) = match frame.union {
        Some(proto::video_frame::Union::Vp9s(frames)) => (Codec::Vp9, frames),
        Some(proto::video_frame::Union::Vp8s(frames)) => (Codec::Vp8, frames),
        // The peer ignored our declared abilities; treat it as no picture
        // rather than pretending a frame arrived.
        _ => return Event::Idle,
    };
    match frames.frames.into_iter().next() {
        Some(first) => Event::Video {
            codec,
            data: first.data,
            key: first.key,
        },
        None => Event::Idle,
    }
}

/// The incoming half of a session.
pub struct Events {
    reader: Reader,
}

impl Events {
    pub async fn next(&mut self) -> Result<Event, String> {
        match self.reader.recv().await?.union {
            Some(message::Union::VideoFrame(frame)) => Ok(video(frame)),
            Some(message::Union::TestDelay(delay)) if !delay.from_client => {
                Ok(Event::Ping(delay))
            }
            Some(message::Union::Misc(misc)) => match misc.union {
                Some(proto::misc::Union::CloseReason(reason)) => Ok(Event::Closed(reason)),
                _ => Ok(Event::Idle),
            },
            _ => Ok(Event::Idle),
        }
    }
}

/// The outgoing half of a session.
pub struct Commands {
    writer: Writer,
}

impl Commands {
    /// Asks for a fresh key frame, used after connecting or resuming.
    pub async fn request_refresh(&mut self) -> Result<(), String> {
        self.writer
            .send(proto::Message {
                union: Some(message::Union::Misc(proto::Misc {
                    union: Some(proto::misc::Union::RefreshVideo(true)),
                })),
            })
            .await
    }

    /// Echoes the machine's latency probe back untouched.
    ///
    /// The machine sends one probe at a time and waits for it to come back
    /// before sending the next, and it tells its own probes from ours by the
    /// `from_client` flag — so the echo must carry the flag as received.
    /// Marking it as ours makes the machine echo it back instead, and it then
    /// never sends another: the session goes quiet and, after thirty seconds,
    /// the machine closes it for timeout.
    pub async fn pong(&mut self, delay: proto::TestDelay) -> Result<(), String> {
        self.writer
            .send(proto::Message {
                union: Some(message::Union::TestDelay(delay)),
            })
            .await
    }

    pub async fn send(&mut self, inputs: &[Input]) -> Result<(), String> {
        for input in inputs {
            let union = match input {
                Input::Mouse { mask, x, y } => message::Union::MouseEvent(proto::MouseEvent {
                    mask: *mask,
                    x: *x,
                    y: *y,
                    modifiers: Vec::new(),
                }),
                Input::Key(event) => message::Union::KeyEvent(event.clone()),
            };
            self.writer.send(proto::Message { union: Some(union) }).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendezvous_defaults_to_the_public_server_and_keeps_custom_ones() {
        let mut options = Options {
            id: "123456789".into(),
            password: String::new(),
            rendezvous: String::new(),
            key: String::new(),
            two_factor_code: String::new(),
            trust_device: false,
        };
        assert_eq!(options.rendezvous_address(), "rs-ny.rustdesk.com:21116");
        assert_eq!(options.signing_key().unwrap().len(), 32);
        options.rendezvous = " rs.example.com:9 ".into();
        assert_eq!(options.rendezvous_address(), "rs.example.com:9");
        options.key = "chave-que-nao-e-base64!!".into();
        assert!(options.signing_key().is_err());
    }

    #[test]
    fn refusals_reach_the_operator_as_a_cause_not_a_code() {
        let mut response = proto::PunchHoleResponse {
            failure: proto::punch_hole_response::Failure::Offline as i32,
            ..Default::default()
        };
        assert!(punch_failure(&response).contains("offline"));
        response.failure = proto::punch_hole_response::Failure::IdNotExist as i32;
        assert!(punch_failure(&response).contains("não existe"));
        response.other_failure = "servidor em manutenção".into();
        assert_eq!(punch_failure(&response), "servidor em manutenção");
        assert!(Session::login_error("Wrong Password").contains("incorreta"));
        assert!(Session::login_error("2FA Required").contains("duas etapas"));
        assert!(Session::login_error("Wrong 2FA Code").contains("30 segundos"));
        assert!(Session::login_error("qualquer outra").contains("qualquer outra"));
    }

    #[test]
    fn login_never_carries_the_password_and_declares_only_decodable_codecs() {
        let options = Options {
            id: "123456789".into(),
            password: "segredo".into(),
            rendezvous: String::new(),
            key: String::new(),
            two_factor_code: String::new(),
            trust_device: false,
        };
        let hash = proto::Hash {
            salt: "sal".into(),
            challenge: "desafio".into(),
        };
        let request = login_request(&options, &hash);
        assert_eq!(
            request.password,
            crypto::login_password("segredo", "sal", "desafio")
        );
        assert!(!request.password.windows(7).any(|w| w == b"segredo"));
        assert_eq!(request.my_id.len(), 9);
        let decoding = request.option.unwrap().supported_decoding.unwrap();
        assert_eq!((decoding.ability_vp8, decoding.ability_vp9), (1, 1));
        assert_eq!(
            (decoding.ability_h264, decoding.ability_h265, decoding.ability_av1),
            (0, 0, 0)
        );
    }

    #[test]
    fn the_login_presents_a_stable_device_identity_for_trust() {
        let options = Options {
            id: "123456789".into(),
            password: "segredo".into(),
            rendezvous: String::new(),
            key: String::new(),
            two_factor_code: String::new(),
            trust_device: false,
        };
        let hash = proto::Hash::default();
        let first = login_request(&options, &hash);
        let second = login_request(&options, &hash);
        // The machine matches a trusted device on all four of these, so none
        // may change from one connection to the next.
        assert_eq!(first.hwid, crate::rustdesk::device::identity());
        assert_eq!(first.hwid, second.hwid);
        assert_eq!(first.my_id, second.my_id);
        assert_eq!(first.my_name, second.my_name);
        assert_eq!(first.my_platform, second.my_platform);
        assert_eq!(first.my_id.len(), 9);
        assert!(first.my_id.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn a_multi_session_windows_machine_is_attached_without_asking() {
        let session = |sid, name: &str| proto::WindowsSession {
            sid,
            name: name.into(),
        };
        let with = |sessions, current_sid| proto::PeerInfo {
            windows_sessions: Some(proto::WindowsSessions {
                sessions,
                current_sid,
            }),
            ..Default::default()
        };
        // The machine names an active session: take that one.
        assert_eq!(
            choose_session(&with(
                vec![session(1, "Console"), session(3, "RDP: Lego")],
                3
            )),
            Some((3, "RDP: Lego".into()))
        );
        // It names none we were offered: take the first rather than stall.
        assert_eq!(
            choose_session(&with(vec![session(1, "Console")], 99)),
            Some((1, "Console".into()))
        );
        // A machine with a single desktop offers no choice at all.
        assert_eq!(choose_session(&proto::PeerInfo::default()), None);
        assert_eq!(choose_session(&with(vec![], 1)), None);
    }

    #[test]
    fn the_peers_current_display_sets_the_session_resolution() {
        let display = |width, height| proto::DisplayInfo {
            width,
            height,
            ..Default::default()
        };
        let info = proto::PeerInfo {
            hostname: "WIN-LAB".into(),
            displays: vec![display(1280, 800), display(1920, 1080)],
            current_display: 1,
            ..Default::default()
        };
        let peer = Session::describe(&info);
        assert_eq!((peer.width, peer.height), (1920, 1080));
        assert_eq!(peer.hostname, "WIN-LAB");
        // An out-of-range index must not lose the session.
        let peer = Session::describe(&proto::PeerInfo {
            current_display: 9,
            ..info.clone()
        });
        assert_eq!((peer.width, peer.height), (1280, 800));
        // A peer that reports no display leaves the resolution unknown, not wrong.
        let peer = Session::describe(&proto::PeerInfo::default());
        assert_eq!((peer.width, peer.height), (0, 0));
    }

    #[test]
    fn only_decodable_video_becomes_a_frame() {
        let frames = |data: &[u8]| proto::EncodedVideoFrames {
            frames: vec![proto::EncodedVideoFrame {
                data: data.to_vec(),
                key: true,
                pts: 0,
            }],
        };
        let event = video(proto::VideoFrame {
            union: Some(proto::video_frame::Union::Vp9s(frames(b"quadro"))),
            display: 0,
        });
        assert!(matches!(event, Event::Video { codec: Codec::Vp9, ref data, key: true } if data == b"quadro"));
        // H264 was never advertised; a peer sending it produces no picture.
        assert!(matches!(
            video(proto::VideoFrame {
                union: Some(proto::video_frame::Union::H264s(frames(b"x"))),
                display: 0,
            }),
            Event::Idle
        ));
        assert!(matches!(
            video(proto::VideoFrame {
                union: Some(proto::video_frame::Union::Vp8s(proto::EncodedVideoFrames::default())),
                display: 0,
            }),
            Event::Idle
        ));
    }
}

#[cfg(test)]
mod probe_tests {
    //! The machine's latency probe must go back with its flag untouched; the
    //! session dies quietly otherwise. Exercised over a loopback socket so the
    //! bytes on the wire are what is checked, not a helper's intent.
    use super::*;
    use prost::Message as _;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn the_probe_is_echoed_with_its_flag_untouched() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let machine = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0u8; 256];
            let read = socket.read(&mut buffer).await.unwrap();
            buffer.truncate(read);
            let (head, len) = crate::rustdesk::codec::decode_header(&buffer).unwrap().unwrap();
            proto::Message::decode(&buffer[head..head + len]).unwrap()
        });
        let stream = Stream::connect(&address.to_string()).await.unwrap();
        let (_, mut commands) = {
            let (reader, writer) = stream.split();
            (reader, Commands { writer })
        };
        commands
            .pong(proto::TestDelay {
                time: 7,
                from_client: false,
                last_delay: 12,
                target_bitrate: 800,
            })
            .await
            .unwrap();
        let echoed = machine.await.unwrap();
        let Some(message::Union::TestDelay(delay)) = echoed.union else {
            panic!("a resposta não foi um TestDelay");
        };
        assert!(!delay.from_client, "a máquina trataria isto como sondagem nossa e nunca enviaria a próxima");
        assert_eq!((delay.time, delay.last_delay, delay.target_bitrate), (7, 12, 800));
    }
}

#[cfg(test)]
mod login_probe_tests {
    //! A machine that probes latency while the login is still pending must
    //! get its probe back, or it never probes again. Played out against a fake
    //! machine on a loopback socket, in the clear, since the cipher is not
    //! what is under test.
    use super::*;
    use crate::rustdesk::codec;
    use prost::Message as _;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn send(socket: &mut tokio::net::TcpStream, message: proto::Message) {
        socket
            .write_all(&codec::encode(&message.encode_to_vec()).unwrap())
            .await
            .unwrap();
    }

    async fn receive(socket: &mut tokio::net::TcpStream, buffer: &mut Vec<u8>) -> proto::Message {
        loop {
            if let Some((head, len)) = codec::decode_header(buffer).unwrap() {
                if buffer.len() >= head + len {
                    let payload: Vec<u8> = buffer.drain(..head + len).skip(head).collect();
                    return proto::Message::decode(&payload[..]).unwrap();
                }
            }
            let mut chunk = [0u8; 4096];
            let read = socket.read(&mut chunk).await.unwrap();
            assert!(read > 0, "o cliente fechou");
            buffer.extend_from_slice(&chunk[..read]);
        }
    }

    #[tokio::test]
    async fn a_probe_sent_before_the_login_answer_is_echoed() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let machine = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = Vec::new();
            // Challenge, then a probe while the client is busy answering it.
            send(&mut socket, proto::Message {
                union: Some(message::Union::Hash(proto::Hash { salt: "s".into(), challenge: "c".into() })),
            }).await;
            let login = receive(&mut socket, &mut buffer).await;
            assert!(matches!(login.union, Some(message::Union::LoginRequest(_))));
            send(&mut socket, proto::Message {
                union: Some(message::Union::TestDelay(proto::TestDelay { time: 41, from_client: false, last_delay: 0, target_bitrate: 0 })),
            }).await;
            let echo = receive(&mut socket, &mut buffer).await;
            let Some(message::Union::TestDelay(delay)) = echo.union else {
                panic!("o cliente não ecoou a sondagem enviada durante o login");
            };
            assert!(!delay.from_client && delay.time == 41);
            // Only now does the machine answer the login.
            send(&mut socket, proto::Message {
                union: Some(message::Union::LoginResponse(proto::LoginResponse {
                    union: Some(proto::login_response::Union::PeerInfo(proto::PeerInfo {
                        hostname: "fake".into(),
                        displays: vec![proto::DisplayInfo { width: 8, height: 8, ..Default::default() }],
                        ..Default::default()
                    })),
                    enable_trusted_devices: false,
                })),
            }).await;
        });
        let mut stream = Stream::connect(&address.to_string()).await.unwrap();
        let options = Options {
            id: "1".into(),
            password: "p".into(),
            rendezvous: String::new(),
            key: String::new(),
            two_factor_code: String::new(),
            trust_device: false,
        };
        let peer = Session::login(&mut stream, &options).await.unwrap();
        assert_eq!(peer.hostname, "fake");
        machine.await.unwrap();
    }
}
