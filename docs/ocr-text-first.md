# AgentSmith 0.11.0 — OCR e texto como caminho principal

## Fluxo

1. Capturar a sessão Windows, sem capturar o desktop do Mac.
2. Havendo critério explícito, ler primeiro a região configurada e conferir o texto exato no motor. Confirmar somente com a resolução esperada, confiança mínima, correspondência única e pixels ainda iguais. Em repetição, exigir uma nova ocorrência após não correspondência.
3. Ler a tela por OCR ou reutilizar a leitura em memória se seus pixels estiverem iguais.
4. Enviar JSON de textos, confiança e caixas ao modelo de Verificar, sem imagem. Confirmações precisam citar IDs existentes de alta confiança. Esta é uma verificação do LLM, sujeita a erros; somente critérios explícitos são determinísticos.
5. Se não confirmado, enviar a observação ao modelo de Operar, sem imagem. Ele retorna uma ação ou solicita apoio visual.
6. O motor resolve os IDs OCR para coordenadas e valida a ação. Um clique textual não aceita coordenadas livres. IDs e caixas só valem para a observação atual.
7. Conferir se a captura continua válida antes da ação. Enviar mouse/teclado exclusivamente à sessão Windows. Aguardar atualização e repetir.

O apoio visual recebe imagens se OCR falhar/não fornecer informação, se o modelo pedir, se a resposta textual for inválida/indisponível, se uma entrada não mudar os pixels ou após duas esperas. Continua aceitando recortes e conversão de coordenadas. Um pedido de bloqueio por falta de autorização não é contornado. Se o apoio visual não estiver configurado, a tarefa pede atenção com instrução de configuração.

## Configuração e compatibilidade

- **Planejar:** modelo de texto para o roteiro.
- **Operar:** modelo de texto para decisões a partir do OCR.
- **Verificar:** modelo de texto para condições sem regra explícita.
- **Apoio visual:** modelo multimodal sob demanda. Vazio usa apenas os perfis com visão que já estavam atribuídos a Operar/Verificar. Nenhum perfil alheio às rotas é escolhido automaticamente.
- **Ritmo → OCR nativo:** ativado usa o novo fluxo. Desativado usa visão, preservando as regras explícitas OCR.
- **Somente local:** continua validando endpoints e autenticação em toda consulta, inclusive no apoio visual.

Não há migração de máquinas, tarefas, credenciais ou roteamento. Modelos multimodais existentes podem ser usados no caminho textual sem receber imagem. Esta versão não adiciona um novo modelo de texto ao catálogo nem baixa pesos automaticamente.

## Cache e limites

Cache de até duas leituras por execução, com região, resolução, captura e resultado OCR. Apenas a comparação exata dos pixels autoriza reutilização. Uma leitura de região não serve como leitura de tela inteira. Um novo ciclo/retomada não herda cache. Imagens ficam na memória; o histórico registra tempos, reutilização, evidências e motivo do apoio visual.

Não se repetem ações em cache: cada nova entrada precisa de decisão atual. Três entradas consecutivas sem mudança nos pixels pedem atenção. Essa heurística não mede progresso semântico em todas as interfaces; relógios/animações podem mudar pixels sem cumprir o objetivo. Ícones, campos vazios e foco de teclado continuam dependendo frequentemente de visão. O executor preserva pausas, Esc, limite de ações e janelas de repetição.

## Validação

- Testes de cache: mudança de um pixel invalida; mudança fora da região preserva; resolução diferente invalida; região parcial não substitui tela inteira; memória limitada.
- Testes de ações: clique por ID correto; rejeição de ID ausente, baixa confiança, caixa fora da tela, coordenadas injetadas e tipos não autorizados.
- Testes de evidência: IDs reais de alta confiança obrigatórios para confirmação textual.
- Teste HTTP com modelo somente textual: verificação e operação sem imagem, saída limitada, clique convertido, solicitação de visão pelo verificador/operador e regra explícita impedindo que o LLM confirme sucesso.
- Testes de roteamento: seleção visual explícita, compatibilidade com rotas antigas, exclusão de perfis textuais/desativados e preservação das preferências.
- Regressões de pausa/parada, repetição, edição de planos, geometria e idiomas.

Os testes automatizados não medem a qualidade das decisões de um LLM real. Comparar latência, taxa de conclusão e uso de apoio visual nas mesmas tarefas Windows ainda é necessário para afirmar ganho de desempenho ou acerto.
