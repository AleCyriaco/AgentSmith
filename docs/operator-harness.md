# Harness de operação — AgentSmith 0.12.5

O contrato é comum aos adaptadores de API, clientes oficiais e modelos locais. Compatibilidade com o protocolo não garante competência visual ou latência: valide cada perfil com **Testar operador** e uma tarefa curta no Windows.

## Ciclo

1. Capturar uma imagem atual e reutilizar OCR somente se os pixels da região não mudaram.
2. Fornecer roteiro autorizado, etapa e critério, etapas anteriores e até quatro entradas recentes com indicação de mudança da tela.
3. Quando Operar e Verificar têm a mesma rota, uma chamada de texto propõe a próxima ação ou a conclusão. Rotas diferentes mantêm decisões separadas.
4. Validar JSON, campos, IDs, coordenadas, tamanho do texto e atalhos. Uma resposta inválida recebe uma única correção por perfil; depois utiliza somente a alternativa já configurada.
5. Enviar uma ação validada ao RDP após conferir se a observação ainda é atual. Nenhuma ferramenta local do provedor deve executar ações.
6. Observar novamente. A tela pode demorar até 1,2 segundo adicional para responder. Após três entradas sem mudança, parar para revisão.

Propostas de sucesso por texto continuam exigindo confirmação visual. Regras OCR explícitas são verificadas pelo motor. O harness não transforma ausência de evidência em sucesso nem remove condições de parada.

## Semântica

- `key`: um atalho simultâneo, por exemplo `{"kind":"key","keys":["ctrl","l"]}`. Ctrl+L pressupõe navegador ativo. Não agrupar atalhos sequenciais.
- `type_text`: digita até 400 caracteres; não pressiona Enter. Não transmite a senha pela linha de comando.
- Clique OCR: `{"kind":"click","target":0}` usa ID da observação atual com confiança suficiente.
- Clique visual: `{"kind":"click","x":120,"y":80}` usa pixels da imagem recebida, mesmo quando reduzida ou recortada. O motor faz a conversão para o RDP.
- `need_vision`: texto/OCR insuficiente. `inspect`: solicita recorte se habilitado. Nenhum dos dois envia entrada.
- `wait`: espera de 1 a 10 segundos. `blocked`: motivo concreto e respeito às restrições do roteiro.

## Custo e velocidade

O OCR é enviado em linhas compactas com até 120 elementos e orçamento de 12 KB para as linhas serializadas. Textos longos são limitados a 200 caracteres e omissões são indicadas; falta de informação deve solicitar visão. O roteiro autorizado é preservado integralmente para não perder restrições. Diagnósticos de execução não substituem a memória das últimas entradas. Conteúdo digitado não é copiado para essa memória.

Uma etapa incompleta na mesma rota passa de duas chamadas de texto para uma. A confirmação visual, correções de JSON e alternativas podem exigir chamadas adicionais. Limites de saída continuam limitados por adaptador; latência, raciocínio interno e cobrança dependem do modelo. Não há economia percentual medida ainda.

## Testar operador

Usa um botão fictício recebido por OCR e exige um clique com ID correto. Usa apenas o perfil selecionado, permite uma correção e mede o tempo total. Não abre a sessão Windows, não envia mouse/teclado e não valida capacidade de visão. É um teste mínimo de entendimento do contrato, não certificação de todas as tarefas.

## Validação

A suíte cobre JSON inválido, correção e alternativa, impedimentos reais preservados, clique fora da tela, IDs inventados, proposta textual falsa recusada pela visão, ausência de imagens no caminho de texto, atraso de atualização e liberação das teclas. O fluxo real precisa ser medido com a mesma tarefa e tela inicial para comparar provedores.

## Validação xAI neste Mac

No teste sintético de 7/9/2026, o perfil grok-4.6 via API retornou recusa textual ao contrato inicial. Com a descrição explícita de geração de uma proposta (execução pertence ao aplicativo) e `response_format: {"type":"json_object"}`, retornou `{"kind":"click","target":0}` em 14,25 s. Não é benchmark comparativo nem validação de operação visual real.

O modo JSON é aplicado somente a chamadas do harness via adaptador xAI/chat, conforme [documentação oficial](https://docs.x.ai/developers/model-capabilities/text/structured-outputs). A validação local continua necessária: JSON válido não garante ação correta. Os demais adaptadores mantêm contrato por texto e validação local, sem presumir suporte a parâmetros exclusivos da xAI.


## Login xAI: saída estruturada nativa (0.12.2)

O adaptador Grok Build envia `_meta.outputSchema` em `session/prompt` e usa `_meta.structuredOutput` da resposta final. Esses campos são usados pelo [cliente headless oficial](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/src/headless.rs). Texto intermediário da conversa deixa de ser interpretado como ação quando um contrato foi solicitado. Se o cliente não devolver a estrutura ou indicar erro de validação, a chamada falha sem executar entradas e apresenta uma mensagem específica.

Cada função tem seu contrato: plano, ação OCR, observação/ação combinada, verificação OCR, ação visual e verificação visual. O seletor pertence às instruções internas, não ao texto do roteiro ou da tela. Todos os objetos fecham campos extras; clique OCR exige `target` e clique visual exige `x`/`y`. A validação semântica local continua obrigatória. O modo de teste simples e o cliente Gemini mantêm seu protocolo anterior.

A suíte cobre seleção dos contratos, separação de coordenadas e IDs, prioridade da saída estruturada sobre prosa e rejeição de metadados ausentes, erro ou cancelamento. Os testes ao vivo são sintéticos e não executam tarefas no Windows.


## Ritmo abaixo de 100 ms (0.12.3)

Capturas aceitam de 20 a 2.000 ms em passos de 1 ms na interface, na configuração salva e no processo FreeRDP. A pausa adicional após ações aceita de 0 a 3.000 ms. O preset Turbo usa 50 ms, pausa zero e imagens de até 1.280 pixels; o padrão Equilibrado continua igual. Zero não dispensa a observação de uma nova imagem nem a verificação da ação. A frequência configurada é um alvo; transferência dos quadros, codificação, OCR, rede e inferência podem limitar o ritmo efetivo. Capturas não disparam chamadas de IA por si mesmas.


## Login Claude: protocolo de entrada e saída (0.12.4)

Claude Code 2.1.179 rejeita `--input-format stream-json --output-format json` antes da inferência. O adaptador passa a usar `stream-json` nos dois sentidos, com `--verbose` exigido para a saída de eventos. O leitor consome somente o evento final `result`, rejeitando eventos intermediários isolados, resultados duplicados, JSON malformado e resultados com erro. A compatibilidade com uma resposta JSON única é mantida.

O teste real da conexão com a sessão existente respondeu OK em 2,9 segundos; não enviou ações ao Windows. Os testes de regressão cobrem a leitura do fluxo e impedem que texto intermediário seja tratado como resposta final. [Referência oficial da CLI](https://code.claude.com/docs/en/cli-reference).


## Diagnóstico de encerramento do Claude (0.12.5)

Um código de saída não zero não explica sozinho a falha. O adaptador agora examina o evento final de erro e stderr antes de descartar a resposta. Distingue protocolo, autenticação, acesso ao modelo, limites de uso, orçamento, turnos, rede e serviço; motivos desconhecidos continuam identificados como desconhecidos. Não copia a saída bruta para avisos ou histórico, pois ela pode conter dados da tarefa ou da conta. Se o processo fechar stdin antecipadamente, o erro final continua disponível. Saída de processo malsucedido nunca é usada como ação.

Após a nova ocorrência relatada, o pedido exato do botão Testar respondeu normalmente pela sessão existente. Isso não identifica retrospectivamente a causa do código 1 anterior: os detalhes haviam sido descartados. Os testes simulam um cliente que encerra com código 1, incluem fechamento antecipado de stdin e verificam a classificação sem expor texto privado.
