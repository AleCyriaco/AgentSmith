//! Live check against a real RustDesk machine. Ignored by default: it needs a
//! reachable peer, and its credentials come from the environment so they never
//! enter the repository or a command line that gets logged.
//!
//! ```sh
//! read -rs RUSTDESK_PASSWORD && export RUSTDESK_PASSWORD
//! export RUSTDESK_ID=123456789
//! cargo test --manifest-path src-tauri/Cargo.toml rustdesk_live -- --ignored --nocapture
//! ```
//!
//! Optional: `RUSTDESK_SERVER` and `RUSTDESK_KEY` for a self-hosted server,
//! and `RUSTDESK_SEND_INPUT=1` to also move the pointer on the remote machine.
#![cfg(test)]
use crate::rustdesk::{
    decoder::Decoder,
    input::{self, TYPE_MOVE},
    session::{Codec, Event, Options, Session},
};

fn options() -> Option<Options> {
    let id = std::env::var("RUSTDESK_ID").ok()?;
    Some(Options {
        id,
        password: std::env::var("RUSTDESK_PASSWORD").unwrap_or_default(),
        rendezvous: std::env::var("RUSTDESK_SERVER").unwrap_or_default(),
        key: std::env::var("RUSTDESK_KEY").unwrap_or_default(),
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a reachable RustDesk machine and RUSTDESK_ID/RUSTDESK_PASSWORD"]
async fn rustdesk_live_session_connects_decodes_and_accepts_input() {
    let Some(options) = options() else {
        panic!("Defina RUSTDESK_ID (e RUSTDESK_PASSWORD) antes de rodar este teste.");
    };
    println!("→ servidor de encontro: {}", options.rendezvous_address());
    println!("→ ID solicitado: {}", options.id);
    if options.password.is_empty() {
        println!("  aviso: sem senha; a máquina precisará aprovar a sessão manualmente");
    }

    let session = Session::connect(&options)
        .await
        .unwrap_or_else(|error| panic!("conexão falhou: {error}"));
    assert!(
        session.is_secure(),
        "a sessão abriu sem criptografia, o que não deveria ser possível"
    );
    let (peer, mut events, mut commands) = session.split();
    println!(
        "✓ autenticado e cifrado · {} · {} · RustDesk {} · {}×{}",
        if peer.hostname.is_empty() { "sem hostname" } else { &peer.hostname },
        if peer.platform.is_empty() { "sem plataforma" } else { &peer.platform },
        peer.version,
        peer.width,
        peer.height
    );
    assert!(peer.width > 0 && peer.height > 0, "a máquina não relatou display");

    commands.request_refresh().await.expect("pedido de quadro");

    let mut decoder: Option<(Codec, Decoder)> = None;
    let mut decoded = 0;
    let mut idle = 0;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while decoded < 3 && std::time::Instant::now() < deadline {
        match events.next().await.expect("fluxo interrompido") {
            Event::Video { codec, data, key } => {
                if !matches!(&decoder, Some((current, _)) if *current == codec) {
                    println!("→ codec negociado: {codec:?}");
                    decoder = Some((codec, Decoder::new(codec).expect("decodificador")));
                }
                let (_, active) = decoder.as_mut().unwrap();
                match active.decode(&data).expect("decodificação") {
                    Some(picture) => {
                        decoded += 1;
                        assert_eq!(
                            picture.rgba.len(),
                            picture.width as usize * picture.height as usize * 4
                        );
                        let opaque = picture.rgba.chunks_exact(4).all(|p| p[3] == 255);
                        let varied = picture.rgba.chunks_exact(4).any(|p| p[..3] != picture.rgba[..3]);
                        println!(
                            "✓ quadro {decoded}: {}×{} · {} bytes codificados · chave={key} · opaco={opaque} · imagem não uniforme={varied}",
                            picture.width,
                            picture.height,
                            data.len()
                        );
                        assert!(opaque, "o quadro saiu com transparência");
                    }
                    None => println!("  quadro sem imagem (aguardando quadro-chave)"),
                }
            }
            Event::Ping(delay) => commands.pong(delay).await.expect("resposta de latência"),
            Event::Closed(reason) => panic!("a máquina encerrou a sessão: {reason}"),
            Event::Idle => idle += 1,
        }
    }
    assert!(decoded >= 3, "só {decoded} quadros decodificados em 30s (idle={idle})");

    if std::env::var("RUSTDESK_SEND_INPUT").as_deref() == Ok("1") {
        // Moves the pointer to the middle of the screen and clicks nothing.
        let middle = vec![input::Input::Mouse {
            mask: input::mask(0, TYPE_MOVE),
            x: peer.width as i32 / 2,
            y: peer.height as i32 / 2,
        }];
        commands.send(&middle).await.expect("envio de entrada");
        commands.send(&input::release()).await.expect("liberação");
        println!("✓ ponteiro movido para o centro e modificadores liberados");
    } else {
        println!("→ entrada não enviada (defina RUSTDESK_SEND_INPUT=1 para mover o ponteiro)");
    }
    println!("✓ sessão RustDesk validada de ponta a ponta");
}
