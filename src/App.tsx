import {t,systemText,useLanguage} from './i18n';
import LanguagePicker from './LanguagePicker';
import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { Activity, ArrowDown, ArrowRight, BookOpen, Check, CheckCircle2, ChevronDown, Command, Computer, Copy, Cpu, ExternalLink, Globe, KeyRound, Layers3, Loader2, LockKeyhole, Monitor, MousePointer2, Network, PanelLeftClose, PanelLeftOpen, Pause, Play, Plus, Power, Route, Repeat, RotateCcw, Settings2, ShieldCheck, Sparkles, Square, Terminal, Trash2, X } from 'lucide-react';
import { vendors } from './catalog';
import { emptySettings, defaultDisplay, defaultPerformance, type PerformanceSettings, type DisplaySettings, type Settings, type Machine, type Profile, type Run, type Snapshot, type SessionInfo, type Vendor } from './types';
import SessionPanel from './SessionPanel';
import LoginConnection from './LoginConnection';
import LocalVision from './LocalVision';
import PlanEditor from './PlanEditor';
import TextCheckForm from './TextCheckForm';
import TaskActivity from './TaskActivity';
import RepeatForm from './RepeatForm';
import { stopOnEscape } from './stopShortcut';
import { loginClients, withAuth } from './auth';
const pages = [{ id: 'operate', label: 'Central de operação', icon: Monitor }, { id: 'machines', label: 'Máquinas', icon: Computer }, { id: 'models', label: 'Provedores e modelos', icon: Cpu }, { id: 'routes', label: 'Roteamento de IA', icon: Route }, { id: 'architecture', label: 'Como funciona', icon: Layers3 }];
const statuses: Record<string, string> = { ready: 'Pronta para revisar', running: 'Em execução', verifying: 'Verificando', waiting: 'Aguardando ciclo', expired: 'Período encerrado', paused: 'Pausada', blocked: 'Precisa de atenção', completed: 'Concluída', cancelled: 'Parada', connected: 'Conectado', connecting: 'Conectando', disconnected: 'Desconectado', error: 'Falha de conexão' };
const protocolNames: Record<string, string> = { rdp: 'Microsoft RDP', rustdesk: 'RustDesk', nanokvm: 'NanoKVM', nanokvm_pro: 'NanoKVM Pro', nanokvm_usb: 'NanoKVM-USB' };
function Brand({ small = false }: {
    small?: boolean;
}) { return <div className={'brand ' + (small ? 'small' : '')}><div className="brand-mark"><span /><span /><span /></div>{!small && <span>Agent<span className="brand-light">Smith</span></span>}</div>; }
function Modal({ title, sub, children, close }: {
    title: string;
    sub?: string;
    children: React.ReactNode;
    close: () => void;
}) { return <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget)
    close(); }}><section className="modal"><button className="icon-button close" onClick={close} aria-label={t("Fechar")}><X size={18}/></button><div className="eyebrow">{t("AGENTSMITH / CONFIGURAÇÃO")}</div><h2>{title}</h2>{sub && <p className="muted">{sub}</p>}{children}</section></div>; }
function Label({ title, children, hint }: {
    title: string;
    children: React.ReactNode;
    hint?: string;
}) { return <label className="field"><span>{title}</span>{children}{hint && <small>{hint}</small>}</label>; }
export default function App() {
    useLanguage();
    const [page, setPage] = useState('operate'), [settings, setSettings] = useState<Settings>(emptySettings), [runs, setRuns] = useState<Run[]>([]), [session, setSession] = useState<SessionInfo>({ machineId: '', status: '', message: '' }), [frame, setFrame] = useState<Snapshot | null>(null);
    const [selectedMachine, setSelectedMachine] = useState(''), [selectedRun, setSelectedRun] = useState<string | null>(''), [instructions, setInstructions] = useState(''), [notice, setNotice] = useState(''), [error, setError] = useState(''), [busy, setBusy] = useState(false), [manual, setManual] = useState(false), [focusRdp, setFocusRdp] = useState(false), [windowBusy, setWindowBusy] = useState(false);
    const [sidebarCollapsed, setSidebarCollapsed] = useState(() => { try {
        return localStorage.getItem('agentsmith-sidebar-collapsed') === 'true';
    }
    catch {
        return false;
    } });
    useEffect(() => { try {
        localStorage.setItem('agentsmith-sidebar-collapsed', String(sidebarCollapsed));
    }
    catch { } }, [sidebarCollapsed]);
    const [testingOperator,setTestingOperator]=useState('');
    const [removeProfile, setRemoveProfile] = useState<Profile | null>(null);
    const [removingProfile, setRemovingProfile] = useState(false);
    const [removeProfileError, setRemoveProfileError] = useState('');
    const [planModal,setPlanModal]=useState<{run:Run;mode:"edit"|"delete"}|null>(null);
    const [textStep, setTextStep] = useState<{run:Run;index:number}|null>(null);
    const [topCollapsed,setTopCollapsed]=useState(()=>{try{return localStorage.getItem("agentsmith-top-collapsed")==="true"}catch{return false}});
    useEffect(()=>{try{localStorage.setItem("agentsmith-top-collapsed",String(topCollapsed))}catch{}},[topCollapsed]);
    useEffect(()=>{const sync=(event:StorageEvent)=>{if(event.key==="agentsmith-top-collapsed")setTopCollapsed(event.newValue==="true")};window.addEventListener("storage",sync);return()=>window.removeEventListener("storage",sync)},[]);
    const [performanceModal, setPerformanceModal] = useState(false);
    const [repeatRun, setRepeatRun] = useState<Run | null>(null);
    const restartPending = useRef(false);
    const [restartingId, setRestartingId] = useState('');
    const stopPending = useRef(false);
    const [stoppingId, setStoppingId] = useState('');
    const connectionPending = useRef(false);
    const [connectingSaved, setConnectingSaved] = useState(false);
    const lastFrame = useRef(0);
    const performance = settings.performance ?? defaultPerformance;
    const [displayModal, setDisplayModal] = useState<Machine | null>(null);
    const [modelModal, setModelModal] = useState<{
        vendor: Vendor;
        profile?: Profile;
    } | null>(null), [machineModal, setMachineModal] = useState<Machine | 'new' | null>(null), [connectModal, setConnectModal] = useState<Machine | null>(null);
    const detachedView = new URLSearchParams(window.location.search).get('view') === 'rdp';
    const observedSession = useRef('');
    const native = isTauri();
    const polling = useRef(false);
    const active = selectedRun === null ? undefined : runs.find(r => r.id === selectedRun) ?? runs[0];
    const running = runs.some(r => ['running', 'verifying', 'waiting'].includes(r.status));
    const machine = settings.machines.find(m => m.id === (session.machineId || selectedMachine));
    async function call<T>(name: string, args?: Record<string, unknown>): Promise<T> { if (!native)
        throw new Error(t("Esta é a visualização da interface. Abra o aplicativo AgentSmith para usar os recursos nativos.")); return invoke<T>(name, args); }
    async function attempt(fn: () => Promise<unknown>) { setError(''); try {
        await fn();
    }
    catch (e) {
        setError(String(e).replace(/^Error: /, ''));
    } }
    async function refresh() { const [s, r] = await Promise.all([call<Settings>('load_settings'), call<Run[]>('list_runs')]); setSettings(s); setRuns(r); setSelectedMachine(old => old || s.machines[0]?.id || ''); }
    async function persist(s: Settings) { await call('save_settings', { settings: s }); setSettings(s); }
    useEffect(() => { if (native)
        void attempt(refresh); }, []);
    useEffect(() => { if (!native)
        return; lastFrame.current = 0; const timer = setInterval(async () => { if (polling.current)
        return; polling.current = true; try {
        const [i, r] = await Promise.all([call<SessionInfo>('session_info'), call<Run[]>('list_runs')]);
        setSession(i);
        setRuns(r);
        if (i.machineId && observedSession.current !== i.machineId) {
            observedSession.current = i.machineId;
            setSelectedMachine(i.machineId);
        }
        if (i.status === 'connected' && (detachedView || (!i.detached && page === 'operate'))) {
            try {
                const next = await call<Snapshot | null>('snapshot_if_new', { sequence: lastFrame.current });
                if (next) {
                    lastFrame.current = next.sequence;
                    setFrame(next);
                }
            }
            catch {
                lastFrame.current = 0;
                setFrame(null);
            }
        }
        else {
            lastFrame.current = 0;
            setFrame(null);
        }
    }
    catch { }
    finally {
        polling.current = false;
    } }, performance.captureIntervalMs); return () => clearInterval(timer); }, [page, detachedView, performance.captureIntervalMs]);
    useEffect(() => { if (!native)
        return; const sync = () => void attempt(refresh); const storage = (e: StorageEvent) => { if (e.key === 'agentsmith-settings-updated')
        sync(); if (e.key === 'agentsmith-navigation' && e.newValue?.startsWith('routes:'))
        setPage('routes'); }; window.addEventListener('focus', sync); window.addEventListener('storage', storage); return () => { window.removeEventListener('focus', sync); window.removeEventListener('storage', storage); }; }, []);
    useEffect(() => { if (!notice)
        return; const timerId = setTimeout(() => setNotice(''), 6500); return () => clearTimeout(timerId); }, [notice]);
    async function plan() { if (!selectedMachine)
        throw new Error(t("Cadastre e selecione uma máquina.")); setBusy(true); try {
        const run = await call<Run>('plan_run', { machineId: selectedMachine, instructions });
        setRuns(r => [run, ...r]);
        setSelectedRun(run.id);
        setNotice(t("Roteiro criado. Confira as etapas e inicie quando estiver pronto."));
    }
    finally {
        setBusy(false);
    } }
    async function pause() { await call('pause_run'); setManual(true); setNotice(t("Automação pausada. O controle manual será liberado assim que a ação em andamento terminar.")); }
    async function restart(id: string) { if (restartPending.current)
        return; restartPending.current = true; setRestartingId(id); try {
        const run = await call<Run>('restart_run', { id });
        setSelectedRun(run.id);
        setManual(false);
        setRuns(old => [run, ...old.filter(r => r.id !== run.id)]);
        setNotice(t("Roteiro reiniciado desde a primeira etapa. A execução anterior permanece no histórico."));
        await refresh();
    }
    finally {
        restartPending.current = false;
        setRestartingId('');
    } }
    async function stop(id: string) { if (stopPending.current)
        return; stopPending.current = true; setStoppingId(id); try {
        await call('stop_run', { id });
        setManual(true);
        setNotice(t("Parada solicitada. As ações já enviadas ao Windows são preservadas."));
        await refresh();
    }
    catch (e) {
        setStoppingId('');
        throw e;
    }
    finally {
        stopPending.current = false;
    } }
    useEffect(() => { if (stoppingId && runs.some(r => r.id === stoppingId && ['cancelled', 'completed'].includes(r.status)))
        setStoppingId(''); }, [runs, stoppingId]);
    useEffect(() => {
        const escape = (event: KeyboardEvent) => {
            const executing = runs.find(r => ['running', 'verifying', 'waiting'].includes(r.status));
            stopOnEscape(event, !!executing, () => { if (executing && !stoppingId)
                void attempt(() => stop(executing.id)); });
        };
        window.addEventListener('keydown', escape, true);
        return () => window.removeEventListener('keydown', escape, true);
    }, [runs, stoppingId]);
    async function send(action: object) { await call('manual_action', { action }); }
    async function saveModel(profile: Profile, secret: string) { const profiles = [...settings.profiles.filter(p => p.id !== profile.id), profile]; const routes = { ...settings.routes }; if (!routes.planner.length)
        routes.planner = [profile.id]; for (const role of ['operator', 'verifier'] as const)
        if (!routes[role].length)
            routes[role] = [profile.id]; await persist({ ...settings, profiles, routes }); if (secret)
        await call('save_credential', { id: profile.id, binding: profile.baseUrl, secret }); setModelModal(null); setNotice(t("Perfil salvo. Use “Testar” para conferir a conexão com o modelo.")); }
    async function saveMachine(m: Machine, password: string) { await persist({ ...settings, machines: [...settings.machines.filter(x => x.id !== m.id), m] }); if (password)
        await call('save_credential', { id: m.id, binding: `rdp://${m.host}:${m.port}|${m.domain}|${m.username}`, secret: password }); setSelectedMachine(m.id); setMachineModal(null); setNotice(t("Máquina cadastrada.")); }
    async function beginConnection(m: Machine | undefined) {
        if (!m || connectionPending.current)
            return;
        connectionPending.current = true;
        setConnectingSaved(true);
        setError('');
        try {
            const started = await call<boolean>('connect_saved_machine', { id: m.id });
            if (!started) {
                setConnectModal(m);
                return;
            }
            setSelectedMachine(m.id);
            setConnectModal(null);
            setPage('operate');
            setManual(true);
            setNotice(t("Conectando com a senha salva no Chaves do macOS."));
        }
        catch (e) {
            setError(String(e));
        }
        finally {
            connectionPending.current = false;
            setConnectingSaved(false);
        }
    }
    async function connectWithPassword(password: string, remember: boolean) {
        if (!connectModal)
            return;
        const m = connectModal;
        if (remember && password)
            await call('save_credential', { id: m.id, binding: `rdp://${m.host}:${m.port}|${m.domain}|${m.username}`, secret: password });
        await call('connect_machine', { id: m.id, password: password || null });
        setSelectedMachine(m.id);
        setConnectModal(null);
        setPage('operate');
        setManual(true);
    }
    const profileName = (id: string) => settings.profiles.find(p => p.id === id)?.name ?? t("Não configurado");
    async function detach() { setWindowBusy(true); try {
        await call('detach_rdp');
        setSession(i => ({ ...i, detached: true }));
        setFocusRdp(false);
    }
    finally {
        setWindowBusy(false);
    } }
    async function attach() { await call('attach_rdp'); setSession(i => ({ ...i, detached: false })); }
    async function saveDisplay(m: Machine, display: DisplaySettings) {
        const latest = await call<Settings>('load_settings');
        const saved = latest.machines.find(x => x.id === m.id);
        if (!saved)
            throw new Error(t("Máquina não encontrada."));
        const updated = { ...saved, display };
        await persist({ ...latest, machines: latest.machines.map(x => x.id === m.id ? updated : x) });
        setDisplayModal(null);
        await beginConnection(updated);
    }
    const performanceForm = performanceModal && <PerformanceForm value={performance} close={() => setPerformanceModal(false)} save={async (value) => { await call('save_performance', { performance: value }); await refresh(); try {
        localStorage.setItem('agentsmith-settings-updated', String(Date.now()));
    }
    catch { } setPerformanceModal(false); setNotice(t("Ritmo atualizado. O tempo do modelo aparecerá no histórico.")); }}/>;
    const displayForm = displayModal && <DisplayForm machine={displayModal} close={() => setDisplayModal(null)} save={d => saveDisplay(displayModal, d)}/>;
    const progressRun = runs.find(r => ['running', 'verifying', 'waiting'].includes(r.status)) ?? active;
    const taskActivity = <TaskActivity run={progressRun} planning={busy && !running} connected={session.status === 'connected' && session.machineId === progressRun?.machineId} onResume={() => void attempt(async () => { if (!progressRun)
        return; await call('start_run', { id: progressRun.id }); setManual(false); await refresh(); })} onPause={() => void attempt(pause)} onStop={() => progressRun && void attempt(() => stop(progressRun.id))} stopping={stoppingId === progressRun?.id} onRestart={() => progressRun && void attempt(() => restart(progressRun.id))} restarting={!!restartingId} onRepeat={() => progressRun && setRepeatRun(progressRun)} onRoutes={() => void attempt(async () => { localStorage.setItem('agentsmith-navigation', 'routes:' + Date.now()); setPage('routes'); if (detachedView)
        await attach(); })}/>;
    const repeatForm = repeatRun && <RepeatForm run={repeatRun} connected={!running && session.status === 'connected' && session.machineId === repeatRun.machineId} close={() => setRepeatRun(null)} start={async (options) => { const run = await call<Run>('repeat_run', { id: repeatRun.id, options }); setSelectedRun(run.id); setRuns(old => [run, ...old.filter(r => r.id !== run.id)]); setManual(false); setRepeatRun(null); await refresh(); }}/>;
    const alerts = <>{error && <div className="alert"><span>{systemText(error)}</span><button onClick={() => setError('')} aria-label={t("Dispensar")}><X size={16}/></button></div>}{notice && <div className="toast"><CheckCircle2 size={17}/>{systemText(notice)}</div>}</>;
    const sessionPanel = <SessionPanel topCollapsed={topCollapsed} toggleTop={()=>setTopCollapsed(v=>!v)} stop={()=>progressRun && void attempt(()=>stop(progressRun.id))} stopping={!!stoppingId} activity={busy ? t("Preparando roteiro…") : progressRun ? t(statuses[progressRun.status])+" · "+progressRun.title : ""} machines={settings.machines} machine={machine} selectedMachine={selectedMachine} session={session} frame={frame} manual={manual} running={running} detachedView={detachedView} focused={focusRdp} windowBusy={windowBusy} connecting={connectingSaved} performance={() => setPerformanceModal(true)} display={() => setDisplayModal(machine ?? null)} select={setSelectedMachine} connect={() => void beginConnection(settings.machines.find(m => m.id === selectedMachine) ?? settings.machines[0])} add={() => setMachineModal('new')} assume={() => void attempt(pause)} disconnect={() => void attempt(() => call('disconnect_machine'))} act={async (action) => { setError(''); try {
        await send(action);
        return true;
    }
    catch (e) {
        setError(String(e));
        return false;
    } }} detach={() => void attempt(detach)} attach={() => void attempt(attach)} focus={() => detachedView ? void attempt(async () => setFocusRdp(await call<boolean>('toggle_rdp_fullscreen'))) : setFocusRdp(v => !v)}/>;
    if (detachedView)
        return <div className={"detached-workspace "+(topCollapsed?"top-collapsed":"")}><div className="detached-language"><LanguagePicker/></div>{taskActivity}{alerts}{repeatForm}{sessionPanel}{displayForm}{performanceForm}{connectModal && <ConnectForm machine={connectModal} close={() => setConnectModal(null)} connect={connectWithPassword}/>}</div>;
    return <div className={'app-shell ' + (sidebarCollapsed ? 'sidebar-collapsed ' : '') + (page === 'operate' ? 'operation-shell '+(topCollapsed?'top-collapsed ':'') : '') + (focusRdp && page === 'operate' ? 'rdp-focus' : '')}><aside id="workspace-sidebar" className="sidebar" aria-label={t("Navegação principal")}><Brand /><a className="brand-credit" href="https://prodigy-lab.com" target="_blank" rel="noopener noreferrer" title={t('Abrir site da prodigy-lab')} onClick={event=>{if(native){event.preventDefault();void attempt(()=>call('open_prodigy_site'))}}}>by prodigy-lab <ExternalLink size={10}/></a><div className="workspace-label"><span className="workspace-dot"/>{t(" Workspace pessoal ")}<ChevronDown size={13}/></div><div className="nav-caption">{t("OPERAÇÃO REMOTA COM IA")}</div><nav>{pages.map(p => <button key={p.id} className={page === p.id ? 'active' : ''} onClick={() => setPage(p.id)}><p.icon size={18}/>{t(p.label)}{p.id === 'machines' && <span className="nav-count">{settings.machines.length}</span>}</button>)}</nav><div className="sidebar-bottom"><div className="local-indicator"><span className="status-dot"/>{settings.localOnly ? t("IA somente neste Mac") : t("Execução neste Mac")}<LockKeyhole size={13}/></div><p>{t("Você define o objetivo.")}<br />{t("Smith cuida dos próximos passos.")}</p><div className="version"><Command size={13}/> macOS <span>{t("v0.12.6 · Prévia")}</span></div></div></aside>
 <div className="main-shell"><header><div className="header-navigation"><button type="button" className="icon-button sidebar-toggle" onClick={() => setSidebarCollapsed(value => !value)} aria-controls="workspace-sidebar" aria-expanded={!sidebarCollapsed} aria-label={sidebarCollapsed ? t("Expandir barra lateral") : t("Recolher barra lateral")} title={sidebarCollapsed ? t("Expandir barra lateral") : t("Recolher barra lateral")}>{sidebarCollapsed ? <PanelLeftOpen size={19}/> : <PanelLeftClose size={19}/>}</button><div className="breadcrumbs">{t("Workspace ")}<span>/</span> <strong>{t(pages.find(p => p.id === page)?.label??'')}</strong></div></div><div className="header-right"><LanguagePicker/><span className={'badge ' + (session.status === 'connected' ? 'green' : '')}>{session.status === 'connected' ? <><span className="status-dot"/>{t(" Sessão ativa")}</> : <><span className="neutral-dot"/>{t(" Nenhuma sessão ativa")}</>}</span><div className="avatar">AS</div></div></header>
 {taskActivity}{repeatForm}{planModal && <PlanEditor run={planModal.run} mode={planModal.mode} close={()=>setPlanModal(null)} save={async edit=>{const saved=await call<Run>("edit_plan",{id:planModal.run.id,edit});setSelectedRun(saved.id);await refresh();setPlanModal(null)}} remove={async()=>{await call("delete_plan",{id:planModal.run.id,updatedAt:planModal.run.updatedAt});setSelectedRun(null);setRuns(old=>old.filter(r=>r.id!==planModal.run.id));setPlanModal(null);setNotice(t("Execução excluída. Outras execuções do mesmo plano permanecem no histórico."));void attempt(refresh)}}/>}{textStep && <TextCheckForm run={textStep.run} index={textStep.index} call={call} close={()=>setTextStep(null)} saved={async()=>{await refresh();setTextStep(null)}}/>}<main>{!native && <div className="banner">{t("Visualização da interface · Os recursos de conexão e IA funcionam no aplicativo para macOS.")}</div>}{error && <div className="alert"><span>{systemText(error)}</span><button onClick={() => setError('')} aria-label={t("Dispensar")}><X size={16}/></button></div>}{notice && <div className="toast"><CheckCircle2 size={17}/>{systemText(notice)}</div>}
 {page === 'operate' && <div className="operation-workspace"><div className="page-heading operation-heading"><div><div className="eyebrow">{t("SEU OPERADOR DIGITAL")}</div><h1>{t("Vamos colocar o trabalho em movimento.")}</h1><p>{t("Conecte um Windows, entregue um roteiro e acompanhe cada etapa.")}</p></div><button className="button secondary" onClick={() => setPage('models')}><Settings2 size={16}/>{t(" Configurar IA")}</button></div>
 <div className="summary-grid"><div className="summary"><span className="summary-icon"><Computer size={19}/></span><div><strong>{settings.machines.length.toString().padStart(2, '0')}</strong><span>{t("máquinas cadastradas")}</span></div><span className="summary-foot">{t("RDP + conectores futuros")}</span></div><div className="summary"><span className="summary-icon"><Cpu size={19}/></span><div><strong>{settings.profiles.length.toString().padStart(2, '0')}</strong><span>{t("perfis de modelo")}</span></div><span className="summary-foot">{t("Nuvem, local ou híbrido")}</span></div><div className="summary"><span className="summary-icon"><CheckCircle2 size={19}/></span><div><strong>{runs.filter(r => r.status === 'completed').length.toString().padStart(2, '0')}</strong><span>{t("tarefas concluídas")}</span></div><span className="summary-foot">{t("Com verificação por etapa")}</span></div></div>
 <div className="rdp-stage">{sessionPanel}</div><div className="operation-support">
 <section className="panel task-panel"><div className="panel-head"><div><span className="tiny-label">{t("PRÓXIMA MISSÃO")}</span><h3>{t("O que vamos fazer?")}</h3></div><Sparkles size={19} className="green-text"/></div><div className="task-compose"><textarea value={instructions} onChange={e => setInstructions(e.target.value)} placeholder={t("Descreva o objetivo ou cole seu passo a passo.\n\nEx.: abra o sistema de atendimento, localize o chamado 4582 e altere o responsável para Carlos. Confirme que foi salvo.")}/><div className="compose-foot"><span><Cpu size={13}/>{profileName(settings.routes.planner[0])}</span><span>{instructions.length}{t(" caracteres")}</span></div><button className="button primary full" disabled={busy || !instructions.trim()} onClick={() => void attempt(plan)}>{busy ? <Loader2 className="spin" size={16}/> : <Sparkles size={16}/>} {busy ? t("Preparando roteiro…") : t("Preparar roteiro")} <ArrowRight size={15}/></button><small>{t("Smith prepara as etapas. Você inicia a execução.")}</small></div><div className="task-detail">{active ? <><div className="task-title"><h4>{active.title}</h4><span className={'badge ' + (active.status === 'completed' ? 'green' : '')}>{t(statuses[active.status])}</span></div><div className="steps">{active.steps.map((step, i) => <div className={'step ' + step.status} key={i}><span className="step-number">{step.status === 'done' ? <Check size={13}/> : i + 1}</span><div><strong>{step.title}</strong><p>{step.evidence || step.success}</p><button className="ocr-step-button" disabled={running || session.status !== "connected" || session.machineId !== active.machineId} onClick={()=>setTextStep({run:active,index:i})}>{step.textCheck ? t("Editar verificação OCR") : t("Texto esperado (OCR)")}</button></div></div>)}</div><div className="task-actions"><button className="button secondary" disabled={running || busy || !!stoppingId || !!restartingId} onClick={()=>setPlanModal({run:active,mode:"edit"})}><Settings2 size={15}/>{t("Editar plano")}</button><button className="button stop-button" disabled={running || busy || !!stoppingId || !!restartingId} onClick={()=>setPlanModal({run:active,mode:"delete"})}><Trash2 size={15}/>{t("Excluir plano")}</button><button className="button primary" disabled={running || session.status !== 'connected' || ['completed', 'cancelled', 'expired'].includes(active.status)} onClick={() => void attempt(async () => { setManual(false); await call('start_run', { id: active.id }); await refresh(); })}><Play size={15}/>{active.status === 'ready' ? t("Executar") : t("Retomar")}</button><button className="button secondary" disabled={!running} onClick={() => void attempt(pause)}><Pause size={15}/>{t(" Pausar")}</button><button className="button stop-button" disabled={!!stoppingId || ['completed', 'cancelled', 'expired'].includes(active.status)} onClick={() => void attempt(() => stop(active.id))} title={t("Encerrar esta tarefa. Esc durante a execução.")}><Square size={15}/>{stoppingId === active.id ? t("Parando…") : t("Parar")}</button><button className="button secondary" disabled={running || !!stoppingId || !!restartingId || session.status !== 'connected' || session.machineId !== active.machineId} onClick={() => void attempt(() => restart(active.id))} title={running ? t("Pause ou pare a execução antes de reiniciar.") : t("Executar o mesmo roteiro desde a primeira etapa, mantendo o histórico anterior.")}><RotateCcw size={15} className={restartingId === active.id ? 'spin' : ''}/>{restartingId === active.id ? t("Reiniciando…") : t("Reiniciar")}</button><button className="button secondary" disabled={running || !!stoppingId || !!restartingId} onClick={() => setRepeatRun(active)}><Repeat size={15}/>{t(" Repetir")}</button><span>{active.actionCount}/{settings.maxActions}{t(" ações")}{active.repetition ? t(" no ciclo") : ''}</span></div></> : <div className="empty-task"><Route size={24}/><p>{t("Do objetivo ao resultado,")}<br />{t("um passo verificado de cada vez.")}</p><div className="mini-flow"><span>{t("Observar")}</span><ArrowRight size={12}/><span>{t("Agir")}</span><ArrowRight size={12}/><span>{t("Verificar")}</span></div></div>}</div></section>
 <section className="panel history-panel"><div className="panel-head"><h3>{t("Histórico de trabalho ")}<span className="count">{runs.length}</span></h3><span className="muted">{t("Progresso salvo neste Mac")}</span></div>{runs.length ? <div className="history-content"><div className="run-list">{runs.map(r => <button className={active?.id === r.id ? 'selected' : ''} key={r.id} onClick={() => setSelectedRun(r.id)}><Activity size={15}/><span>{r.title}<small className="run-date">{t("Atualizado em {0}", {"0":new Date(r.updatedAt).toLocaleString()})} · {r.id.slice(0,8)}</small></span><small>{t(statuses[r.status])}</small></button>)}</div><div className="run-log">{!active && <p className="muted">{t("Selecione uma execução no histórico para ver seus detalhes.")}</p>}{active?.log.slice(-8).map((line, i) => <div key={i}><span className="log-dot"/>{line}</div>)}</div></div> : <div className="history-empty"><Activity size={17}/><span>{t("As ações e verificações das suas tarefas vão aparecer aqui.")}</span></div>}</section></div></div>}
 {page === 'machines' && <><div className="page-heading"><div><div className="eyebrow">{t("AMBIENTES DE TRABALHO")}</div><h1>{t("Suas máquinas, em um só lugar.")}</h1><p>{t("Cadastre o destino e escolha a forma de acesso.")}</p></div><button className="button primary" onClick={() => setMachineModal('new')}><Plus size={16}/>{t(" Adicionar máquina")}</button></div><div className="card-grid">{settings.machines.map(m => <div className="panel machine-card" key={m.id}><div className="card-top"><span className="large-icon"><Computer size={24}/></span><span className="badge">{t(protocolNames[m.protocol])}</span></div><h3>{m.name}</h3><p>{m.host}:{m.port}</p><div className="machine-meta"><span>{m.username || t("Usuário a configurar")}</span><span>{m.protocol === 'rdp' ? t("Conector nativo") : t("Em desenvolvimento")}</span></div><div className="button-row"><button className="button primary" disabled={m.protocol !== 'rdp' || connectingSaved || running} onClick={() => void beginConnection(m)}><Power size={14}/>{t(" Conectar")}</button><button className="button secondary" onClick={() => setMachineModal(m)}>{t("Editar")}</button></div></div>)}<button className="add-card" onClick={() => setMachineModal('new')}><Plus size={26}/><strong>{t("Adicionar um Windows")}</strong><span>{t("Endereço, credenciais e conexão")}</span></button></div><div className="info-strip"><Network size={20}/><div><strong>{t("Primeiro conector: Microsoft RDP")}</strong><p>{t("RustDesk, NanoKVM, NanoKVM Pro e NanoKVM-USB têm lugar na arquitetura. Seus conectores ainda serão implementados.")}</p></div></div></>}
 {page === 'models' && <><div className="page-heading"><div><div className="eyebrow">{t("INTELIGÊNCIA À SUA ESCOLHA")}</div><h1>{t("Um Smith. Diferentes inteligências.")}</h1><p>{t("Use um ou vários provedores, com modelos diferentes para cada função.")}</p></div><span className="badge large"><Globe size={15}/>{t(" 10 provedores + modelos locais")}</span></div>{settings.profiles.length > 0 && <section className="panel configured"><div className="panel-head"><h3>{t("Seus perfis de modelo")}</h3><span className="muted">{t("Chaves guardadas no macOS")}</span></div>{settings.profiles.map(p => <div className="profile-row" key={p.id}><span className="vendor-letter">{vendors.find(v => v.id === p.vendor)?.name[0] ?? 'L'}</span><div><strong>{p.name}</strong><small>{p.authMethod === 'browser' ? t("Login · ") : ''}{p.model === 'default' ? t("Modelo automático") : p.model} · {p.vision ? t("Visão habilitada") : t("Texto")}</small></div><button className="button secondary" onClick={() => void attempt(async () => setNotice(await call<string>('test_profile', { id: p.id })))}>{t("Testar")}</button><button className="button secondary" disabled={running || busy || !!testingOperator} onClick={()=>void attempt(async()=>{setTestingOperator(p.id);setNotice(t("Testando contrato do operador…"));try{setNotice(await call<string>('test_operator_profile',{id:p.id}));}finally{setTestingOperator('');}})}>{t(testingOperator===p.id?"Testando…":"Testar operador")}</button><button className="icon-button" aria-label={t("Editar ") + p.name} onClick={() => p.vendor === 'builtin' ? document.getElementById('local-vision')?.scrollIntoView({ behavior: 'smooth' }) : setModelModal({ vendor: vendors.find(v => v.id === p.vendor)!, profile: p })}><Settings2 size={17}/></button><button className="button stop-button" disabled={running || busy} aria-label={t("Remover perfil") + ' ' + p.name} onClick={() => {setRemoveProfileError(''); setRemoveProfile(p);}}><Trash2 size={15}/>{t("Remover")}</button></div>)}</section>}{<div id="local-vision"><LocalVision call={call} native={native} running={running} onProfiles={s => { setSettings(s); localStorage.setItem('agentsmith-settings-updated', String(Date.now())); }}/></div>}{['EUA', 'China', 'Local'].map(country => <div key={country} className="provider-group"><div className="group-title"><h3>{country === 'EUA' ? t("Estados Unidos") : country === 'China' ? 'China' : t("No seu Mac")}</h3><span>{country === 'Local' ? t("Privacidade e controle local") : t("Seleção inicial de fornecedores")}</span></div><div className="provider-grid">{vendors.filter(v => v.country === country).map((v, i) => <button className="provider-card" key={v.id} onClick={() => setModelModal({ vendor: v })}><div className={'vendor-symbol vendor-' + v.id}>{v.local ? <Terminal size={21}/> : v.name.slice(0, 1)}</div><strong>{t(v.name)}</strong><span>{t(v.family)}</span><div className="provider-bottom"><small>{v.local ? t("Servidor local") : loginClients[v.id] ? t("Login ou API key") : t("API própria")}</small><Plus size={15}/></div></button>)}</div></div>)}<div className="info-strip"><ShieldCheck size={22}/><div><strong>{t("Capacidade pertence ao modelo, não ao nome do provedor.")}</strong><p>{t("Planejar, Operar e Verificar aceitam modelos de texto. O OCR lê a tela; o apoio visual recebe imagens quando necessário.")}</p></div></div></>}
 {page === 'routes' && <><div className="page-heading"><div><div className="eyebrow">{t("AS PESSOAS CERTAS, EM CADA PAPEL")}</div><h1>{t("Escolha quem pensa e quem confere.")}</h1><p>{t("Use o mesmo modelo em tudo ou distribua o trabalho entre perfis.")}</p></div></div><section className="panel privacy-panel"><div><LockKeyhole size={23}/><div><h3>{t("Somente local")}</h3><p>{t("Permite apenas endpoints neste Mac. Nunca usa nuvem como alternativa.")}</p></div></div><button aria-label={t("Somente local")} aria-pressed={settings.localOnly} className={'switch ' + (settings.localOnly ? 'on' : '')} onClick={() => void attempt(() => persist({ ...settings, localOnly: !settings.localOnly }))}><span /></button></section><div className="routing-grid">{([{ role: 'planner', name: t("Planejar"), sub: t("Transforma seu objetivo em etapas verificáveis."), icon: BookOpen }, { role: 'operator', name: t("Operar"), sub: t("Usa os textos do OCR para escolher a próxima ação."), icon: MousePointer2 }, { role: 'verifier', name: t("Verificar"), sub: t("Propõe a conclusão por texto; o apoio visual confirma. Critérios OCR exatos são conferidos pelo motor."), icon: CheckCircle2 }, { role: 'vision', name: t("Apoio visual"), sub: t("Usado quando o OCR não basta ou a tela não apresenta progresso."), icon: Monitor }] as const).map(({ role, name, sub, icon: Icon }, i) => <section className="panel route-card" key={role}><div className="route-number">0{i + 1}</div><Icon size={25} className="green-text"/><h2>{name}</h2><p>{sub}</p>{[0, 1].map(priority => <Label key={priority} title={priority === 0 ? t("Modelo principal") : t("Alternativa se indisponível")}><select value={settings.routes[role]?.[priority] ?? ''} onChange={e => void attempt(() => { const route = [...(settings.routes[role] ?? [])]; route[priority] = e.target.value; return persist({ ...settings, routes: { ...settings.routes, [role]: Array.from(new Set(route.filter(Boolean))) } }); })}><option value="">{priority === 0 ? t("Selecionar perfil") : t("Sem alternativa")}</option>{settings.profiles.filter(p => p.enabled && (role !== 'vision' || p.vision)).map(p => <option value={p.id} key={p.id}>{p.name}</option>)}</select></Label>)}<small>{role === 'vision' ? t("Aceita imagens. Sem seleção, usa os perfis visuais já atribuídos a Operar ou Verificar.") : t("Texto estruturado · imagens apenas no apoio visual")}</small></section>)}</div><div className="panel limits-panel"><div><h3>{t("Limite de ações por tarefa")}</h3><p>{t("Ao atingir o limite, Smith salva o progresso e sinaliza para revisão.")}</p></div><input aria-label={t("Limite de ações")} type="number" min="1" max="500" value={settings.maxActions} onChange={e => void attempt(() => persist({ ...settings, maxActions: Number(e.target.value) }))}/></div><div className="info-strip"><Route size={20}/><p>{t("A alternativa só é consultada em falhas transitórias, como indisponibilidade ou limite de requisições. As ações no Windows continuam sob um único executor.")}</p></div></>}
 {page === 'architecture' && <><div className="page-heading"><div><div className="eyebrow">{t("ARQUITETURA AGENTSMITH")}</div><h1>{t("Entender. Agir. Conferir.")}</h1><p>{t("Uma base modular para controlar computadores Windows a partir do Mac.")}</p></div></div><div className="architecture-flow"><div className="arch-box"><Monitor /><h3>{t("Aplicativo macOS")}</h3><p>{t("Tauri + React · Configuração e acompanhamento")}</p></div><ArrowDown /><div className="arch-middle"><div className="arch-box"><Cpu /><h3>{t("Roteador de modelos")}</h3><p>{t("Motor local integrado · 10 fornecedores · APIs locais")}</p></div><div className="arch-box accent"><Brand small/><h3>{t("Executor Rust")}</h3><p>{t("Plano → Observação → Ação → Verificação")}</p></div><div className="arch-box"><LockKeyhole /><h3>{t("Estado persistente")}</h3><p>{t("SQLite · Chaves do macOS · Histórico")}</p></div></div><ArrowDown /><div className="arch-box"><Network /><h3>{t("Adaptadores de conexão")}</h3><p>{t("FreeRDP nativo nesta prévia · RustDesk e NanoKVM nas próximas etapas")}</p></div><ArrowDown /><div className="arch-box windows"><Computer /><h3>{t("Computadores Windows")}</h3><p>{t("Mouse e teclado enviados à sessão remota")}</p></div></div><div className="architecture-notes"><div className="panel"><h3>{t("O que já está nesta prévia")}</h3><p>{t("Perfis de IA, quatro formatos de API, modo local, planejamento de roteiros, executor visual, controle RDP, pausa, histórico e retomada manual.")}</p></div><div className="panel"><h3>{t("Validação e próximas etapas")}</h3><p>{t("As contas de LLM e uma máquina Windows real precisam ser configuradas para validar o fluxo completo. RustDesk, NanoKVM, reconexão automática e execução simultânea ainda estão pendentes.")}</p></div></div></>}
 </main><footer><span><Brand small/> AgentSmith <span className="footer-dot">·</span>{t(" Seu próximo ajudante.")}</span><span>{t("macOS primeiro. Arquitetura preparada para evoluir.")}</span></footer></div>
 {displayForm}{performanceForm}
 {removeProfile && <Modal title={t("Remover perfil")} sub={removeProfile.name} close={() => {if (!removingProfile) setRemoveProfile(null);}}><p>{t("O perfil e sua chave salva serão removidos. O histórico, os modelos baixados e o login compartilhado no cliente oficial serão mantidos.")}</p><p>{t("As referências no roteamento de IA serão removidas; a alternativa existente passa a ser principal. Confira o roteamento antes de executar outra tarefa.")}</p>{removeProfileError && <div className="login-error" role="alert">{systemText(removeProfileError)}</div>}<div className="modal-actions"><button className="button secondary" disabled={removingProfile} onClick={() => setRemoveProfile(null)}>{t("Cancelar")}</button><button className="button stop-button" disabled={removingProfile || running || busy} onClick={async () => {setRemovingProfile(true);setRemoveProfileError('');try {const next=await call<Settings>('remove_profile',{id:removeProfile.id});setSettings(next);localStorage.setItem('agentsmith-settings-updated',String(Date.now()));setRemoveProfile(null);setNotice(t("Perfil removido. Confira o roteamento de IA."));}catch(e){setRemoveProfileError(String(e));}finally{setRemovingProfile(false);}}}><Trash2 size={15}/>{t(removingProfile ? "Removendo…" : "Remover perfil")}</button></div></Modal>}
 {modelModal && <ModelForm vendor={modelModal.vendor} profile={modelModal.profile} close={() => setModelModal(null)} save={saveModel} call={call} attempt={attempt}/>}
 {machineModal && <MachineForm machine={machineModal === 'new' ? undefined : machineModal} close={() => setMachineModal(null)} save={saveMachine} call={call}/>}
 {connectModal && <ConnectForm machine={connectModal} close={() => setConnectModal(null)} connect={connectWithPassword}/>}
 </div>;
}
function PerformanceForm({ value, close, save }: {
    value: PerformanceSettings;
    close: () => void;
    save: (value: PerformanceSettings) => Promise<void>;
}) {
    const [p, setP] = useState({...defaultPerformance,...value}), [saving, setSaving] = useState(false), [error, setError] = useState('');
    return <Modal title={t("Ritmo de operação")} sub={t("Equilibre resposta, nitidez e consumo do Mac.")} close={close}><form onSubmit={e => { e.preventDefault(); setSaving(true); setError(''); void save(p).catch(e => setError(String(e))).finally(() => setSaving(false)); }}>
 <div className="performance-presets">{[{ name: t("Turbo"), captureIntervalMs: 50, postActionDelayMs: 0, visionMaxWidth: 1280 }, { name: t("Ágil"), captureIntervalMs: 150, postActionDelayMs: 250, visionMaxWidth: 1280 }, { name: t("Equilibrado"), ...defaultPerformance }, { name: t("Econômico"), captureIntervalMs: 1000, postActionDelayMs: 1200, visionMaxWidth: 1600 }].map(({ name, ...v }) => <button type="button" className="button secondary" key={name} onClick={() => setP(old=>({...old,...v,nativeOcr:old.nativeOcr,allowCrops:old.allowCrops}))}>{name}</button>)}</div>
 <Label title={t("Intervalo entre capturas")} hint={t("20 a 2.000 ms. Menor intervalo deixa a visualização mais fluida e usa mais CPU.")}><input aria-label={t("Intervalo entre capturas em milissegundos")} type="number" required min="20" max="2000" step="1" value={p.captureIntervalMs} onChange={e => setP({ ...p, captureIntervalMs: Number(e.target.value) })}/><small>{(1000 / Math.max(20, p.captureIntervalMs)).toFixed(1)}{t(" capturas por segundo · Atualização da interface acompanha esse ritmo.")}</small></Label>
 <Label title={t("Pausa após cada ação")} hint={t("0 a 3.000 ms. Zero remove a pausa extra; o aplicativo ainda aguarda uma nova imagem.")}><input aria-label={t("Pausa após cada ação em milissegundos")} type="number" required min="0" max="3000" step="1" value={p.postActionDelayMs} onChange={e => setP({ ...p, postActionDelayMs: Number(e.target.value) })}/></Label>
 <p className="muted">{t("Esses ajustes reduzem as esperas do aplicativo. OCR, rede e resposta do modelo têm tempos próprios; mais capturas não geram mais chamadas à IA.")}</p>
 <Label title={t("Imagem enviada à IA")} hint={t("Imagens menores reduzem os dados, mas podem dificultar a leitura de textos pequenos. O zoom da RDP é independente.")}><select aria-label={t("Largura máxima da imagem para IA")} value={p.visionMaxWidth} onChange={e => setP({ ...p, visionMaxWidth: Number(e.target.value) })}><option value="1280">{t("Até 1280 pixels · Mais leve")}</option><option value="1600">{t("Até 1600 pixels · Equilibrado")}</option><option value="1920">{t("Até 1920 pixels · Mais detalhe")}</option><option value="2560">{t("Até 2560 pixels")}</option><option value="0">{t("Resolução original")}</option></select></Label>
 <label className="ocr-toggle"><input type="checkbox" checked={p.nativeOcr} onChange={e=>setP({...p,nativeOcr:e.target.checked})}/>{t("Ler textos com OCR nativo do macOS")}</label><label className="ocr-toggle"><input type="checkbox" checked={p.allowCrops} onChange={e=>setP({...p,allowCrops:e.target.checked})}/>{t("Permitir recortes solicitados pela IA")}</label><div className="display-preview">{t("Com OCR ativado, o modelo recebe textos e posições. Imagens são enviadas ao apoio visual quando necessário. Leituras são reutilizadas enquanto a área observada não mudar. Desativar OCR usa o caminho visual.")}</div>
 {error && <div className="alert" role="alert">{systemText(error)}</div>}<div className="modal-actions"><button type="button" className="button secondary" onClick={close}>{t("Cancelar")}</button><button className="button primary" disabled={saving}>{saving ? t("Salvando…") : t("Salvar ritmo")}</button></div></form></Modal>;
}
function DisplayForm({ machine, close, save }: {
    machine: Machine;
    close: () => void;
    save: (d: DisplaySettings) => Promise<void>;
}) {
    const [display, setDisplay] = useState<DisplaySettings>(machine.display ?? defaultDisplay), [saving, setSaving] = useState(false), [error, setError] = useState('');
    const presets = [[1280, 800], [1600, 900], [1920, 1080], [2560, 1440]];
    const options = presets.some(([w, h]) => w === display.width && h === display.height) ? presets : [[display.width, display.height], ...presets];
    return <Modal title={t("Ajustar tela do Windows")} sub={machine.name} close={close}><form onSubmit={e => { e.preventDefault(); setSaving(true); setError(''); void save(display).catch(e => setError(String(e))).finally(() => setSaving(false)); }}>
 <Label title={t("Resolução da sessão")} hint={t("Mais resolução oferece mais espaço para janelas e aplicativos.")}><select aria-label={t("Resolução da sessão")} value={`${display.width}x${display.height}`} onChange={e => { const [width, height] = e.target.value.split('x').map(Number); setDisplay({ ...display, width, height }); }}>{options.map(([w, h]) => <option key={`${w}x${h}`} value={`${w}x${h}`}>{w} × {h}{w === 1600 ? t(" · Equilibrado") : w === 1920 ? t(" · Full HD") : ''}</option>)}</select></Label>
 <Label title={t("Escala do Windows")} hint={t("100% usa ícones e textos menores. Aumente se precisar de letras maiores.")}><select aria-label={t("Escala do Windows")} value={display.scale} onChange={e => setDisplay({ ...display, scale: Number(e.target.value) })}>{[100, 125, 150, 200].map(v => <option key={v} value={v}>{v}%</option>)}</select></Label>
 <div className="display-preview">{t("Os ajustes ficam salvos nesta máquina e são solicitados ao reconectar. O Windows pode exigir sair da conta para aplicar uma mudança de escala. Para ampliar ou reduzir apenas a imagem no Mac, use o controle de Zoom.")}</div>
 {error && <div className="alert" role="alert">{systemText(error)}</div>}
 <div className="modal-actions"><button type="button" className="button secondary" onClick={close}>{t("Cancelar")}</button><button className="button primary" disabled={saving}>{saving ? t("Salvando…") : t("Salvar e conectar…")}</button></div></form></Modal>;
}
function ModelForm({ vendor, profile, close, save, call, attempt }: {
    vendor: Vendor;
    profile?: Profile;
    close: () => void;
    save: (p: Profile, s: string) => Promise<void>;
    call: <T>(n: string, a?: Record<string, unknown>) => Promise<T>;
    attempt: (f: () => Promise<unknown>) => Promise<void>;
}) {
    const supportsLogin = Boolean(loginClients[vendor.id]);
    const initial: Profile = { id: crypto.randomUUID(), vendor: vendor.id, name: vendor.name, protocol: vendor.protocol, baseUrl: vendor.baseUrl, model: '', vision: false, enabled: true, authMethod: 'api_key' };
    const [p, setP] = useState<Profile>(profile ?? (supportsLogin ? withAuth(initial, vendor, 'browser') : initial)), [secret, setSecret] = useState(''), [saving, setSaving] = useState(false), [models, setModels] = useState<string[]>([]), [formError, setFormError] = useState('');
    const browser = p.authMethod === 'browser';
    return <Modal title={profile ? t("Editar perfil") : t("Adicionar {0}", { "0": t(vendor.name) })} sub={t(vendor.family)} close={close}><form onSubmit={e => { e.preventDefault(); setSaving(true); setFormError(''); void save(p, browser ? '' : secret).catch(e => setFormError(String(e))).finally(() => setSaving(false)); }}>
 {supportsLogin && <div className="auth-selector" role="group" aria-label={t("Forma de conexão")}><button type="button" aria-pressed={browser} className={browser ? 'selected' : ''} onClick={() => { setP(withAuth(p, vendor, 'browser')); setSecret(''); }}><Globe size={16}/>{t(" Login pelo navegador")}</button><button type="button" aria-pressed={!browser} className={!browser ? 'selected' : ''} onClick={() => { setP(withAuth(p, vendor, 'api_key')); setSecret(''); }}><KeyRound size={16}/>{t(" API key")}</button></div>}
 <div className="form-grid"><Label title={t("Nome do perfil")}><input required value={p.name} onChange={e => setP({ ...p, name: e.target.value })} placeholder={t("Ex.: Planejador principal")}/></Label><Label title={browser ? t("Modelo da sua conta") : t("ID exato do modelo")} hint={browser ? t("Use default para o padrão do programa, ou informe um modelo disponível na sua conta.") : undefined}><input required list="models-list" value={p.model} onChange={e => setP({ ...p, model: e.target.value })} placeholder={browser ? 'default' : t("Copie o ID disponível na sua conta")}/><datalist id="models-list">{browser && <option value="default">{t("Automático · padrão da conta")}</option>}{models.map(m => <option key={m} value={m}/>)}</datalist></Label></div>
 {browser ? <LoginConnection profile={p} call={call}/> : <><Label title={t("Endereço base da API")} hint={vendor.id === 'amazon' ? t("Use o endpoint Bedrock da região com acesso ao modelo. Autenticação por chave de API Bedrock; IAM/SigV4 ainda não incluído.") : vendor.local ? t("Inicie o servidor local e carregue o modelo antes de testar.") : t("O endereço e a chave precisam corresponder à mesma região e conta.")}><input required value={p.baseUrl} onChange={e => setP({ ...p, baseUrl: e.target.value.trim() })}/></Label><Label title={vendor.local ? t("Chave do servidor (opcional)") : t("Chave da API")} hint={t("Guardada no Chaves do macOS. Deixe em branco para manter a chave já cadastrada neste endpoint.")}><input type="password" autoComplete="new-password" value={secret} onChange={e => setSecret(e.target.value)} placeholder={profile ? t("•••••••• (não exibida)") : t("Cole sua chave aqui")}/></Label></>}
 {vendor.id === 'deepseek' && <p className="muted">{t("Para imagens na API DeepSeek, use deepseek-v4-flash-vision-exp. Flash e Pro sem vision são modelos de texto. Ações curtas usam o modo sem raciocínio prolongado.")}</p>}
 <label className="checkbox-row"><input type="checkbox" checked={p.vision} onChange={e => setP({ ...p, vision: e.target.checked })}/><span><strong>{t("Este modelo aceita imagens")}</strong><small>{t("Permite usar este perfil no apoio visual. O teste de conexão verifica a resposta de texto.")}</small></span></label>
 {formError && <div className="login-error" role="alert">{systemText(formError)}</div>}
 <div className="modal-actions">{profile && !browser && <button type="button" className="button secondary" onClick={() => void attempt(async () => setModels(await call<string[]>('list_models', { id: profile.id })))}>{t("Consultar modelos")}</button>}<button type="button" className="button secondary" onClick={close}>{t("Cancelar")}</button><button disabled={saving} className="button primary">{saving ? <Loader2 size={15} className="spin"/> : <Check size={15}/>}{t(" Salvar perfil")}</button></div></form></Modal>;
}
function MachineForm({ machine, close, save, call }: {
    machine?: Machine;
    close: () => void;
    save: (m: Machine, p: string) => Promise<void>;
    call: <T>(name: string, args?: Record<string, unknown>) => Promise<T>;
}) {
    const [m, setM] = useState<Machine>(machine ?? { id: crypto.randomUUID(), name: '', protocol: 'rdp', host: '', port: 3389, username: '', domain: '', fingerprint: '' }), [password, setPassword] = useState(''), [saving, setSaving] = useState(false), [saveError, setSaveError] = useState(''), [saved, setSaved] = useState<boolean | null>(machine ? null : false), [statusError, setStatusError] = useState('');
    useEffect(() => { if (!machine)
        return; let closed = false; void call<boolean>('machine_credential_status', { id: machine.id }).then(v => { if (!closed)
        setSaved(v); }).catch(e => { if (!closed)
        setStatusError(String(e)); }); return () => { closed = true; }; }, [machine?.id]);
    const sameTarget = machine && m.host === machine.host && m.port === machine.port && m.username === machine.username && m.domain === machine.domain && m.protocol === machine.protocol;
    const savedHere = Boolean(saved && sameTarget);
    return <Modal title={machine ? t("Editar máquina") : t("Adicionar máquina Windows")} sub={t("Uma conexão salva para o seu próximo trabalho.")} close={close}><form onSubmit={e => { e.preventDefault(); setSaving(true); setSaveError(''); void save(m, password).catch(e => setSaveError(String(e))).finally(() => setSaving(false)); }}><div className="form-grid"><Label title={t("Nome da máquina")}><input required value={m.name} onChange={e => setM({ ...m, name: e.target.value })} placeholder={t("Ex.: Atendimento 01")}/></Label><Label title={t("Tipo de conexão")}><select value={m.protocol} onChange={e => setM({ ...m, protocol: e.target.value as Machine['protocol'] })}>{Object.entries(protocolNames).map(([k, v]) => <option key={k} value={k}>{v}{k !== 'rdp' ? t(" · em desenvolvimento") : ''}</option>)}</select></Label></div><div className="form-grid wide"><Label title={t("Endereço IP ou hostname")}><input required value={m.host} onChange={e => setM({ ...m, host: e.target.value })} placeholder="192.168.1.10"/></Label><Label title={t("Porta")}><input type="number" required min="1" max="65535" value={m.port} onChange={e => setM({ ...m, port: Number(e.target.value) })}/></Label></div><div className="form-grid"><Label title={t("Usuário do Windows")}><input value={m.username} onChange={e => setM({ ...m, username: e.target.value })} placeholder={t("operador")}/></Label><Label title={t("Domínio (opcional)")}><input value={m.domain} onChange={e => setM({ ...m, domain: e.target.value })}/></Label></div><Label title={t("Senha (opcional)")} hint={savedHere ? t("Senha salva no Chaves do macOS. Deixe em branco para manter; digite apenas para trocar.") : machine && sameTarget && saved === null ? statusError || t("Verificando se existe senha salva…") : t("Nenhuma senha salva para este endereço e usuário. Informe para conectar automaticamente nas próximas vezes.")}><input type="password" autoComplete="new-password" placeholder={savedHere ? t("Senha salva · não exibida") : t("Digite para salvar no Chaves")} value={password} onChange={e => setPassword(e.target.value)}/></Label><Label title={t("Impressão digital do certificado (opcional)")} hint={t("Para certificado não reconhecido, copie a impressão digital confirmada com o administrador. Não desativa a validação TLS.")}><input value={m.fingerprint} onChange={e => setM({ ...m, fingerprint: e.target.value })} placeholder={t("SHA-256 exibido na primeira conexão")}/></Label>{saveError && <div className="login-error" role="alert">{systemText(saveError)}</div>}<div className="modal-actions"><button type="button" className="button secondary" onClick={close}>{t("Cancelar")}</button><button disabled={saving} className="button primary"><Check size={15}/>{t(" Salvar máquina")}</button></div></form></Modal>;
}
function ConnectForm({ machine, close, connect }: {
    machine: Machine;
    close: () => void;
    connect: (p: string, remember: boolean) => Promise<void>;
}) {
    const [p, setP] = useState(''), [remember, setRemember] = useState(true), [busy, setBusy] = useState(false), [error, setError] = useState('');
    return <Modal title={t("Conectar a {0}", { "0": machine.name })} sub={`${machine.host}:${machine.port} · ${machine.username}`} close={close}><form onSubmit={e => { e.preventDefault(); setBusy(true); setError(''); void connect(p, remember).catch(e => setError(String(e))).finally(() => setBusy(false)); }}>
 <Label title={t("Senha do Windows")} hint={t("Não há senha salva para este endereço e usuário.")}><input type="password" required autoFocus autoComplete="current-password" value={p} onChange={e => setP(e.target.value)}/></Label>
 <label className="checkbox-row"><input type="checkbox" checked={remember} onChange={e => setRemember(e.target.checked)}/><span><strong>{t("Salvar senha no Chaves do macOS")}</strong><small>{t("Na próxima conexão, Smith usará a senha salva automaticamente.")}</small></span></label>
 {error && <div className="login-error" role="alert">{systemText(error)}</div>}<div className="modal-actions"><button type="button" className="button secondary" onClick={close}>{t("Cancelar")}</button><button disabled={busy || machine.protocol !== 'rdp'} className="button primary"><Power size={15}/> {busy ? t("Conectando…") : t("Conectar")}</button></div></form></Modal>;
}
