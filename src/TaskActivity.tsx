import {t,systemText} from './i18n';
import { useEffect, useState } from 'react';
import { AlertCircle, CheckCircle2, Loader2, Pause, Play, Route, Square, RotateCcw, Repeat } from 'lucide-react';
import type { Run } from './types';
import { formatMoment, formatDate, weekDays } from './repetition';
import './task-activity.css';
export function activityMessage(run: Run): string {
    if (run.status === 'cancelled')
        return t("Execução encerrada. Histórico preservado. Use Reiniciar para executar este roteiro desde a primeira etapa.");
    if (run.status === 'expired')
        return systemText(run.progress?.message??'') || t("O período de repetição foi encerrado.");
    if (run.status === 'completed')
        return t("Todas as etapas foram concluídas e verificadas.");
    if (run.status === 'ready')
        return t("Roteiro preparado. Revise as etapas e clique em Executar.");
    if (run.status === 'paused')
        return t("Execução pausada. Reconecte, se necessário, e retome para conferir a tela antes de agir.");
    return systemText(run.progress?.message??'') || systemText(run.log.at(-1)??'') || (run.status === 'verifying' ? t("Conferindo a tela…") : t("Preparando a próxima ação…"));
}
export default function TaskActivity({ run, planning, connected, onResume, onPause, onStop, stopping, onRestart, restarting, onRepeat, onRoutes }: {
    run?: Run;
    planning: boolean;
    connected: boolean;
    onResume: () => void;
    onPause: () => void;
    onStop: () => void;
    stopping: boolean;
    onRestart: () => void;
    restarting: boolean;
    onRepeat: () => void;
    onRoutes: () => void;
}) {
    const [clock, setClock] = useState(Date.now()), [planStarted, setPlanStarted] = useState(Date.now()), [pending, setPending] = useState(false);
    const running = !!run && ['running', 'verifying', 'waiting'].includes(run.status);
    useEffect(() => { if (planning)
        setPlanStarted(Date.now()); }, [planning]);
    useEffect(() => { if (!running && !planning)
        return; setClock(Date.now()); const timerId = setInterval(() => setClock(Date.now()), 1000); return () => clearInterval(timerId); }, [running, planning, run?.progress?.startedAt]);
    useEffect(() => setPending(false), [run?.status]);
    if (!planning && !run)
        return null;
    const status = planning ? 'planning' : run!.status;
    const active = planning || running, blocked = status === 'blocked';
    const done = run?.steps.filter(s => s.status === 'done').length ?? 0;
    const title = planning ? t("Preparando roteiro") : ({ running: t("Em execução"), verifying: t("Analisando a tela"), waiting: t("Repetição aguardando"), expired: t("Período encerrado"), blocked: t("Tarefa interrompida"), paused: t("Tarefa pausada"), ready: t("Pronta para executar"), completed: t("Tarefa concluída"), cancelled: t("Tarefa parada") }[status] ?? status);
    const seconds = Math.max(0, Math.floor((clock - (planning ? planStarted : run?.progress?.startedAt ?? run?.updatedAt ?? clock)) / 1000));
    return <section className={'task-activity ' + (blocked ? 'blocked' : status === 'completed' ? 'completed' : '')} aria-label={t("Andamento da tarefa")}>
  <div className="activity-icon">{active ? <Loader2 className="spin" size={21}/> : blocked ? <AlertCircle size={21}/> : status === 'completed' ? <CheckCircle2 size={21}/> : status === 'cancelled' ? <Square size={21}/> : <Pause size={21}/>}</div>
  <div className="activity-copy"><div className="activity-label"><strong>{restarting ? t("Reiniciando tarefa…") : stopping ? t("Parando tarefa…") : title}</strong>{!planning && <span>{run!.title} · {done}/{run!.steps.length}{t(" etapas confirmadas · ")}{run!.actionCount}{t(" ações")}</span>}</div>
   <p role={blocked ? 'alert' : 'status'}>{planning ? t("A IA está organizando o objetivo em etapas.") : activityMessage(run!)}</p>
   {!planning && run?.repetition && <p className="repeat-progress">{run.repetition.weekly && <>{weekDays.filter(d => run.repetition!.weekly!.weekdays.includes(d.day)).map(d => t(d.label)).join(', ')} · </>}{run.repetition.cycle === 0 || clock < run.repetition.startsAt ? t("Início: {0}", { "0": formatMoment(run.repetition.startsAt) }) : t("Ciclo {0}", { "0": run.repetition.cycle })} · {run.repetition.completedCycles}{t(" concluídos · ")}{run.repetition.totalActions}{t(" ações no total · Até ")}{formatMoment(run.repetition.endsAt)}{running ? t(" · {0} min {1}", { "0": Math.max(0, Math.ceil((run.repetition.endsAt - clock) / 60000)), "1": run.repetition.weekly ? t("neste período") : t("restantes") }) : ''}{run.repetition.weekly?.endDate && t(" · Fim do loop: {0}", { "0": formatDate(run.repetition.weekly.endDate) })}</p>}
   {active && <small>{seconds}{t("s nesta fase")}{seconds >= 20 && !planning && status !== 'waiting' ? t(" · Ainda aguardando resposta; você pode pausar ou parar com Esc.") : ''}</small>}
  </div>
  <div className="activity-actions">{!planning && running ? <button className="button secondary" disabled={pending || stopping || restarting} onClick={() => { setPending(true); onPause(); setTimeout(() => setPending(false), 1500); }}><Pause size={14}/>{t(" Pausar")}</button> : !planning && ['blocked', 'paused', 'ready'].includes(status) && <><button className="button secondary" onClick={onRoutes}><Route size={14}/>{t(" Configurar IA")}</button><button className="button primary" disabled={!connected || pending || stopping || restarting} onClick={() => { setPending(true); onResume(); setTimeout(() => setPending(false), 1500); }}><Play size={14}/>{status === 'ready' ? t("Executar") : t("Retomar")}</button></>}{!planning && ['running', 'verifying', 'waiting', 'blocked', 'paused', 'ready'].includes(status) && <button className="button stop-button" disabled={stopping || restarting} onClick={onStop} title={t("Encerrar esta tarefa. Esc durante a execução.")}><Square size={14}/>{stopping ? t("Parando…") : t("Parar")}{running && <kbd>Esc</kbd>}</button>}{!planning && <button className="button secondary" disabled={running || !connected || stopping || restarting || pending} onClick={onRestart} title={running ? t("Pause ou pare a execução antes de reiniciar.") : !connected ? t("Conecte a máquina desta tarefa para reiniciar.") : t("Executar o mesmo roteiro desde a primeira etapa, mantendo o histórico anterior.")}><RotateCcw size={14} className={restarting ? 'spin' : ''}/>{restarting ? t("Reiniciando…") : t("Reiniciar")}</button>}{!planning && <button className="button secondary" disabled={running || stopping || restarting || pending} onClick={onRepeat}><Repeat size={14}/>{t(" Repetir")}</button>}</div>
 </section>;
}
