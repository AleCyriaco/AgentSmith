# AgentSmith para macOS

> Atualização 0.11.0: Operar e Verificar usam OCR + texto por padrão, com apoio visual sob demanda. Consulte [o fluxo atual e sua validação](ocr-text-first.md). As seções de versões anteriores abaixo documentam a evolução.

**Prévia 0.10.0: Apple Silicon, macOS 26 ou posterior.**

Abra `AgentSmith.app`. A aba do navegador é apenas uma prévia visual e não tem acesso aos recursos nativos.

## Primeiros passos

1. Abra **Provedores e modelos** e adicione um perfil.
2. Para OpenAI, Anthropic, Google ou xAI, escolha **Login pelo navegador**, clique em **Entrar pelo navegador** e conclua o acesso na página oficial. Se necessário, use **Instalar componente** primeiro (requer Node.js LTS). Use `default` para o modelo padrão da conta. A opção **API key** continua disponível. Para Ollama ou LM Studio, inicie o servidor local e carregue um modelo.
3. Use **Testar conexão** e depois **Salvar perfil**. O teste confirma uma resposta de texto; salvar sozinho não confirma autenticação.
4. Em **Roteamento de IA**, selecione os modelos de planejamento, operação e verificação. Operação e verificação exigem um modelo que aceite imagens.
5. Se quiser manter toda a inferência neste Mac, ative **Somente local** e use perfis com endpoints localhost.
6. Cadastre uma máquina em **Máquinas**, com RDP, endereço, usuário e porta.
7. Conecte, forneça um roteiro, clique em **Preparar roteiro**, revise as etapas e clique em **Executar**.

**Pausar** ou **Assumir** interrompe a automação. O clique na imagem remota e o teclado funcionam no controle manual após a pausa. A caixa de texto permite enviar até 400 caracteres por ação.

Se a conexão cair, o progresso fica salvo. Reconecte a mesma máquina e use **Retomar**. O executor verifica a condição da etapa antes de agir novamente.

## Mais espaço para o Windows — versão 0.3.0

A RDP ocupa toda a largura da área principal. A missão e o histórico ficam logo abaixo; os cadastros e as configurações continuam disponíveis.

- **Ampliar RDP** recolhe os demais painéis. **Sair do foco** restaura a Central e preserva seu roteiro em edição.
- **Destacar** abre a visualização em uma janela separada, que pode ser movida para outro monitor. **Tela cheia** amplia essa janela.
- **Voltar à Central**, **Trazer de volta** ou fechar a janela destacada devolve a visualização, sem encerrar a conexão RDP.
- Mouse, teclado, **Assumir**, **Digitar** e **Ctrl Alt End** continuam disponíveis na janela da sessão.

Na versão 0.4.0, **Tela** permite escolher a resolução (1280 × 800 até 2560 × 1440) e a escala do Windows (100%, 125%, 150% ou 200%). Os valores ficam salvos por máquina e são enviados ao reconectar. O padrão é 1600 × 900 a 100%. O servidor pode exigir sair da conta para aplicar uma nova escala.

**Zoom** ajusta apenas a visualização no Mac: **Ajustar à janela** mostra a tela inteira sem ampliar além de 100%; os valores de 50% a 200% permitem escolher o tamanho. Quando a imagem ultrapassar a área, use as barras de rolagem ou Option + rolagem para navegar pela imagem. A rolagem normal continua sendo enviada ao Windows no controle manual.

A IA recebe a captura original na resolução da sessão. Zoom e rolagem locais não alteram essa captura; as coordenadas dos cliques são convertidas para os pixels remotos.

## O que esta prévia inclui

- Cadastro de vários perfis e vários provedores, com modelo principal e alternativa por função.
- Dez fornecedores: OpenAI, Anthropic, Google, xAI, Amazon Bedrock, Alibaba, DeepSeek, Moonshot, Z.ai e ByteDance/BytePlus.
- Ollama, LM Studio e outros servidores locais com API compatível.
- Chaves e senhas no Chaves do macOS; logins administrados pelos clientes oficiais Codex, Claude Code, Gemini CLI e Grok Build.
- Planejamento, execução visual, verificação, pausa e histórico local.
- Conector RDP nativo compilado com FreeRDP.

## Limites atuais

- O fluxo completo precisa ser validado com sua conta de LLM e uma máquina Windows de teste. Nenhuma conta foi conectada automaticamente.
- RustDesk e NanoKVM aparecem como integrações previstas; ainda não controlam máquinas nesta versão.
- Uma sessão e uma tarefa ativa por vez. Reconexão manual.
- Na versão Grok Build 1.0.4 verificada, a integração de login anuncia apenas texto. Use-a para planejamento e escolha outro perfil com visão para operação/verificação.
- A sessão de login é compartilhada com o programa oficial neste Mac. O plano precisa oferecer acesso aos modelos; o login não cria uma assinatura.
- O teste do perfil é textual; habilitar visão no cadastro não certifica a capacidade visual do modelo.
- Não inclui modelos locais instalados, assinatura Developer ID ou notarização Apple. O pacote é uma compilação local de desenvolvimento.
- Não há arraste de mouse, redirecionamento de arquivos, áudio, RD Gateway ou múltiplos monitores nesta prévia.
- O Mac precisa permanecer ligado e o aplicativo aberto.

O documento `AgentSmith-arquitetura.md` detalha a estrutura, as decisões e os próximos marcos. O código-fonte acompanha a entrega em `AgentSmith-codigo-fonte.zip`.


## Ritmo e capturas — versão 0.5.0

Na barra da RDP, abra **Ritmo**. Pause uma tarefa em execução antes de salvar. A mudança do intervalo é aplicada ao conector ativo sem reconectar. A pausa entre ações e o tamanho da imagem para IA valem na próxima execução ou retomada.

| Perfil | Captura | Pausa após ação | Largura máxima para IA |
| --- | --- | --- | --- |
| Ágil | 150 ms | 250 ms | 1280 px |
| Equilibrado | 300 ms | 650 ms | 1600 px |
| Econômico | 1000 ms | 1200 ms | 1600 px |

Os intervalos podem ser ajustados: captura de 100 a 2000 ms e pausa de 100 a 3000 ms. Imagens podem ser enviadas em até 1280, 1600, 1920 ou 2560 pixels de largura, ou na resolução original. A proporção é mantida; imagens pequenas não são ampliadas. O controlador converte cliques da imagem reduzida para as coordenadas reais do Windows.

A interface consulta imagens no intervalo escolhido e só recebe o PNG quando há um novo quadro. A IA recebe a captura mais recente sob demanda, ao verificar ou escolher uma ação, não todas as capturas da visualização. Após agir, o executor respeita a pausa mínima e espera um novo quadro. O histórico registra o tempo das respostas de operação e verificação; tempo de rede e inferência não é controlado pelo intervalo de captura.

**Armazenamento:** não existe um arquivo de screenshots no histórico SQLite. O quadro atual e as imagens usadas por chamadas em andamento ficam em memória. Para Codex, uma cópia PNG é escrita em pasta temporária privada e removida ao encerrar normalmente a chamada, inclusive em cancelamentos normais. Falhas abruptas podem deixar resíduos temporários. A aplicação não garante apagamento físico nem controla retenção de clientes oficiais e provedores externos.


## Visão local integrada — versão 0.6.0

Em **Provedores e modelos → Visão local**, baixe **SmolVLM 500M** (546 MB, visão básica) ou **Qwen2.5 VL 3B** (2,77 GB, mais detalhe). O motor llama.cpp já acompanha o aplicativo e usa Metal no Mac. Não precisa instalar Ollama, LM Studio ou outro programa.

Depois do download, use **Testar visão** e **Adicionar aos meus modelos**. Em **Roteamento de IA**, escolha o perfil em Operar e/ou Verificar. A adição do perfil preserva as rotas existentes. Modelos pequenos podem errar leitura e coordenadas; o teste sintético confirma a interpretação básica de imagens, sem certificar automação de Windows.

O motor carrega um modelo por vez, sob demanda, e libera a memória após dois minutos sem uso. **Liberar memória** descarrega imediatamente; o ícone de lixeira remove os arquivos baixados. Downloads mostram progresso, podem ser cancelados e são verificados por SHA-256. Arquivos concluídos são reaproveitados ao tentar novamente.

As capturas desta integração ficam em memória. Os modelos permanecem em Application Support/com.agentsmith.desktop/local-vision até serem removidos. Não há conta, API key externa ou serviço iniciado com o Mac. Para evitar envio a qualquer provedor externo, habilite **Somente local** e selecione apenas modelos locais no roteamento.


## Senha salva — versão 0.6.1

Clique em **Conectar** para usar automaticamente a senha guardada no Chaves do macOS. O formulário de senha aparece quando não há credencial salva para aquele endereço e usuário; nele, **Salvar senha no Chaves do macOS** permite guardar a senha.

Em **Editar máquina**, o campo informa **Senha salva** quando a credencial existe. O campo fica vazio para preservar a senha; digite apenas para substituí-la. Alterar endereço, porta, domínio ou usuário exige a credencial do novo destino. Falhas ao salvar aparecem dentro do formulário. Uma solicitação de autorização do próprio Chaves do macOS é independente do formulário de senha do Windows.


## Andamento no topo — versão 0.6.2

Um aviso fixo acompanha a tarefa na Central e na janela destacada. Ele mostra análise da tela, escolha de ação, envio de entrada, espera, pausa, conclusão ou motivo do bloqueio. O tempo exibido mede a fase atual; a contagem de etapas corresponde a resultados verificados. Pausar, Retomar e Configurar IA ficam acessíveis no aviso.

O SmolVLM 500M reconheceu a figura sintética de teste, mas falhou no formato de resposta necessário para executar roteiros. Ele continua disponível para visão básica experimental; use um modelo mais capaz para Operar e Verificar. Respostas inválidas continuam sendo recusadas, e a mensagem agora identifica o perfil que falhou.


## Parar tarefa — versão 0.6.3

**Parar** encerra a tarefa, preserva o histórico e as etapas verificadas. O botão aparece no aviso de andamento e junto de Pausar/Retomar. Uma tarefa parada não pode ser retomada com Retomar; a partir da versão 0.6.4, use Reiniciar para começar uma nova execução do mesmo roteiro. **Pausar** continua permitindo retomar.

Durante a execução, pressione **Esc** com o AgentSmith em foco, na janela principal ou na RDP destacada. A parada cancela a espera pela IA e impede novas entradas da execução. Ações que já chegaram ao Windows não são desfeitas. A sessão RDP permanece conectada. Sem automação em execução, Esc continua disponível para o controle manual do Windows. O atalho não é global ao macOS.


## Reiniciar roteiro — versão 0.6.4

**Reiniciar** cria uma nova execução do mesmo roteiro e começa pela primeira etapa, sem pedir à IA para preparar as etapas novamente. A contagem de ações e as evidências começam do zero. A execução anterior fica intacta no histórico. O Windows permanece no estado atual: o operador observa e verifica cada etapa antes de agir.

O botão aparece no aviso do topo, na janela destacada e ao lado de Pausar/Retomar/Parar. Conecte a máquina do roteiro para habilitá-lo. Se uma tarefa estiver em execução, pause ou pare antes de reiniciar.


## Repetir roteiro (loop) — versão 0.7.0

No topo ou nos controles do roteiro, clique em **Repetir**. Escolha **Por duração** (minutos ou horas, até 7 dias) ou **De um horário até outro**, usando o horário local do Mac. Janelas como 22h às 2h atravessam a meia-noite. Se você estiver dentro da janela escolhida, começa agora; se ela já terminou, começa na próxima ocorrência. Não é um agendamento diário recorrente: uma única janela será executada.

Defina o intervalo entre ciclos, de 1 a 3600 segundos, e clique em **Iniciar repetição**. A máquina do roteiro precisa estar conectada. Uma nova execução preserva o roteiro e o histórico anterior. Cada ciclo limpa as evidências da execução anterior e começa pela primeira etapa, observando e verificando o resultado antes de agir. O limite de ações configurado vale para cada ciclo; o topo mostra também o total acumulado. O histórico registra inícios e conclusões, mantendo os 300 registros mais recentes por execução.

Ao fim do período, a espera pela IA é cancelada e o executor deixa de enviar novas ações. Ações já enviadas ao Windows não são desfeitas; um ciclo interrompido pelo prazo não é contado como concluído. Pausar preserva o progresso e o prazo original continua contando. Parar ou Esc encerra a repetição, inclusive durante a espera pelo horário de início ou pelo próximo ciclo. Erros e desconexões suspendem a sequência para revisão manual. **Repetir** cria um novo período; **Reiniciar** inicia uma execução única do roteiro.

Mantenha o aplicativo aberto, o Mac acordado e a RDP conectada. Não há serviço em segundo plano nem despertar automático do Mac. Ao reabrir o app, o agendamento fica pausado e exige retomada manual; seu prazo não é ampliado.


## Dias da semana — versão 0.7.1

Em **Repetir → Dias da semana e horários**, marque **Seg, Ter, Qua, Qui, Sex, Sáb e/ou Dom**. Há atalhos **Seg a sex** e **Todos os dias**. Escolha a faixa de horário e o intervalo entre ciclos. É obrigatório selecionar ao menos um dia.

Esta opção se repete semanalmente até Parar/Esc. Ao terminar a faixa de horário, interrompe o ciclo atual e aguarda a próxima ocorrência selecionada, sem contar um ciclo incompleto como concluído. O dia marcado é o dia de início: segunda, 22h às 2h, inclui a madrugada de terça. A programação usa o fuso local do Mac e calcula as próximas janelas no calendário, incluindo mudanças de horário de verão. Ao retomar após dias sem executar, não acumula execuções atrasadas: usa a janela atual ou a próxima disponível.

Os dias ficam salvos na execução e aparecem no aviso de andamento. Ao abrir Repetir sobre uma execução semanal, seus dias, horários e intervalo são reaproveitados no formulário. **Por duração** e **De um horário até outro** continuam disponíveis para períodos únicos. O app deve permanecer aberto, o Mac acordado e o Windows conectado. Pausas e erros continuam exigindo retomada manual; reabrir o app não ativa automaticamente um agendamento pausado.

## Datas de início e fim do loop — versão 0.7.2

Em **Repetir → Dias da semana e horários**, preencha **Data de início** e **Data de fim**, além dos dias e horários. As datas usam o calendário local do Mac. A data inicial impede janelas iniciadas antes dela. A data final é inclusiva: permite executar nesse dia, dentro do horário escolhido, e corta qualquer janela que ultrapasse a meia-noite ao terminar esse dia. Exemplo: segunda, 22h às 2h, com data final na segunda, termina à meia-noite, sem avançar para terça.

Início em branco significa a partir de agora; fim em branco mantém o loop até você parar. É possível marcar um início com várias semanas de antecedência. A prévia informa a próxima janela; datas inválidas, fim anterior ao início e períodos sem nenhum dia/horário disponível impedem iniciar. As datas são salvas e reaparecem ao abrir Repetir sobre a execução. O topo mostra também a data final do loop, quando informada.

Ao esgotar a última janela permitida, o estado muda para **Período encerrado**, sem criar outro ciclo. Retomar depois da data final não prolonga o agendamento. Continuam valendo os requisitos de manter o app aberto, o Mac acordado e a máquina conectada.

## Recolher barra lateral — versão 0.7.3

Use o botão **Recolher barra lateral**, no topo, à esquerda de Workspace, para esconder o menu e ampliar a área de trabalho. O mesmo botão passa a **Expandir barra lateral** para recuperar todos os atalhos. A escolha fica salva neste Mac e é mantida ao reabrir o aplicativo. Funciona em todas as páginas da janela principal; o modo Ampliar RDP e a janela destacada mantêm seus controles próprios.

## Idiomas e identidade visual — versão 0.8.0

O seletor no topo permite alternar entre **Português (BR)**, **English** e **Español**, inclusive com a barra lateral recolhida. A preferência é salva neste Mac e sincronizada com a janela RDP destacada. Menus, formulários, botões, ajuda, dias da semana, prévias de datas e avisos conhecidos do aplicativo acompanham o idioma. Nomes cadastrados, roteiros, evidências da IA, histórico original e mensagens técnicas externas mantêm seu conteúdo; a troca da interface não reescreve os dados nem muda o idioma dos prompts de execução.

A paleta agora usa roxo e lavanda no menu, botões, painéis e ícone do aplicativo. Abaixo do logo, **by prodigy-lab** abre **https://prodigy-lab.com** no navegador padrão do Mac. O link não contém a vírgula que acompanhava o endereço na mensagem.


## Novidades da versão 0.9

- Qwen3-VL 2B disponível para baixar em **Provedores e modelos → Visão local**. Aproximadamente 2,28 GB. Adicione o perfil e escolha-o em Roteamento de IA somente após comparar os resultados.
- **Ritmo de operação**: controles de OCR nativo e recortes.
- **Texto esperado (OCR)**, abaixo de cada etapa: selecione o visor/campo na imagem, informe o resultado exato e salve. Uma regra explícita permite verificar essa etapa sem uma consulta ao LLM. Precisa de sessão conectada; não use um texto isolado se ele não comprovar todo o resultado da etapa.
- Na mesma janela, **Comparar leitura** mostra OCR, tela inteira e recorte, duas passagens por modelo. Escolha outro modelo e repita usando a mesma captura. O texto esperado não é enviado ao modelo.
- As capturas continuam apenas em memória. A regra e as evidências textuais ficam no histórico local.
- O modelo e o roteamento anteriores foram mantidos. Esta versão continua sendo uma prévia; os resultados de leitura não garantem a execução completa de tarefas.


## Parte superior recolhível — 0.9.1

Na barra da conexão, use **Recolher parte superior** (Collapse upper panel). O andamento detalhado, a apresentação, os contadores e os controles de conexão ficam ocultos, liberando altura para a RDP. A faixa compacta mantém o nome da máquina, o estado da tarefa e o botão **Expandir parte superior**. Durante uma execução, Pausar e Parar continuam acessíveis; Esc continua funcionando. A escolha fica salva neste Mac e é compartilhada com a janela destacada. Recolher não desconecta a sessão nem apaga o roteiro.


### Espaço máximo para RDP — 0.9.2

Com a parte superior recolhida, a RDP passa a ocupar toda a altura restante da janela. Os painéis de roteiro e histórico, rodapé e caixa auxiliar de digitação ficam ocultos; a digitação direta na sessão continua disponível no controle manual. **Ampliar RDP**, na faixa compacta, oculta também a barra lateral e o cabeçalho. **Sair do foco** e **Expandir parte superior** restauram a interface. Os painéis continuam montados para preservar rascunhos e conexão.


## Editar e excluir planos — 0.10.0

Na Central, expanda os painéis, selecione um plano no histórico e use **Editar plano** ou **Excluir plano** abaixo das etapas. A edição permite alterar título, instruções, ações e resultados esperados, incluir, remover e reordenar etapas. Planos ainda não executados são atualizados; para planos já executados, **Salvar nova versão** cria um plano pronto e preserva o anterior. A nova versão não inicia automaticamente e requer configurar novamente regras OCR e repetição.

Excluir pede confirmação do plano selecionado e remove sua entrada do histórico; não desfaz ações no Windows. Durante execução ou espera de um ciclo, pause ou pare primeiro. Alterações concorrentes são rejeitadas para evitar sobrescrever uma versão mais recente.
