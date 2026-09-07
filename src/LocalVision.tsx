import {t,systemText} from './i18n';
import { useEffect, useState } from 'react';
import { Cpu, Download, Loader2, Power, ScanEye, Trash2 } from 'lucide-react';
import type { Settings } from './types';
import './local-vision.css';
type Model = {
    id: string;
    name: string;
    description: string;
    license: string;
    installed: boolean;
    downloadBytes: number;
};
type Status = {
    engineAvailable: boolean;
    models: Model[];
    download: {
        modelId: string;
        phase: string;
        received: number;
        total: number;
        message: string;
    };
    activeModel: string;
    working: boolean;
};
type Call = <T>(name: string, args?: Record<string, unknown>) => Promise<T>;
const size = (n: number) => n >= 1e9 ? `${(n / 1e9).toFixed(2)} GB` : `${Math.round(n / 1e6)} MB`;
export default function LocalVision({ call, native, onProfiles, running }: {
    call: Call;
    native: boolean;
    onProfiles: (s: Settings) => void;
    running: boolean;
}) {
    const [status, setStatus] = useState<Status | null>(null), [error, setError] = useState(''), [result, setResult] = useState(''), [action, setAction] = useState('');
    useEffect(() => { if (!native)
        return; let stopped = false, polling = false; const refresh = async () => { if (polling)
        return; polling = true; try {
        const s = await call<Status>('local_engine_status');
        if (!stopped)
            setStatus(s);
    }
    catch (e) {
        if (!stopped)
            setError(String(e));
    }
    finally {
        polling = false;
    } }; void refresh(); const timerId = setInterval(() => void refresh(), 1200); return () => { stopped = true; clearInterval(timerId); }; }, [native]);
    async function perform(label: string, fn: () => Promise<void>) { setAction(label); setError(''); setResult(''); try {
        await fn();
        setStatus(await call<Status>('local_engine_status'));
    }
    catch (e) {
        setError(String(e));
    }
    finally {
        setAction('');
    } }
    const downloading = status?.download.phase === 'downloading';
    return <section className="panel local-vision" aria-label={t("Visão local integrada")}>
  <div className="local-heading"><div className="local-title"><Cpu size={25}/><div><div className="eyebrow">{t("DENTRO DO AGENTSMITH")}</div><h2>{t("Visão local")}</h2></div></div><span className="badge">{t("Sem conta · Sem API key")}</span></div>
  <p>{t("Baixe um modelo e interprete imagens neste Mac. O motor já vem no app; não precisa instalar outro programa.")}</p>
  <div className="local-runtime"><span>{status?.working ? t("Modelo carregando ou respondendo…") : status?.activeModel ? t("Modelo na memória · Pronto para responder") : t("Motor em repouso · Memória liberada")}</span><button className="button secondary" disabled={!native || !!action || running || !status?.activeModel || status?.working} onClick={() => void perform('stop', async () => { await call('local_engine_stop'); setResult(t("Modelo descarregado da memória. O download continua salvo.")); })}><Power size={15}/>{t(" Liberar memória")}</button></div>
  {!native && <div className="info-strip">{t("Abra o aplicativo AgentSmith para baixar e executar modelos neste Mac.")}</div>}
  {status && !status.engineAvailable && <div className="alert">{t("Este aplicativo está sem o motor local. Abra a versão completa atualizada.")}</div>}
  <div className="local-models">{status?.models.map(m => {
            const current = downloading && status.download.modelId === m.id;
            return <article className="local-model" key={m.id}><div className="local-model-top"><h3>{t(m.name)}</h3><span className="badge">{m.installed ? t("Baixado") : size(m.downloadBytes)}</span></div><p>{t(m.description)}</p><small>{size(m.downloadBytes)}{t(" em disco · ")}{m.license}{t(" · Um modelo por vez")}</small>
    <div className="button-row">{!m.installed ? <button className="button primary" disabled={!!action || downloading || !status.engineAvailable} onClick={() => void perform(m.id, async () => { await call('local_model_download', { id: m.id }); })}><Download size={15}/>{t(" Baixar modelo")}</button> : <><button className="button primary" disabled={!!action || running || status.working} onClick={() => void perform('test-' + m.id, async () => setResult(await call<string>('local_vision_test', { id: m.id })))}>{action === 'test-' + m.id ? <Loader2 size={15} className="spin"/> : <ScanEye size={15}/>} {action === 'test-' + m.id ? t("Testando visão…") : t("Testar visão")}</button><button className="button secondary" disabled={!!action || running} onClick={() => void perform('profile-' + m.id, async () => { onProfiles(await call<Settings>('local_model_profile', { id: m.id })); setResult(t("Perfil disponível. Em Roteamento de IA, escolha este modelo em Operar e/ou Verificar. O roteamento atual foi preservado.")); })}>{t("Adicionar aos meus modelos")}</button><button className="icon-button" aria-label={t("Remover download de ") + m.name} disabled={!!action || running || status.working || downloading} onClick={() => void perform('remove-' + m.id, async () => { await call('local_model_remove', { id: m.id }); setResult(t("Download removido. Baixe novamente antes de usar este modelo.")); })}><Trash2 size={16}/></button></>}</div>
    {current && <div className="local-progress"><progress aria-label={t("Download de ") + m.name} max={status.download.total} value={status.download.received}/><div><span>{size(status.download.received)}{t(" de ")}{size(status.download.total)} · {Math.min(100, Math.floor(100 * status.download.received / status.download.total))}%</span><button className="button secondary" onClick={() => void perform('cancel', async () => { await call('local_download_cancel'); })}>{t("Cancelar download")}</button></div></div>}
   </article>;
        })}</div>
  {status?.download.message && !downloading && <p role="status" className={status.download.phase === 'error' ? 'login-error' : 'muted'}>{systemText(status.download.message)}</p>}
  {error && <div className="login-error" role="alert">{systemText(error)}</div>}{result && <div className="login-success" role="status">{systemText(result)}</div>}
  <p className="local-note">{t("A primeira resposta inclui carregar o modelo. Após 2 minutos sem uso, a memória é liberada automaticamente. As imagens não são gravadas pelo motor; os arquivos do modelo ficam salvos até você removê-los. Se houver outro provedor no roteamento, ele poderá receber imagens conforme a rota escolhida.")}</p>
 </section>;
}
