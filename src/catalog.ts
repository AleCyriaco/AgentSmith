import type {Vendor} from './types';
// Provider selection, not a market-share ranking. Model IDs are supplied by the account owner.
export const vendors:Vendor[] = [
 {id:'openai',name:'OpenAI',country:'EUA',protocol:'responses',baseUrl:'https://api.openai.com/v1',family:'GPT',docs:'https://developers.openai.com/api/docs/guides/images-vision'},
 {id:'anthropic',name:'Anthropic',country:'EUA',protocol:'anthropic',baseUrl:'https://api.anthropic.com/v1',family:'Claude',docs:'https://platform.claude.com/docs/en/api/overview'},
 {id:'google',name:'Google',country:'EUA',protocol:'chat',baseUrl:'https://generativelanguage.googleapis.com/v1beta/openai',family:'Gemini',docs:'https://ai.google.dev/gemini-api/docs/openai'},
 {id:'xai',name:'xAI',country:'EUA',protocol:'responses',baseUrl:'https://api.x.ai/v1',family:'Grok',docs:'https://docs.x.ai/developers/model-capabilities/text/generate-text'},
 {id:'amazon',name:'Amazon Bedrock',country:'EUA',protocol:'bedrock',baseUrl:'https://bedrock-runtime.us-east-1.amazonaws.com',family:'Nova / modelos Bedrock • chave Bedrock',docs:'https://docs.aws.amazon.com/bedrock/latest/userguide/api-keys.html'},
 {id:'alibaba',name:'Alibaba Cloud',country:'China',protocol:'chat',baseUrl:'https://dashscope-intl.aliyuncs.com/compatible-mode/v1',family:'Qwen',docs:'https://www.alibabacloud.com/help/en/model-studio/compatibility-of-openai-with-dashscope'},
 {id:'deepseek',name:'DeepSeek',country:'China',protocol:'chat',baseUrl:'https://api.deepseek.com',family:'DeepSeek',docs:'https://api-docs.deepseek.com/'},
 {id:'moonshot',name:'Moonshot AI',country:'China',protocol:'chat',baseUrl:'https://api.moonshot.ai/v1',family:'Kimi',docs:'https://platform.kimi.ai/docs/overview'},
 {id:'zai',name:'Z.ai / Zhipu',country:'China',protocol:'chat',baseUrl:'https://api.z.ai/api/paas/v4',family:'GLM',docs:'https://docs.z.ai/guides/overview/quick-start'},
 {id:'bytedance',name:'ByteDance / BytePlus',country:'China',protocol:'chat',baseUrl:'https://ark.ap-southeast.bytepluses.com/api/v3',family:'Seed / Doubao • ID do modelo ou endpoint',docs:'https://docs.byteplus.com/en/docs/modelark/1099455'},
 {id:'ollama',name:'Ollama',country:'Local',protocol:'chat',baseUrl:'http://127.0.0.1:11434/v1',family:'Modelo instalado no Ollama',docs:'https://docs.ollama.com/api/openai-compatibility',local:true},
 {id:'lmstudio',name:'LM Studio',country:'Local',protocol:'chat',baseUrl:'http://127.0.0.1:1234/v1',family:'Modelo carregado no LM Studio',docs:'https://lmstudio.ai/docs/developer/openai-compat',local:true},
 {id:'local',name:'Servidor local compatível',country:'Local',protocol:'chat',baseUrl:'http://127.0.0.1:8080/v1',family:'llama.cpp / servidor MLX compatível',docs:'https://github.com/ggml-org/llama.cpp/tree/master/tools/server',local:true}
];
