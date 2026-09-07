import {t,systemText} from './i18n';
import { useEffect, useRef, useState } from 'react';
import { ArrowLeft, ChevronDown, ChevronUp, Pause, Square, ExternalLink, LockKeyhole, Maximize2, Minimize2, Monitor, MousePointer2, Plus, Power, Settings2, Timer, X } from 'lucide-react';
import type { Machine, SessionInfo, Snapshot } from './types';
import { imagePoint, viewScale } from './geometry';
type Props = {
    topCollapsed:boolean;
    toggleTop:()=>void;
    stop:()=>void;
    stopping:boolean;
    activity:string;
    machines: Machine[];
    machine?: Machine;
    selectedMachine: string;
    session: SessionInfo;
    frame: Snapshot | null;
    manual: boolean;
    running: boolean;
    detachedView: boolean;
    focused: boolean;
    windowBusy: boolean;
    connecting: boolean;
    select: (id: string) => void;
    connect: () => void;
    add: () => void;
    assume: () => void;
    disconnect: () => void;
    act: (action: object) => Promise<boolean>;
    detach: () => void;
    attach: () => void;
    focus: () => void;
    display: () => void;
    performance: () => void;
};
export default function SessionPanel(p: Props) {
    const [text, setText] = useState('');
    const [zoom, setZoom] = useState('fit'), [viewportSize, setViewportSize] = useState({ width: 0, height: 0 });
    const viewport = useRef<HTMLDivElement>(null);
    const { frame, session, manual, running } = p;
    const away = Boolean(session.detached && !p.detachedView);
    const canControl = manual && !running && !away && Boolean(frame);
    useEffect(() => {
        const el = viewport.current;
        if (!el)
            return;
        const observer = new ResizeObserver(([entry]) => setViewportSize({ width: entry.contentRect.width, height: entry.contentRect.height }));
        observer.observe(el);
        return () => observer.disconnect();
    }, [away, Boolean(frame)]);
    useEffect(() => {
        try {
            setZoom(localStorage.getItem('agentsmith-rdp-zoom') || 'fit');
        }
        catch { }
        const sync = (e: StorageEvent) => { if (e.key === 'agentsmith-rdp-zoom')
            setZoom(e.newValue || 'fit'); };
        window.addEventListener('storage', sync);
        return () => window.removeEventListener('storage', sync);
    }, []);
    useEffect(() => {
        const el = viewport.current;
        if (!el)
            return;
        const wheel = (e: WheelEvent) => { if (canControl && !e.altKey && e.target instanceof HTMLImageElement) {
            e.preventDefault();
            void p.act({ kind: 'scroll', direction: e.deltaY > 0 ? 'down' : 'up', amount: 1 });
        } };
        el.addEventListener('wheel', wheel, { passive: false });
        return () => el.removeEventListener('wheel', wheel);
    }, [canControl, p.act]);
    const scale = frame ? viewScale(viewportSize.width, viewportSize.height, frame.width, frame.height, zoom) : 1;
    function changeZoom(value: string) { setZoom(value); try {
        localStorage.setItem('agentsmith-rdp-zoom', value);
    }
    catch { } if (viewport.current) {
        viewport.current.scrollTop = 0;
        viewport.current.scrollLeft = 0;
    } }
    return <section className={"panel session-panel "+(p.topCollapsed?"session-top-collapsed":"")} aria-label={t("Sessão Windows remota")}>
  {p.topCollapsed && <div className="rdp-compact-bar"><span className="rdp-compact-label" title={p.activity}>{p.machine?.name ?? t("Windows remoto")} · {p.activity || t("CONEXÃO")}</span>{running && <><button className="button secondary" onClick={p.assume}><Pause size={14}/>{t("Pausar")}</button><button className="button stop-button" disabled={p.stopping} onClick={p.stop}><Square size={14}/>{p.stopping?t("Parando…"):t("Parar")} · Esc</button></>}<button className="button secondary" onClick={p.focus} aria-pressed={p.focused} title={p.detachedView?t("Alternar tela cheia"):t("Ampliar a RDP e recolher os demais painéis")}>{p.focused?<Minimize2 size={14}/>:<Maximize2 size={14}/>} {p.detachedView?t("Tela cheia"):p.focused?t("Sair do foco"):t("Ampliar RDP")}</button><button className="button secondary" aria-expanded={false} onClick={p.toggleTop}><ChevronDown size={14}/>{t("Expandir parte superior")}</button></div>}
  <div className="rdp-toolbar">
   <div className="rdp-identity"><Monitor size={18}/><div><span className="tiny-label">{t("CONEXÃO · ")}{p.machine?.protocol === 'rdp' ? 'Microsoft RDP' : p.machine?.protocol === 'rustdesk' ? 'RustDesk' : t("Windows remoto")}</span><h3>{p.machine?.name ?? t("Seu próximo posto de trabalho")}</h3></div></div>
   <div className="rdp-window-actions">
    <button className="button secondary" aria-expanded={true} onClick={p.toggleTop}><ChevronUp size={14}/>{t("Recolher parte superior")}</button>
    {!away && <button className="button secondary" onClick={p.focus} aria-pressed={p.focused} title={p.detachedView ? t("Alternar tela cheia") : t("Ampliar a RDP e recolher os demais painéis")}>{p.focused ? <Minimize2 size={14}/> : <Maximize2 size={14}/>} {p.detachedView ? t("Tela cheia") : p.focused ? t("Sair do foco") : t("Ampliar RDP")}</button>}
    {p.detachedView ? <button className="button secondary" onClick={p.attach}><ArrowLeft size={14}/>{t(" Voltar à Central")}</button> : <button className="button secondary" disabled={p.windowBusy || !p.machines.length} onClick={p.detach}><ExternalLink size={14}/> {away ? t("Mostrar janela RDP") : t("Destacar")}</button>}
   </div>
  </div>
  {!away && <div className="session-controls">
   <select aria-label={t("Máquina selecionada")} value={p.selectedMachine} disabled={running} onChange={e => p.select(e.target.value)}><option value="">{t("Selecionar máquina")}</option>{p.machines.map(m => <option key={m.id} value={m.id}>{m.name}</option>)}</select>
   <button className="button secondary" disabled={!p.selectedMachine || running || p.connecting} onClick={p.connect}><Power size={14}/> {p.connecting ? t("Conectando…") : t("Conectar")}</button>
   <button className="button secondary" disabled={!frame} onClick={p.assume}><MousePointer2 size={14}/> {running ? t("Pausar e assumir") : t("Assumir")}</button>
   {session.machineId && <button className="icon-button" title={t("Desconectar")} aria-label={t("Desconectar")} onClick={p.disconnect}><X size={17}/></button>}
   <button className="button secondary" disabled={running || !p.machine} onClick={p.display}><Settings2 size={14}/>{t(" Tela")}</button><button className="button secondary" disabled={running} onClick={p.performance} title={running ? t("Pause a tarefa para ajustar") : t("Ajustar captura e tempo entre ações")}><Timer size={14}/>{t(" Ritmo")}</button><label className="rdp-zoom">Zoom<select aria-label={t("Zoom da visualização RDP")} value={zoom} onChange={e => changeZoom(e.target.value)}><option value="fit">{t("Ajustar à janela")}</option>{[50, 75, 100, 125, 150, 200].map(v => <option key={v} value={v}>{v}%</option>)}</select></label><span className="rdp-control-hint">{running ? t("Smith está executando a tarefa") : canControl ? t("Clique para controlar · Option + rolagem move a visualização") : t("Acompanhe a sessão ou assuma o controle")}</span>
  </div>}
  <div className={'remote-screen ' + (frame && !away ? 'has-frame' : '') + (away ? ' is-detached' : '')} tabIndex={canControl ? 0 : -1} onKeyDown={e => {
            if (!canControl)
                return;
            e.preventDefault();
            const key = e.key === 'Escape' ? 'esc' : e.key === ' ' ? 'space' : e.key.startsWith('Arrow') ? e.key.slice(5).toLowerCase() : e.key;
            const modifiers = [e.ctrlKey ? 'ctrl' : '', e.metaKey ? 'win' : '', e.altKey ? 'alt' : '', e.shiftKey ? 'shift' : ''].filter(Boolean);
            if (['Control', 'Meta', 'Alt', 'Shift'].includes(key))
                return;
            void p.act(key.length === 1 && !modifiers.length ? { kind: 'type_text', text: key } : { kind: 'key', keys: [...modifiers, key.toLowerCase()] });
        }}>
   {away ? <div className="screen-empty detached-message"><ExternalLink size={38}/><h3>{t("Sua sessão está em outra janela.")}</h3><p>{t("A conexão e as tarefas continuam aqui. Mova a janela RDP para outro monitor ou traga a visualização de volta.")}</p><div className="button-row"><button className="button dark" onClick={p.detach}>{t("Mostrar janela RDP")}</button><button className="button dark" onClick={p.attach}><ArrowLeft size={15}/>{t(" Trazer de volta")}</button></div></div> : frame ? <div className="remote-viewport" ref={viewport}><div className="remote-canvas" style={{ width: frame.width * scale, height: frame.height * scale }}><img style={{ width: frame.width * scale, height: frame.height * scale }} draggable={false} src={frame.dataUrl} alt={t("Tela atual do Windows remoto")} onClick={e => { if (!canControl)
            return; const pos = imagePoint(e.clientX, e.clientY, e.currentTarget.getBoundingClientRect(), frame.width, frame.height); if (pos)
            void p.act({ kind: 'click', ...pos }); e.currentTarget.closest<HTMLElement>('.remote-screen')?.focus(); }} onContextMenu={e => { e.preventDefault(); if (!canControl)
            return; const pos = imagePoint(e.clientX, e.clientY, e.currentTarget.getBoundingClientRect(), frame.width, frame.height); if (pos)
            void p.act({ kind: 'right_click', ...pos }); e.currentTarget.closest<HTMLElement>('.remote-screen')?.focus(); }}/></div></div>
            : <div className="screen-empty"><div className="orbit"><Monitor size={48} strokeWidth={1}/><div className="orbit-dot"/><div className="cursor-decoration"><MousePointer2 size={20}/></div></div><h3>{session.status === 'connecting' ? t("Conectando ao Windows…") : t("Uma máquina. Muitas possibilidades.")}</h3><p>{systemText(session.message) || t("A tela remota aparecerá aqui. Smith vai observar, agir e verificar o resultado.")}</p><button className="button dark" disabled={p.connecting} onClick={p.machines.length ? p.connect : p.add}><Plus size={16}/>{p.machines.length ? t("Conectar máquina") : t("Adicionar primeira máquina")}</button></div>}
   <div className="screen-caption"><span><span className={session.status === 'connected' ? 'status-dot' : 'neutral-dot'}/>{away ? t("Visualização destacada") : frame ? `${frame.width} × ${frame.height} · Zoom ${Math.round(scale * 100)}% · ${running ? t("Automação") : manual ? t("Controle manual") : t("Observação")}` : t("Aguardando conexão")}</span><span><LockKeyhole size={12}/>{t(" Sessão isolada do seu desktop")}</span></div>
  </div>
  {manual && frame && !away && <div className="manual-input"><input aria-label={t("Texto para digitar no Windows")} maxLength={400} placeholder={t("Texto para digitar no Windows…")} value={text} onChange={e => setText(e.target.value)}/><button className="button secondary" disabled={running || !text} onClick={() => { void p.act({ kind: 'type_text', text }).then(ok => { if (ok)
        setText(''); }); }}>{t("Digitar")}</button><button className="button secondary" disabled={running} onClick={() => void p.act({ kind: 'key', keys: ['ctrl', 'alt', 'end'] })}>Ctrl Alt End</button></div>}
 </section>;
}
