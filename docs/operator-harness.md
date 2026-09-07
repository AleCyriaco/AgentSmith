# Harness de operação — AgentSmith 0.12.2

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
