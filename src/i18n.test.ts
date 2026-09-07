import {afterEach,describe,expect,it,vi} from 'vitest';
import messages from './locales/messages.json';
import {getLanguage,setLanguage,systemText,translate,type Language} from './i18n';
afterEach(()=>{setLanguage('pt-BR');vi.unstubAllGlobals()});
describe('interface languages',()=>{
 it('provides English and Spanish translations with matching placeholders',()=>{
  const fields=(value:string)=>[...value.matchAll(/\{(\w+)\}/g)].map(match=>match[1]).sort();
  for(const [source,translations] of Object.entries(messages))for(const lang of ['en','es'] as const){expect(translations[lang].trim(),source).not.toBe('');expect(fields(translations[lang]),source).toEqual(fields(source))}
 });
 it('preserves spaces and dynamic names while switching language',()=>{
  expect(translate(' Recolher barra lateral ','en')).toBe(' Collapse sidebar ');
  expect(translate('Adicionar {0}','es',{'0':'Servidor Carlos'})).toBe('Añadir Servidor Carlos');
  expect(translate('Somar na Calculadora','en')).toBe('Somar na Calculadora');
  expect(translate('Retomar','pt-BR')).toBe('Retomar');
 });
 it('translates native progress messages and preserves unknown diagnostics',()=>{
  setLanguage('en');expect(systemText('Conferindo a tela · etapa 2 de 10')).toBe('Checking screen · Step 2 of 10');
  setLanguage('es');expect(systemText('Período encerrado · 3 ciclos concluídos. Nenhuma nova ação será enviada.')).toBe('Período finalizado · 3 ciclos completados. No se enviarán nuevas acciones.');
  expect(systemText('ERRCONNECT_TLS_CONNECT_FAILED')).toBe('ERRCONNECT_TLS_CONNECT_FAILED');
 });
 it('saves supported languages and ignores invalid values',()=>{
  const setItem=vi.fn();vi.stubGlobal('localStorage',{setItem});setLanguage('es');
  expect(getLanguage()).toBe('es');expect(setItem).toHaveBeenCalledWith('agentsmith-language','es');
  setLanguage('xx' as Language);expect(getLanguage()).toBe('es');
 });
 it('remains usable when preference storage is unavailable',()=>{
  vi.stubGlobal('localStorage',{setItem(){throw new Error('Unavailable')}});expect(()=>setLanguage('en')).not.toThrow();expect(getLanguage()).toBe('en');
 });
});
