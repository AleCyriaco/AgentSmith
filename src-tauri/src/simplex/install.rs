//! Official standalone CLI download. Verify before ever executing downloaded bytes.
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};
use tokio::io::AsyncWriteExt;

pub async fn client() -> Result<PathBuf, String> {
    if let Some(path) = super::client::binary() {
        return Ok(path);
    }
    let (asset, expected) = match std::env::consts::ARCH {
        "aarch64" => (
            "simplex-chat-macos-aarch64",
            "1ab8d76ad151ffd6166f0c76daa94bb79005b42f8aae22fcdedd0b7b3a034d04",
        ),
        "x86_64" => (
            "simplex-chat-macos-x86-64",
            "750814cd65d90c8dd2673c7232971e48606bd9833ee610a2554ce36b7ab6be86",
        ),
        _ => return Err("O cliente integrado requer um Mac Apple Silicon ou Intel.".into()),
    };
    let dir = super::host::data_dir()?.join("tools");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|_| "Não foi possível preparar a pasta do cliente.")?;
    let path = dir.join("simplex-chat-7.0.2");
    if path.is_file() {
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|_| "Não foi possível verificar o cliente SimpleX.")?;
        if format!("{:x}", Sha256::digest(&bytes)) == expected {
            return Ok(path);
        }
    }
    let temp = dir.join(format!("download-{}.part", uuid::Uuid::new_v4()));
    let result = async {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|_| "Falha ao preparar o download.")?;
        let response = http
            .get(format!(
                "https://github.com/simplex-chat/simplex-chat/releases/download/v7.0.2/{asset}"
            ))
            .send()
            .await
            .map_err(|_| "Não foi possível baixar o cliente oficial SimpleX.")?
            .error_for_status()
            .map_err(|_| "Download do cliente SimpleX indisponível.")?;
        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&temp)
            .await
            .map_err(|_| "Falha ao salvar o cliente.")?;
        let mut hash = Sha256::new();
        let mut size = 0usize;
        while let Some(bytes) = stream.next().await {
            let bytes = bytes.map_err(|_| "Download do cliente interrompido.")?;
            size += bytes.len();
            if size > 400_000_000 {
                return Err("Download do cliente excedeu o tamanho esperado.");
            }
            hash.update(&bytes);
            file.write_all(&bytes)
                .await
                .map_err(|_| "Falha ao salvar o cliente.")?;
        }
        file.sync_all()
            .await
            .map_err(|_| "Falha ao salvar o cliente.")?;
        drop(file);
        if format!("{:x}", hash.finalize()) != expected {
            return Err("A verificação do cliente SimpleX falhou. O arquivo não será executado.");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o700))
                .await
                .map_err(|_| "Falha ao preparar o cliente.")?;
        }
        tokio::fs::rename(&temp, &path)
            .await
            .map_err(|_| "Falha ao instalar o cliente SimpleX.")?;
        Ok(path)
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    result.map_err(str::to_string)
}
