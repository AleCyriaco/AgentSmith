import {useEffect,useState} from 'react';
import {Smartphone,Copy} from 'lucide-react';
import {t} from './i18n';
type Status={active:boolean;baseUrl:string;devices:number;port:number};
type Pair={url:string;qr:string;expiresAt:number};
export default function PocketChannel({call}:{call:<T>(name:string,args?:Record<string,unknown>)=>Promise<T>}){
 const [status,setStatus]=useState<Status|null>(null),[base,setBase]=useState(''),[pair,setPair]=useState<Pair|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState('');
 useEffect(()=>{void call<Status>('pocket_status').then(s=>{setStatus(s);try{setBase(s.baseUrl||localStorage.getItem('agentsmith-pocket-url')||'')}catch{setBase(s.baseUrl)}}).catch(()=>{});},[]);
 async function run(fn:()=>Promise<unknown>){setBusy(true);setError('');try{await fn();setStatus(await call<Status>('pocket_status'));}catch(e){setError(String(e));}finally{setBusy(false)}}
 return <section className="panel pocket-channel"><div className="panel-head"><div><span className="tiny-label">AGENTSMITH POCKET</span><h3>{t('Seu operador no celular')}</h3></div><Smartphone size={22}/></div>
 <p>{t('Acompanhe tarefas, envie pedidos e aprove ações no iPhone ou Android pela sua rede Tailscale.')}</p>
 <label className="field"><span>{t('Endereço HTTPS deste Mac no Tailscale')}</span><input disabled={status?.active} value={base} onChange={e=>setBase(e.target.value)} placeholder="https://meu-mac.minha-rede.ts.net" autoComplete="off" spellCheck={false}/></label>
 <p className="info-strip">{t('Primeiro configure Tailscale Serve neste Mac para encaminhar HTTPS à porta local 17420. Depois ligue o Pocket e escaneie o QR. O app precisa permanecer aberto.')}</p>
 <details><summary>{t('Configurar acesso privado')}</summary><p>{t('No Terminal deste Mac, com o Tailscale conectado:')}</p><code>tailscale serve --bg http://127.0.0.1:17420</code><p>{t('Copie o endereço HTTPS mostrado acima. Use Serve, que mantém o acesso dentro da sua rede Tailscale.')}</p></details>
 <div className="button-row">{!status?.active?<button className="button primary" disabled={busy||!base.trim()} onClick={()=>void run(async()=>{await call('pocket_start',{baseUrl:base});try{localStorage.setItem('agentsmith-pocket-url',base)}catch{}setPair(await call<Pair>('pocket_pair'));})}>{t('Ligar Pocket')}</button>:<><button className="button primary" disabled={busy} onClick={()=>void run(async()=>setPair(await call<Pair>('pocket_pair')))}>{t('Parear outro aparelho')}</button><button className="button secondary" disabled={busy} onClick={()=>void run(async()=>{await call('pocket_stop');setPair(null);})}>{t('Desligar e revogar acessos')}</button></>}</div>
 {pair&&status?.active&&<div className="pocket-pair"><img width="220" height="220" alt={t('QR de pareamento')} src={'data:image/svg+xml;base64,'+btoa(pair.qr)}/><div><strong>{t('Escaneie com a câmera do celular')}</strong><p>{t('Pareamento de uso único, válido por 5 minutos. O acesso dura 12 horas e é revogado ao desligar o Pocket ou fechar o app.')}</p><button className="button secondary" onClick={()=>void navigator.clipboard.writeText(pair.url).catch(()=>setError(t('Não foi possível copiar.')))}><Copy size={14}/>{t('Copiar link de pareamento')}</button></div></div>}
 {error&&<p role="alert" className="login-error">{error}</p>}
 <p className="hint">{status?.active?t('Pocket ligado · acesso privado pelo Tailscale'):t('Pocket desligado')}</p>
 </section>
}
