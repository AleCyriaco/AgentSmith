# AgentSmith

Desktop macOS para planejamento e operação visual de máquinas Windows, com múltiplos provedores de LLM.

Prévia 0.11.1. Este build foi produzido para Apple Silicon/macOS 26+ porque as bibliotecas FreeRDP disponíveis nesta máquina têm esse deployment target. A aplicação não foi homologada em versões anteriores nem Intel.

## Desenvolvimento

Requer Node.js, Rust, Xcode Command Line Tools e FreeRDP 3 (`brew install freerdp`).

```sh
npm ci
npm run helper
npm run vision
npm run ocr
npm run desktop
```

## Testes e pacote

```sh
npm test
cargo test --manifest-path src-tauri/Cargo.toml
npm run bundle
```

O teste HTTP abre um servidor efêmero em localhost e precisa de permissão para escutar sockets. Não usa nenhuma chave real nem chama fornecedores externos.

O comando de bundle gera `src-tauri/target/release/bundle/macos/AgentSmith.app`. As bibliotecas nativas são copiadas para os recursos e suas referências são ajustadas para caminhos relativos. A assinatura é local/ad hoc, sem notarização.

## Módulos

- `src/catalog.ts`: 10 fornecedores e 3 perfis de servidores locais; IDs de modelo permanecem configuráveis.
- `src/App.tsx`: central, modelos, máquinas e rotas.
- `src/SessionPanel.tsx`: visualização e controles RDP compartilhados entre a Central e a janela destacada.
- `src/operation.css`: área RDP ampliada, modo de foco e janela separada.
- `src/LoginConnection.tsx`: escolha, instalação, início, cancelamento e teste do login oficial.
- `src-tauri/src/browser_auth.rs`: processos oficiais, OAuth e transporte de tarefas sem API key.
- `src-tauri/src/llm.rs`: Responses, Chat Completions, Anthropic Messages e Bedrock Converse.
- `src-tauri/src/executor.rs`: plano, verificação, ações validadas e pausa.
- `src-tauri/src/remote.rs`: processo RDP, snapshots e comandos de entrada.
- `src-tauri/src/store.rs`: SQLite e Keychain.
- `native/rdp_worker.c`: biblioteca FreeRDP e canal privado por pipes.

RustDesk e NanoKVM são conectores futuros, explicitamente desabilitados para conexão. O conector RDP foi compilado e seu tratamento de falha testado, mas a captura/entrada e a execução completa exigem uma máquina Windows real para homologação. Os adaptadores HTTP precisam de validação com contas e modelos habilitados em cada fornecedor.

Credenciais não acompanham o código. Não são lidas de outros projetos ou configuradas automaticamente. Screenshots da máquina remota são enviados ao modelo visual escolhido; o modo somente local permite apenas endpoints loopback e desabilita redirecionamentos e proxies do ambiente.

Veja também os documentos de arquitetura e de uso entregues junto ao pacote.

Veja [Login pelo navegador](docs/browser-login.md) para autenticação, dependências e limites verificados.

## Visão local integrada — 0.6.0

O motor llama.cpp b10830 (25 MB) acompanha o app, com aceleração Metal. Para reconstruir os recursos em Apple Silicon, rode `npm run vision` antes de `npm run bundle`. O script fixa o arquivo oficial e verifica SHA-256; não requer Ollama, LM Studio, Python ou Homebrew na máquina do usuário.

Em Provedores e modelos → Visão local: baixar modelo, testar visão, adicionar perfil e selecionar o perfil em Roteamento de IA. Catálogo inicial: SmolVLM 500M Q8 + projetor Q8 (546 MB) e Qwen2.5 VL 3B Q4_K_M + projetor Q8 (2,77 GB). São modelos Apache-2.0. O menor tem limitações importantes para ler interfaces e coordenadas.

`local_engine.rs` gerencia download HTTPS com hashes e revisões fixadas em `local_models.json`, progresso, cancelamento, carregamento sob demanda e descarregamento após 120 segundos sem uso. Um servidor por vez, uma requisição por vez, contexto limitado a 8192 tokens, cache de prompts desabilitado, quatro threads de CPU e GPU Metal. O canal usa loopback, porta efêmera, token efêmero e cliente sem proxy/redirecionamentos. Modelos ficam em Application Support/com.agentsmith.desktop/local-vision; capturas não são gravadas em disco por esta integração. Não há serviço global nem inicialização com o sistema.

O teste visual usa uma figura sintética; não certifica precisão de cliques no Windows. Perfis e rotas existentes são preservados. Para impedir envio a provedores externos, use Somente local e escolha perfis locais também para planejamento. O motor encerra em cancelamento da inferência, falha de resposta, descarga manual ou saída normal do app. Encerramentos abruptos do processo principal podem exigir encerrar um processo residual; não há garantia de limpeza após falha do sistema.

## Senha salva — 0.6.1

Conectar tenta primeiro a credencial do Chaves vinculada ao ID da máquina, endereço, porta, domínio e usuário. A senha não passa pela interface. Apenas ausência de credencial abre o formulário de senha; bloqueio/recusa do Chaves produz mensagem distinta. O formulário oferece salvar a senha para próximas conexões. Editar máquina consulta somente metadados e exibe se existe senha salva, mantendo-a quando o campo fica em branco. Erros de gravação aparecem dentro do modal. Todos os pontos de conexão, incluindo janela destacada e alteração da resolução, usam o mesmo fluxo.

## OCR, recortes e Qwen3-VL — 0.9.0

Qwen3-VL 2B Instruct Q8_0 + projetor Q8_0 (2,28 GB) está disponível em Visão local. A conversão ggml-org usa revisão e SHA-256 fixos. Baixar e adicionar o perfil não altera as rotas atuais. O motor permanece llama.cpp b10830; não exige outro aplicativo.

`npm run bundle` compila também `native/ocr_worker.swift` com Apple Vision. O helper recebe somente o PNG da sessão remota via stdin e devolve textos, confiança e caixas em pixels da tela (origem superior esquerda). Não captura o desktop do Mac nem salva imagens. Há limite de 5 segundos; sem uma regra OCR explícita, falhas de OCR permitem continuar com o modelo visual. O índice de confiança do OCR não é uma garantia estatística de acerto.

Em **Ritmo de operação**, OCR auxiliar e recortes podem ser ligados/desligados. OCR auxiliar fornece textos/posições ao modelo; não conclui etapas sozinho. Em cada etapa, **Texto esperado (OCR)** permite escolher um campo/visor e definir um resultado exato. Essa regra substitui a verificação pelo LLM para a etapa: exige texto e pontuação iguais, confiança >=0,98, uma única ocorrência correspondente inteiramente na região, mesma resolução e pixels da região ainda iguais à captura verificada. Use-a apenas quando esse estado visível comprovar toda a etapa. Mudanças de posição da janela exigem ajustar a região; em loops, o resultado precisa deixar de corresponder antes de ser confirmado novamente no ciclo. Etapas sem regra continuam com verificação visual pelo LLM.

O operador pode solicitar `inspect` para obter um recorte. Essa ação apenas muda a observação e conta no limite de ações; não envia entrada ao Windows. Cliques na imagem reduzida/recortada são convertidos para pixels da sessão, com limites validados. A visão geral retorna após duas ações ou teclado/rolagem/espera. Pixels alterados no recorte invalidam a ação pendente. OCR de uma captura antiga não acompanha uma imagem nova. Respostas visuais têm limite de saída (512 tokens no motor local; até 1024 nos adaptadores HTTP), e o prompt pede JSON conciso.

**Comparar leitura** usa a mesma captura congelada em memória e o texto esperado fornecido pelo usuário como gabarito, que não é enviado ao modelo. Executa OCR e duas passagens de tela inteira/recorte com o modelo local escolhido. Exibe tempo, leitura e acerto exato. A primeira consulta pode incluir carregamento. É um teste de transcrição, não uma homologação de decisões, coordenadas ou tarefas completas. Não modifica rotas nem envia entradas ao Windows; pode ser cancelado. A região, o texto esperado e a evidência da tarefa ficam no histórico SQLite; as capturas continuam transitórias em memória, sem arquivo de imagem criado pela integração.

Teste reproduzível opcional: `reading_test::integration::same_image_local_comparison`, ignorado nos testes comuns porque exige Metal, OCR e pesos baixados. Recebe imagem, regra e diretório dos modelos por variáveis `AGENTSMITH_BENCH_*`. A precisão final deve ser medida em tarefas reais na máquina Windows.


## Parte superior recolhível — 0.9.1

Na barra da conexão, use **Recolher parte superior** (Collapse upper panel). O andamento detalhado, a apresentação, os contadores e os controles de conexão ficam ocultos, liberando altura para a RDP. A faixa compacta mantém o nome da máquina, o estado da tarefa e o botão **Expandir parte superior**. Durante uma execução, Pausar e Parar continuam acessíveis; Esc continua funcionando. A escolha fica salva neste Mac e é compartilhada com a janela destacada. Recolher não desconecta a sessão nem apaga o roteiro.


### Espaço máximo para RDP — 0.9.2

Com a parte superior recolhida, a RDP passa a ocupar toda a altura restante da janela. Os painéis de roteiro e histórico, rodapé e caixa auxiliar de digitação ficam ocultos; a digitação direta na sessão continua disponível no controle manual. **Ampliar RDP**, na faixa compacta, oculta também a barra lateral e o cabeçalho. **Sair do foco** e **Expandir parte superior** restauram a interface. Os painéis continuam montados para preservar rascunhos e conexão.


## Editar e excluir planos — 0.10.0

Na Central, expanda os painéis, selecione um plano no histórico e use **Editar plano** ou **Excluir plano** abaixo das etapas. A edição permite alterar título, instruções, ações e resultados esperados, incluir, remover e reordenar etapas. Planos ainda não executados são atualizados; para planos já executados, **Salvar nova versão** cria um plano pronto e preserva o anterior. A nova versão não inicia automaticamente e requer configurar novamente regras OCR e repetição.

Excluir pede confirmação do plano selecionado e remove sua entrada do histórico; não desfaz ações no Windows. Durante execução ou espera de um ciclo, pause ou pare primeiro. Alterações concorrentes são rejeitadas para evitar sobrescrever uma versão mais recente.


## Repositório

O repositório contém o código da versão 0.11.1 e os scripts para reconstruir os componentes nativos. Dependências instaladas, modelos baixados, aplicativos compilados, credenciais e dados locais de execução não são versionados. Consulte [Primeiros passos](docs/getting-started.md) e [Arquitetura](docs/architecture.md).


## OCR + texto primeiro — 0.11.0

Operar e Verificar agora aceitam perfis de texto. Com OCR ativado, a captura é convertida localmente em JSON com texto, confiança e posições; as consultas principais não incluem imagem. O operador escolhe IDs observados para cliques, e o motor calcula as coordenadas. IDs ausentes, baixa confiança e respostas inválidas solicitam apoio visual.

Em **Roteamento de IA → Apoio visual**, selecione um modelo que aceite imagens. Na ausência de uma seleção explícita, os perfis visuais já configurados em Operar/Verificar continuam disponíveis como apoio, sem mudar as preferências salvas. Modelo de texto pode ser local ou em nuvem. Os modelos multimodais instalados também podem responder apenas a texto; esta versão não adiciona novos pesos ao catálogo.

Falta de informação, pedido do modelo, falha de OCR/resposta de texto ou uma entrada sem mudança nos pixels ativa o apoio visual. Três entradas sem mudança encerram a tentativa com pedido de atenção. Isso detecta tela inalterada, não todo tipo de progresso semântico. Ícones, foco e campos vazios frequentemente precisam de visão. Nenhuma decisão em cache é reexecutada.

Critérios explícitos de texto exato continuam sendo conferidos diretamente pelo motor, agora lendo primeiro somente a região definida. Essa regra nunca é substituída por uma opinião do LLM. As demais condições usam o verificador textual com IDs de evidência; respostas incertas passam à visão. OCR e LLMs podem errar: não há promessa de melhoria de acerto sem medir tarefas reais.

O cache mantém no máximo duas observações em memória durante uma execução. Compara pixels exatos da região e resolução antes de reutilizar; alterações fora de uma região não invalidam sua leitura, mas exigem nova observação para a tela inteira. Pausar/retomar, reiniciar e um novo ciclo criam um novo cache. Capturas não são salvas em arquivos. Antes de enviar ações do caminho textual ou confirmar sucesso, o motor confere novamente a captura para descartar respostas antigas.

Veja [o fluxo e a validação da versão](docs/ocr-text-first.md).


## Correção de bloqueios genéricos — 0.11.1

Respostas como `blocked: impedimento`, vazias ou copiadas dos exemplos não encerram imediatamente a tarefa. No caminho textual, solicitam apoio visual. No caminho visual, o modelo recebe uma única solicitação de correção; se a resposta continuar inválida, o próximo perfil visual configurado é consultado. O motor não envia entradas durante essas tentativas e continua conferindo a validade da captura antes da ação final.

Um bloqueio concreto, como falta de senha ou autorização, permanece um bloqueio e não dispara tentativas para contorná-lo. O aviso inclui modelo e etapa. O histórico identifica respostas rejeitadas e as teclas enviadas (sem registrar conteúdo digitado). A regressão de resposta literal “impedimento” foi reproduzida em teste HTTP simulado, inclusive a passagem ao segundo modelo e a preservação de bloqueios reais.
