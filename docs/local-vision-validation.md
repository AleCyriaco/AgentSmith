# AgentSmith 0.9 — validação de visão local

Medição em 06/09/2026 neste Mac Apple Silicon, usando o código Rust otimizado (`--release`), llama.cpp b10830 e Apple Vision. Uma única imagem **sintética** de Calculadora, 1600 × 900, com expressão e histórico como distrações. Região do visor: x=260, y=270, largura=370, altura=80. Gabarito: `16.666.666`. O gabarito não foi enviado ao modelo. Não é uma medição de tarefas reais no Windows.

| Leitor | Modo / passagem | Tempo | Texto exato correto |
|---|---|---:|---|
| Apple Vision | OCR | 0.829 s | Sim |
| Qwen3-VL 2B · Interfaces | Inteira · 1 | 18.263 s | Não |
| Qwen3-VL 2B · Interfaces | Recorte · 1 | 0.651 s | Sim |
| Qwen3-VL 2B · Interfaces | Inteira · 2 | 5.257 s | Não |
| Qwen3-VL 2B · Interfaces | Recorte · 2 | 0.655 s | Sim |
| Apple Vision | OCR | 1.362 s | Sim |
| Qwen2.5 VL 3B · Mais detalhe | Inteira · 1 | 24.474 s | Sim |
| Qwen2.5 VL 3B · Mais detalhe | Recorte · 1 | 1.344 s | Sim |
| Qwen2.5 VL 3B · Mais detalhe | Inteira · 2 | 10.241 s | Sim |
| Qwen2.5 VL 3B · Mais detalhe | Recorte · 2 | 1.315 s | Sim |

As primeiras consultas com tela inteira incluem verificar os arquivos e carregar o modelo (18,3 s no Qwen3; 24,5 s no Qwen2.5). As passagens seguintes reutilizam o modelo carregado. Tempos do LLM incluem a chamada e sua resposta, mas não a preparação prévia do recorte. Os tempos de OCR incluem iniciar o helper e reconhecer o texto. Não houve isolamento de outros aplicativos do Mac; são observações pontuais, sem percentis ou garantia de velocidade.

O Qwen3 devolveu `12.323.232 + 4.343.434 = 16.666.666` nas duas leituras da tela inteira, apesar da região solicitada. Falhou no critério de transcrição exata. No recorte, retornou apenas `16.666.666` nas duas passagens. O Qwen2.5 acertou as quatro leituras. O OCR acertou as duas consultas, entre 0,83 e 1,36 s. A amostra contém apenas um estado de uma tela sintética, repetido; não representa uma taxa geral de acerto.

**Decisão:** manter o roteamento existente. Qwen3 fica como candidato, especialmente para recortes. OCR auxilia todos os modelos quando habilitado; concluir etapas só por OCR exige configurar uma regra explícita de texto/região. A escolha final precisa avaliar cliques, decisões e resultados nas mesmas tarefas Windows, além da transcrição. Moondream2 não foi incluído nesta entrega.

**Limite da validação remota:** a máquina cadastrada “Teste” inicialmente retornou `ERRCONNECT_CONNECT_FAILED`. A sessão chegou a conectar e exibir o desktop 1600 × 900, mas caiu novamente antes da comparação da Calculadora. Por isso, a medição na sessão Windows continua pendente. O app inclui a janela **Texto esperado (OCR) → Comparar leitura** para repetir a medição assim que a sessão estiver disponível.

**Verificações:** 27 testes da interface; 48 testes comuns de Rust; teste separado dos adaptadores HTTP; benchmark integrado com ambos os modelos; leitura Apple Vision de imagem sintética; controles nativos de OCR/recorte visíveis; seleção de texto e conversão de região conferidas na prévia da interface. Os testes de coordenadas cobrem recorte, escala, limites, origem superior esquerda e detecção de mudança na região. Reiniciar preserva a regra OCR, e dados antigos migram com os novos padrões.

Para reproduzir a imagem, use `scripts/make_vision_fixture.swift`. O teste opcional `same_image_local_comparison` recebe `AGENTSMITH_BENCH_ROOT` (contendo local-vision), `AGENTSMITH_BENCH_IMAGE`, `AGENTSMITH_BENCH_RULE` (JSON de TextCheck) e `AGENTSMITH_BENCH_RESULT` (saída JSON). Rodar com `cargo test --release ... -- --ignored --nocapture`. Aceleradores do macOS e porta loopback precisam estar disponíveis.
