import {t,systemText} from './i18n';
import { useEffect, useState } from 'react';
import { CheckCircle2, ExternalLink, Loader2, RefreshCw } from 'lucide-react';
import type { Profile } from './types';
import { loginClients } from './auth';
type Status = {
    installed: boolean;
    client: string;
    phase: string;
    message: string;
};
type Call = <T>(n: string, a?: Record<string, unknown>) => Promise<T>;
export default function LoginConnection({ profile, call }: {
    profile: Profile;
    call: Call;
}) {
    const [status, setStatus] = useState<Status | null>(null), [error, setError] = useState(''), [testing, setTesting] = useState(false), [starting, setStarting] = useState(false), [result, setResult] = useState('');
    const active = starting || status?.phase === 'connecting' || status?.phase === 'installing';
    useEffect(() => { let stopped = false, polling = false; async function refresh() { if (polling)
        return; polling = true; try {
        const s = await call<Status>('browser_auth_status', { vendor: profile.vendor });
        if (!stopped)
            setStatus(s);
    }
    catch (e) {
        if (!stopped)
            setError(String(e));
    }
    finally {
        polling = false;
    } } void refresh(); const timer = setInterval(() => void refresh(), 1500); return () => { stopped = true; clearInterval(timer); }; }, [profile.vendor]);
    useEffect(() => setResult(''), [profile.model, profile.vision]);
    async function start(install: boolean) { setError(''); setResult(''); setStarting(true); try {
        setStatus(await call<Status>('browser_auth_start', { vendor: profile.vendor, install }));
    }
    catch (e) {
        setError(String(e));
    }
    finally {
        setStarting(false);
    } }
    async function cancel() { setError(''); try {
        await call('browser_auth_cancel', { vendor: profile.vendor });
        setStatus(await call<Status>('browser_auth_status', { vendor: profile.vendor }));
    }
    catch (e) {
        setError(String(e));
    } }
    async function test() { setError(''); setResult(''); setTesting(true); try {
        setResult(await call<string>('test_browser_profile', { profile }));
    }
    catch (e) {
        setError(String(e));
    }
    finally {
        setTesting(false);
    } }
    return <section className="login-connection" aria-label={t("Conexão pelo navegador")}><div className="login-title"><span className="summary-icon"><ExternalLink size={20}/></span><div><strong>{t("Entrar com ")}{profile.vendor === 'google' ? 'Google' : profile.vendor === 'openai' ? 'ChatGPT' : profile.vendor === 'anthropic' ? 'Claude' : 'xAI'}</strong><small>{t("Conexão oficial via ")}{loginClients[profile.vendor]}{t(" · Sem API key")}</small></div></div>
 <p>{t("O navegador abrirá a página oficial para você entrar na sua conta. O acesso aos modelos segue os limites do seu plano.")}</p>
 <p className="login-note">{t("A sessão pertence ao ")}{loginClients[profile.vendor]}{t(" neste Mac e é compartilhada entre os perfis deste provedor. Senhas e tokens ficam sob responsabilidade do programa oficial.")}</p>
 <div className={'login-status ' + (status?.phase === 'authenticated' ? 'success' : '')} role="status">{active && <Loader2 size={15} className="spin"/>}{(status?.message?systemText(status.message):undefined) ?? t("Verificando componente instalado…")}</div>
 {error && <div className="login-error" role="alert">{systemText(error)}</div>}{result && <div className="login-success" role="status"><CheckCircle2 size={16}/>{systemText(result)}</div>}
 <div className="button-row">{status && !status.installed ? <button type="button" className="button primary" disabled={active || testing} onClick={() => void start(true)}>{t("Instalar ")}{loginClients[profile.vendor]}</button> : <button type="button" className="button primary" disabled={!status || active || testing} onClick={() => void start(false)}><ExternalLink size={15}/>{t(" Entrar pelo navegador")}</button>}
 {active ? <button type="button" className="button secondary" onClick={() => void cancel()}>{t("Cancelar conexão")}</button> : <button type="button" className="button secondary" disabled={!status?.installed || testing} onClick={() => void test()}>{testing ? <Loader2 size={15} className="spin"/> : <RefreshCw size={15}/>} {testing ? t("Testando…") : t("Testar conexão")}</button>}</div>
 {!status?.installed && <small className="muted">{t("A instalação usa o pacote oficial e exige Node.js LTS. Nenhuma assinatura é contratada pelo AgentSmith.")}</small>}
 </section>;
}
