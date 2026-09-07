import {useSyncExternalStore} from 'react';
import messages from './locales/messages.json';
export type Language='pt-BR'|'en'|'es';
const storageKey='agentsmith-language';
export const languages:Language[]=['pt-BR','en','es'];
let language:Language='pt-BR';
try{const saved=localStorage.getItem(storageKey);if(languages.includes(saved as Language))language=saved as Language}catch{}
const listeners=new Set<()=>void>();
function publish(){if(typeof document!=='undefined')document.documentElement.lang=language;listeners.forEach(listener=>listener())}
export function setLanguage(value:Language){if(!languages.includes(value))return;language=value;try{localStorage.setItem(storageKey,value)}catch{}publish()}
export function getLanguage(){return language}
export function useLanguage(){return useSyncExternalStore(listener=>{listeners.add(listener);return()=>{listeners.delete(listener)}},getLanguage,getLanguage)}
if(typeof window!=='undefined'){document.documentElement.lang=language;window.addEventListener('storage',event=>{if(event.key===storageKey){language=languages.includes(event.newValue as Language)?event.newValue as Language:'pt-BR';publish()}})}
const catalog:Record<string,{en:string;es:string}>=messages;
export function translate(key:string,locale:Language,params:Record<string,string|number>={}):string{
 const trimmed=key.trim();
 const translated=locale==='pt-BR'?undefined:catalog[key]?.[locale]??catalog[trimmed]?.[locale];
 const text=translated===undefined?key:catalog[key]?translated:key.slice(0,key.indexOf(trimmed))+translated+key.slice(key.indexOf(trimmed)+trimmed.length);
 return text.replace(/\{(\w+)\}/g,(placeholder,id)=>Object.prototype.hasOwnProperty.call(params,id)?String(params[id]):placeholder);
}
export function t(key:string,params:Record<string,string|number>={}){return translate(key,language,params)}
// Translate known application messages without changing stored logs, task content or unknown diagnostics.
const escapeRegex=(text:string)=>text.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
const patterns=Object.keys(catalog).filter(key=>/\{\w+\}/.test(key)).map(key=>{
 const ids=[...key.matchAll(/\{(\w+)\}/g)].map(match=>match[1]);
 return {key,ids,regex:new RegExp('^'+key.split(/\{\w+\}/).map(escapeRegex).join('(.*?)')+'$')};
});
export function systemText(value:string):string{
 if(!value||language==='pt-BR')return value;
 if(catalog[value]||catalog[value.trim()])return t(value);
 for(const {key,ids,regex} of patterns){const match=regex.exec(value);if(match)return t(key,Object.fromEntries(ids.map((id,index)=>[id,match[index+1]])))}
 return value;
}
