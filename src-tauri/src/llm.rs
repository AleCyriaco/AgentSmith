use crate::{model::*, store};
use reqwest::Url;
use serde_json::{json, Value};

pub fn endpoint(p: &Profile, local_only: bool) -> Result<Url, String> {
    if p.auth_method == "local_engine" {
        crate::local_engine::validate(p)?;
        return Url::parse("http://127.0.0.1").map_err(|_| "Endereço local inválido".into());
    }
    if p.auth_method == "browser" {
        crate::browser_auth::validate(p, local_only)?;
        return Url::parse("https://localhost").map_err(|_| "Endereço inválido".into());
    }
    if p.auth_method != "api_key" {
        return Err("Método de autenticação inválido.".into());
    }
    let url = Url::parse(p.base_url.trim_end_matches('/'))
        .map_err(|_| "Endereço do provedor inválido.")?;
    let loopback = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Não coloque credenciais, parâmetros ou fragmentos no endereço do provedor.".into(),
        );
    }
    if local_only && !loopback {
        return Err("Modo somente local: o endpoint precisa estar neste Mac (localhost).".into());
    }
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err("Use HTTPS; HTTP é aceito apenas em localhost.".into());
    }
    if !["responses", "chat", "anthropic", "bedrock"].contains(&p.protocol.as_str()) {
        return Err("Formato de API não suportado.".into());
    }
    Ok(url)
}
pub fn request_body(
    p: &Profile,
    system: &str,
    prompt: &str,
    image: Option<&str>,
) -> Result<(String, Value), String> {
    if image.is_some() && !p.vision {
        return Err("O perfil selecionado não está habilitado para visão.".into());
    }
    let base = p.base_url.trim_end_matches('/');
    let encoded = image
        .map(|i| {
            i.strip_prefix("data:image/png;base64,")
                .ok_or("A captura deve ser PNG.")
        })
        .transpose()?;
    match p.protocol.as_str() {
        "responses" => {
            let mut c = vec![json!({"type":"input_text","text":prompt})];
            if let Some(i) = image {
                c.push(json!({"type":"input_image","image_url":i}));
            }
            Ok((
                format!("{base}/responses"),
                json!({"model":p.model,"store":false,"instructions":system,"input":[{"role":"user","content":c}],"max_output_tokens":if image.is_some(){1024}else{8192}}),
            ))
        }
        "anthropic" => {
            let mut c = vec![json!({"type":"text","text":prompt})];
            if let Some(i) = encoded {
                c.push(json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":i}}));
            }
            Ok((
                format!("{base}/messages"),
                json!({"model":p.model,"system":system,"max_tokens":if image.is_some(){1024}else{8192},"messages":[{"role":"user","content":c}]}),
            ))
        }
        "bedrock" => {
            let mut url = Url::parse(base).map_err(|_| "Endpoint inválido")?;
            url.path_segments_mut()
                .map_err(|_| "Endpoint inválido")?
                .extend(["model", &p.model, "converse"]);
            let mut c = vec![json!({"text":prompt})];
            if let Some(i) = encoded {
                c.push(json!({"image":{"format":"png","source":{"bytes":i}}}));
            }
            Ok((
                url.to_string(),
                json!({"system":[{"text":system}],"messages":[{"role":"user","content":c}],"inferenceConfig":{"maxTokens":if image.is_some(){1024}else{8192}}}),
            ))
        }
        "chat" => {
            let c = if let Some(i) = image {
                json!([{"type":"text","text":prompt},{"type":"image_url","image_url":{"url":i}}])
            } else {
                json!(prompt)
            };
            Ok((
                format!("{base}/chat/completions"),
                json!({"model":p.model,"messages":[{"role":"system","content":system},{"role":"user","content":c}],"stream":false,"max_tokens":if image.is_some(){1024}else{8192}}),
            ))
        }
        _ => Err("Formato de API não suportado.".into()),
    }
}
pub fn response_text(protocol: &str, v: &Value) -> Result<String, String> {
    let texts: Vec<String> = match protocol {
        "responses" => {
            if v["status"].as_str().is_some_and(|s| s != "completed") {
                return Err("O modelo não concluiu a resposta.".into());
            }
            v["output"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|m| m["type"] == "message")
                .flat_map(|m| m["content"].as_array().into_iter().flatten())
                .filter(|c| c["type"] == "output_text")
                .filter_map(|c| c["text"].as_str().map(String::from))
                .collect()
        }
        "anthropic" => {
            if v["stop_reason"] == "max_tokens" {
                return Err("Resposta truncada pelo limite de tokens.".into());
            }
            v["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["type"] == "text")
                .filter_map(|c| c["text"].as_str().map(String::from))
                .collect()
        }
        "bedrock" => {
            if v["stopReason"] == "max_tokens" {
                return Err("Resposta truncada pelo limite de tokens.".into());
            }
            v["output"]["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c["text"].as_str().map(String::from))
                .collect()
        }
        _ => {
            if v["choices"][0]["finish_reason"] == "length" {
                return Err("Resposta truncada pelo limite de tokens.".into());
            }
            vec![v["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string()]
        }
    };
    let text = texts.join("\n");
    if text.trim().is_empty() {
        Err("O provedor retornou uma resposta vazia ou recusou a solicitação.".into())
    } else {
        Ok(text)
    }
}
pub struct Failure {
    pub message: String,
    pub retryable: bool,
}
pub async fn call(
    p: &Profile,
    key: &str,
    local_only: bool,
    system: &str,
    prompt: &str,
    image: Option<&str>,
) -> Result<String, Failure> {
    let fail = |message: String| Failure {
        message,
        retryable: false,
    };
    endpoint(p, local_only).map_err(fail)?;
    if p.auth_method == "local_engine" {
        return crate::local_engine::generate(p, system, prompt, image)
            .await
            .map_err(fail);
    }
    if p.auth_method == "browser" {
        return crate::browser_auth::generate(p, system, prompt, image)
            .await
            .map_err(fail);
    }
    let (url, body) = request_body(p, system, prompt, image).map_err(fail)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|_| fail("Não foi possível iniciar o cliente HTTP.".into()))?;
    let mut req = client.post(url).json(&body);
    if p.protocol == "anthropic" {
        req = req
            .header("anthropic-version", "2023-06-01")
            .header("x-api-key", key);
    } else if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let res = req.send().await.map_err(|_| Failure {
        message: format!("{}: conexão indisponível ou tempo esgotado.", p.name),
        retryable: true,
    })?;
    let status = res.status();
    if !status.is_success() {
        return Err(Failure {
            message: format!(
                "{}: HTTP {}. Confira chave, modelo, região e acesso na conta.",
                p.name,
                status.as_u16()
            ),
            retryable: status.as_u16() == 429 || status.is_server_error(),
        });
    }
    let body = res
        .bytes()
        .await
        .map_err(|_| fail("Falha ao receber a resposta.".into()))?;
    if body.len() > 8 * 1024 * 1024 {
        return Err(fail("Resposta excede o limite.".into()));
    }
    let v: Value = serde_json::from_slice(&body)
        .map_err(|_| fail("O provedor não retornou JSON válido.".into()))?;
    response_text(&p.protocol, &v).map_err(fail)
}
pub async fn routed(
    s: &Settings,
    role: &str,
    system: &str,
    prompt: &str,
    image: Option<&str>,
) -> Result<(String, String), String> {
    let ids = s.routes.get(role).ok_or("Configure a rota de modelos.")?;
    let mut errors = vec![];
    for id in ids {
        let p = s
            .profiles
            .iter()
            .find(|p| &p.id == id && p.enabled)
            .ok_or("A rota contém um perfil ausente ou desativado.")?;
        if image.is_some() && !p.vision {
            errors.push(format!("{}: sem visão habilitada", p.name));
            continue;
        }
        if let Err(e) = endpoint(p, s.local_only) {
            errors.push(e);
            continue;
        }
        let key = if ["browser", "local_engine"].contains(&p.auth_method.as_str()) {
            String::new()
        } else {
            store::get_secret(&p.id, &p.base_url).or_else(|e| {
                if ["ollama", "lmstudio", "local"].contains(&p.vendor.as_str()) {
                    Ok(String::new())
                } else {
                    Err(e)
                }
            })?
        };
        match call(p, &key, s.local_only, system, prompt, image).await {
            Ok(t) => return Ok((t, p.name.clone())),
            Err(e) => {
                errors.push(e.message);
                if !e.retryable {
                    break;
                }
            }
        }
    }
    Err(if errors.is_empty() {
        format!("Escolha ao menos um modelo para {role}.")
    } else {
        errors.join(" • ")
    })
}
pub fn parse_json<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    let text = text.trim();
    let text = if let Some(t) = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
    {
        t.strip_suffix("```")
            .ok_or("Bloco JSON incompleto.")?
            .trim()
    } else {
        text
    };
    serde_json::from_str(text)
        .map_err(|_| "O modelo retornou uma estrutura inválida. Nenhuma ação foi executada.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile(protocol: &str) -> Profile {
        Profile {
            id: uuid::Uuid::new_v4().to_string(),
            vendor: "openai".into(),
            name: "Test".into(),
            protocol: protocol.into(),
            base_url: "https://example.com/v1".into(),
            model: "test".into(),
            vision: true,
            enabled: true,
            auth_method: "api_key".into(),
        }
    }
    #[test]
    fn private_mode_cannot_egress() {
        let mut p = profile("chat");
        assert!(endpoint(&p, true).is_err());
        p.base_url = "http://127.0.0.1:11434/v1".into();
        assert!(endpoint(&p, true).is_ok());
        p.base_url = "http://127.0.0.1.attacker.com/v1".into();
        assert!(endpoint(&p, true).is_err());
        p.base_url = "https://user:key@example.com/v1".into();
        assert!(endpoint(&p, false).is_err());
    }
    #[test]
    fn wires_images_in_each_dialect() {
        for proto in ["chat", "responses", "anthropic", "bedrock"] {
            let (_, v) = request_body(
                &profile(proto),
                "s",
                "p",
                Some("data:image/png;base64,aGVsbG8="),
            )
            .unwrap();
            assert!(v.to_string().contains("aGVsbG8="));
        }
        let mut p = profile("chat");
        p.vision = false;
        assert!(request_body(&p, "s", "p", Some("data:image/png;base64,AA==")).is_err());
    }
    #[test]
    fn incomplete_responses_are_not_actions() {
        assert!(response_text("responses", &json!({"status":"incomplete"})).is_err());
        assert!(parse_json::<Value>("```json\n{\"a\":1}\n```").is_ok());
        assert!(parse_json::<Value>("explanation {\"a\":1}").is_err());
    }
    #[test]
    fn parses_all_response_dialects() {
        assert_eq!(
            response_text("chat", &json!({"choices":[{"message":{"content":"ok"}}]})).unwrap(),
            "ok"
        );
        assert_eq!(response_text("anthropic",&json!({"content":[{"type":"thinking","thinking":"secret"},{"type":"text","text":"ok"}]})).unwrap(),"ok");
        assert_eq!(
            response_text(
                "bedrock",
                &json!({"output":{"message":{"content":[{"text":"ok"}]}}})
            )
            .unwrap(),
            "ok"
        );
        assert_eq!(response_text("responses",&json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}]})).unwrap(),"ok");
    }

    #[tokio::test]
    async fn http_adapters_send_correct_auth_paths_and_images() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for proto in ["chat", "responses", "anthropic", "bedrock"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut p = profile(proto);
            p.base_url = format!("http://{}", listener.local_addr().unwrap());
            let dialect = proto.to_string();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = vec![];
                let mut buf = [0; 4096];
                let (headers, body) = loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(at) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                        let h = String::from_utf8_lossy(&bytes[..at]).to_lowercase();
                        let len: usize = h
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .unwrap()
                            .parse()
                            .unwrap();
                        if bytes.len() >= at + 4 + len {
                            break (h, bytes[at + 4..at + 4 + len].to_vec());
                        }
                    }
                };
                assert!(headers.contains(if dialect == "anthropic" {
                    "x-api-key: fixture-key"
                } else {
                    "authorization: bearer fixture-key"
                }));
                if dialect == "anthropic" {
                    assert!(headers.contains("anthropic-version: 2023-06-01"));
                }
                let parsed: Value = serde_json::from_slice(&body).unwrap();
                assert!(parsed.to_string().contains("aGVsbG8="));
                let response=match dialect.as_str(){"responses"=>json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}]}),"anthropic"=>json!({"content":[{"type":"text","text":"ok"}]}),"bedrock"=>json!({"output":{"message":{"content":[{"text":"ok"}]}}}),_=>json!({"choices":[{"message":{"content":"ok"}}]})}.to_string();
                let expected = match dialect.as_str() {
                    "responses" => "/responses",
                    "anthropic" => "/messages",
                    "bedrock" => "/model/test/converse",
                    _ => "/chat/completions",
                };
                assert!(headers.starts_with(&format!("post {expected} http/1.1")));
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await.unwrap();
            });
            let result = call(
                &p,
                "fixture-key",
                true,
                "system",
                "prompt",
                Some("data:image/png;base64,aGVsbG8="),
            )
            .await;
            assert_eq!(result.map_err(|e| e.message).unwrap(), "ok");
            server.await.unwrap();
        }
    }
}
