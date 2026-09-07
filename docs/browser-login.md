# Autenticação por navegador

## Login pelo navegador — versão 0.2.0

A tela de provedores oferece login para OpenAI, Anthropic, Google e xAI. O aplicativo usa os clientes oficiais instalados no Mac; não coleta senha de conta nem lê/copía seus arquivos de tokens. Não há conversão de assinatura de chat em chave da API.

| Provedor | Login | Execução |
| --- | --- | --- |
| OpenAI | Codex App Server, OAuth ChatGPT no navegador | Codex exec, texto e imagem anexa, com verificação de autenticação ChatGPT |
| Anthropic | Claude Code oficial, `auth login --claudeai` | Claude Code oficial em modo de resposta, mensagens de texto/imagem por stdin; exige autenticação Claude.ai |
| Google | Gemini CLI oficial via ACP, autenticação `oauth-personal` | ACP com texto/imagem, configuração restrita e modelo padrão ou escolhido |
| xAI | Grok Build oficial, login OAuth | ACP com sessão autenticada. A versão instalada anuncia apenas texto; imagens são recusadas com orientação para usar outro perfil |

Abertura do navegador não é confirmação de login. A tela acompanha instalação, espera, conclusão, falha e cancelamento. O teste envia uma mensagem simples ao modelo antes de salvar, se solicitado. `default` usa o modelo padrão do cliente, sem fixar um catálogo desatualizado.

A sessão é compartilhada por provedor com seu cliente oficial neste Mac; perfis diferentes não isolam contas. O modo somente local recusa perfis com login na nuvem. As credenciais das APIs não são consultadas para esses perfis. Ferramentas locais dos agentes são restringidas; ações no Windows continuam passando pela validação do executor do AgentSmith. Cancelar encerra o grupo de processos da chamada, incluindo processos auxiliares do cliente.

Se faltar um cliente, o botão de instalação baixa seu pacote oficial pelo npm na pasta de dados do AgentSmith. Exige Node.js LTS. Instalar um componente não compra nem ativa um plano. A homologação de login, cotas, modelos e operação visual precisa da conclusão de autenticação pelo titular da conta.

Fontes: [Codex App Server](https://developers.openai.com/codex/app-server/), [Claude Code CLI](https://code.claude.com/docs/en/cli-reference), [condições de integração do Claude Code](https://code.claude.com/docs/en/legal-and-compliance), [Gemini CLI](https://geminicli.com/docs/get-started/authentication/), [Grok Build](https://docs.x.ai/build/enterprise).


## Correção 0.3.1: crash ao iniciar login

O comando nativo de login é síncrono e pode ser chamado na thread da interface, sem um contexto Tokio ativo. O agendamento agora usa `tauri::async_runtime::spawn`, mantendo o cancelamento pelo AbortHandle. Isso cobre login e instalação de componentes para os quatro provedores.

O teste de regressão executa fora de um runtime Tokio, inicia e cancela tarefas para os quatro provedores, verifica timers e rejeição de duplicatas. O agendamento anterior reproduziu `there is no reactor running`; a correção passou. Os relatórios de crash do macOS confirmavam SIGABRT em `AuthManager::start` → `tokio::task::spawn`.
