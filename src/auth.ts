import type {Profile,Vendor} from './types';
export const loginClients:Record<string,string>={openai:'Codex',anthropic:'Claude Code',google:'Gemini CLI',xai:'Grok Build'};
export function withAuth(profile:Profile,vendor:Vendor,method:'api_key'|'browser'):Profile {
 return {...profile,authMethod:method,baseUrl:method==='browser'?`official://${vendor.id}`:vendor.baseUrl,model:method==='browser'?'default':'',vision:false};
}
