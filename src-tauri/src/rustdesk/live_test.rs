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
//! `RUSTDESK_2FA` with the current six-digit code when the machine asks for a
//! second factor, `RUSTDESK_SEND_INPUT=1` to also move the pointer on the
//! remote machine, and `RUSTDESK_SOAK_SECS=120` to then hold the session still
//! and measure how long the machine ever goes quiet.
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
        two_factor_code: std::env::var("RUSTDESK_2FA").unwrap_or_default(),
        // Asking a real machine to trust this Mac is a lasting change; the
        // check never does it as a side effect.
        trust_device: false,
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a reachable RustDesk machine and RUSTDESK_ID/RUSTDESK_PASSWORD"]
async fn rustdesk_live_session_connects_decodes_and_accepts_input() {
    let Some(options) = options() else {
        panic!("Defina RUSTDESK_ID (e RUSTDESK_PASSWORD) antes de rodar este teste.");
    };
    let normalized: String = options.id.chars().filter(|c| !c.is_whitespace()).collect();
    println!("→ servidor de encontro: {}", options.rendezvous_address());
    println!("→ ID solicitado: {normalized}");
    println!(
        "→ chave do servidor: {}",
        if options.key.trim().is_empty() {
            "pública do RustDesk".to_string()
        } else {
            format!("própria, {} caracteres", options.key.trim().len())
        }
    );
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
    let path = session.path;
    let (peer, mut events, mut commands) = session.split();
    println!(
        "✓ autenticado e cifrado · via {} · {} · {} · RustDesk {} · {}×{}",
        path,
        if peer.hostname.is_empty() { "sem hostname" } else { &peer.hostname },
        if peer.platform.is_empty() { "sem plataforma" } else { &peer.platform },
        peer.version,
        peer.width,
        peer.height
    );
    match &peer.session {
        Some((sid, name)) => println!("→ sessão do Windows escolhida: {name} (sid {sid})"),
        None => println!("→ a máquina ofereceu uma única sessão"),
    }
    assert!(peer.width > 0 && peer.height > 0, "a máquina não relatou display");

    commands.request_refresh().await.expect("pedido de quadro");

    let mut decoder: Option<(Codec, Decoder)> = None;
    let mut latest: Option<Vec<u8>> = None;
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
                match active.decode(&data, true).expect("decodificação") {
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
                        latest = Some(picture.rgba);
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
        // Two distinct pointer positions, nothing clicked and nothing typed.
        // Whether the move shows up in the video depends on the machine drawing
        // its cursor into the stream, so a still screen is reported, not failed.
        let mut changed = false;
        for (name, x, y) in [
            ("canto superior esquerdo", peer.width as i32 / 4, peer.height as i32 / 4),
            ("canto inferior direito", peer.width as i32 * 3 / 4, peer.height as i32 * 3 / 4),
        ] {
            commands
                .send(&[input::Input::Mouse {
                    mask: input::mask(0, TYPE_MOVE),
                    x,
                    y,
                }])
                .await
                .expect("envio de entrada");
            println!("→ ponteiro movido para o {name} ({x}, {y})");
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while std::time::Instant::now() < until {
                let Ok(event) = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    events.next(),
                )
                .await
                else {
                    break;
                };
                match event.expect("fluxo interrompido") {
                    Event::Video { codec, data, .. } => {
                        let Some((current, active)) = decoder.as_mut() else { continue };
                        if *current != codec {
                            continue;
                        }
                        if let Ok(Some(picture)) = active.decode(&data, true) {
                            if latest.as_deref() != Some(picture.rgba.as_slice()) {
                                changed = true;
                            }
                            latest = Some(picture.rgba);
                        }
                    }
                    Event::Ping(delay) => {
                        commands.pong(delay).await.expect("resposta de latência")
                    }
                    Event::Closed(reason) => panic!("a máquina encerrou a sessão: {reason}"),
                    Event::Idle => {}
                }
            }
        }
        commands
            .send(&input::release())
            .await
            .expect("liberação de botões e modificadores");
        println!(
            "✓ entrada aceita pela sessão · tela {}",
            if changed {
                "mudou depois do movimento"
            } else {
                "não mudou (a máquina pode não desenhar o cursor no vídeo)"
            }
        );
    } else {
        println!("→ entrada não enviada (defina RUSTDESK_SEND_INPUT=1 para mover o ponteiro)");
    }
    // Soak: hold the session with nothing happening and measure how long the
    // machine ever goes quiet. The picture is dropped after five silent
    // seconds, so any gap near that is the fault being looked for.
    if let Ok(secs) = std::env::var("RUSTDESK_SOAK_SECS").map(|v| v.parse::<u64>().unwrap_or(0)) {
        if secs > 0 {
            println!("→ imersão: {secs}s parado, medindo o silêncio entre mensagens");
            let started = std::time::Instant::now();
            let mut last = std::time::Instant::now();
            let (mut pings, mut frames, mut idle, mut skipped) = (0u32, 0u32, 0u32, 0u32);
            let mut worst = std::time::Duration::ZERO;
            let mut worst_at = 0u64;
            while started.elapsed().as_secs() < secs {
                let event = tokio::time::timeout(std::time::Duration::from_secs(40), events.next()).await;
                let gap = last.elapsed();
                last = std::time::Instant::now();
                if gap > worst {
                    worst = gap;
                    worst_at = started.elapsed().as_secs();
                }
                if gap.as_millis() > 2000 {
                    println!("  ! silêncio de {}ms aos {}s", gap.as_millis(), started.elapsed().as_secs());
                }
                match event {
                    Err(_) => panic!("40s sem nenhuma mensagem da máquina"),
                    Ok(Err(error)) => panic!("fluxo interrompido aos {}s: {error}", started.elapsed().as_secs()),
                    Ok(Ok(Event::Ping(delay))) => {
                        pings += 1;
                        commands.pong(delay).await.expect("resposta de latência");
                    }
                    Ok(Ok(Event::Video { codec, data, .. })) => {
                        frames += 1;
                        if let Some((current, active)) = decoder.as_mut() {
                            if *current == codec {
                                match active.decode(&data, false) {
                                    Ok(_) => {}
                                    Err(_) => skipped += 1,
                                }
                            }
                        }
                    }
                    Ok(Ok(Event::Closed(reason))) => panic!("a máquina encerrou aos {}s: {reason}", started.elapsed().as_secs()),
                    Ok(Ok(Event::Idle)) => idle += 1,
                }
            }
            println!(
                "✓ imersão de {secs}s: {pings} pings · {frames} quadros ({skipped} ilegíveis) · {idle} outras · maior silêncio {}ms aos {worst_at}s",
                worst.as_millis()
            );
            assert!(worst.as_millis() < 5000, "a máquina ficou {}ms em silêncio; a imagem cairia", worst.as_millis());
        }
    }
    println!("✓ sessão RustDesk validada de ponta a ponta");
}
