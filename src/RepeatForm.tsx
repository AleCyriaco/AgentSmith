import {t,systemText} from './i18n';
import { useState } from 'react';
import { Repeat, Loader2, X } from 'lucide-react';
import { timeWindow, weeklyWindow, weekDays, formatMoment, dateValue, type RepeatOptions } from './repetition';
import type { Run } from './types';
import './repetition.css';
export default function RepeatForm({ run, connected, close, start }: {
    run: Run;
    connected: boolean;
    close: () => void;
    start: (options: RepeatOptions) => Promise<void>;
}) {
    const saved = run.repetition?.weekly;
    const time = (minute: number) => `${Math.floor(minute / 60).toString().padStart(2, '0')}:${(minute % 60).toString().padStart(2, '0')}`;
    const [days, setDays] = useState<number[]>(run.repetition?.weekly?.weekdays ?? [1, 2, 3, 4, 5]);
    const [mode, setMode] = useState(saved ? 'weekly' : 'duration'), [amount, setAmount] = useState(30), [unit, setUnit] = useState(1), [from, setFrom] = useState(saved ? time(saved.startMinute) : '09:00'), [to, setTo] = useState(saved ? time(saved.endMinute) : '18:00'), [interval, setInterval] = useState(run.repetition?.intervalSeconds ?? 5), [pending, setPending] = useState(false), [error, setError] = useState('');
    const [startDate, setStartDate] = useState(saved ? saved.startDate ?? '' : dateValue()), [endDate, setEndDate] = useState(saved?.endDate ?? '');
    const dates = { startDate: startDate || undefined, endDate: endDate || undefined };
    let previewError = '';
    let summary = '';
    try {
        if (mode !== 'duration') {
            const w = mode === 'weekly' ? weeklyWindow(from, to, days, new Date(), dates) : timeWindow(from, to);
            summary = t("{0} · Fim: {1}", { "0": w.startsAt <= Date.now() ? t("Início agora") : t("Início: {0}", { "0": formatMoment(w.startsAt) }), "1": formatMoment(w.endsAt) });
        }
    }
    catch (e) {
        previewError = String(e).replace(/^Error: /, '');
    }
    async function submit() {
        setError('');
        setPending(true);
        try {
            const minutes = amount * unit;
            if (!Number.isInteger(interval) || interval < 1 || interval > 3600)
                throw new Error(t("Escolha um intervalo de 1 a 3600 segundos."));
            if (mode === 'duration' && (!Number.isInteger(minutes) || minutes < 1 || minutes > 10080))
                throw new Error(t("Escolha uma duração de 1 minuto a 7 dias."));
            if (mode === 'weekly')
                weeklyWindow(from, to, days, new Date(), dates);
            const toMinutes = (value: string) => { const [h, m] = value.split(':').map(Number); return h * 60 + m; };
            await start({ schedule: mode === 'duration' ? { mode: 'duration', minutes } : mode === 'weekly' ? { mode: 'weekly', ...dates, weekdays: days, startMinute: toMinutes(from), endMinute: toMinutes(to) } : { mode: 'window', ...timeWindow(from, to) }, intervalSeconds: interval });
        }
        catch (e) {
            setError(String(e).replace(/^Error: /, ''));
        }
        finally {
            setPending(false);
        }
    }
    return <div className="modal-backdrop"><section className="modal repeat-modal" role="dialog" aria-modal="true" aria-label={t("Repetir roteiro")}>
 <button className="icon-button close" disabled={pending} onClick={close} aria-label={t("Fechar")}><X size={18}/></button>
 <div className="eyebrow">{t("AGENTSMITH / REPETIÇÃO")}</div><h2>{t("Repetir roteiro")}</h2><p className="muted">{run.title}</p>
 <label className="field"><span>{t("Quando repetir")}</span><select value={mode} disabled={pending} onChange={e => setMode(e.target.value)}><option value="duration">{t("Por duração")}</option><option value="window">{t("De um horário até outro")}</option><option value="weekly">{t("Dias da semana e horários")}</option></select></label>
 {mode === 'duration' ? <div className="repeat-fields"><label className="field"><span>{t("Duração")}</span><input type="number" min="1" max={unit === 1 ? 10080 : 168} value={amount} disabled={pending} onChange={e => setAmount(Number(e.target.value))}/></label><label className="field"><span>{t("Unidade")}</span><select value={unit} disabled={pending} onChange={e => setUnit(Number(e.target.value))}><option value="1">{t("Minutos")}</option><option value="60">{t("Horas")}</option></select></label></div> : <><div className="repeat-fields"><label className="field"><span>{t("De")}</span><input aria-label={t("Horário de início")} type="time" value={from} disabled={pending} onChange={e => setFrom(e.target.value)}/></label><label className="field"><span>{t("Até")}</span><input aria-label={t("Horário de fim")} type="time" value={to} disabled={pending} onChange={e => setTo(e.target.value)}/></label></div><p className="repeat-summary">{summary || previewError || t("Escolha os horários.")}{t(" · Horário deste Mac. Se o fim for anterior ao início, o período atravessa a meia-noite.")}</p>{mode === 'weekly' && <><div className="repeat-fields"><label className="field"><span>{t("Data de início")}</span><input aria-label={t("Data de início")} type="date" min="1970-01-01" max="9999-12-31" value={startDate} disabled={pending} onChange={e => setStartDate(e.target.value)}/></label><label className="field"><span>{t("Data de fim")}</span><input aria-label={t("Data de fim")} type="date" min={startDate || '1970-01-01'} max="9999-12-31" value={endDate} disabled={pending} onChange={e => setEndDate(e.target.value)}/></label></div><p className="repeat-summary">{t("Início em branco: a partir de agora. Fim em branco: até você parar. A data final é inclusiva; a execução não passa da meia-noite desse dia.")}</p><fieldset className="weekday-field"><legend>{t("Dias de execução")}</legend><div className="weekday-buttons">{weekDays.map(({ day, label, name }) => <button type="button" key={day} aria-label={t(name)} aria-pressed={days.includes(day)} disabled={pending} className={days.includes(day) ? 'selected' : ''} onClick={() => setDays(old => old.includes(day) ? old.filter(d => d !== day) : [...old, day].sort((a, b) => a - b))}>{t(label)}</button>)}</div><div className="weekday-presets"><button type="button" disabled={pending} onClick={() => setDays([1, 2, 3, 4, 5])}>{t("Seg a sex")}</button><button type="button" disabled={pending} onClick={() => setDays([1, 2, 3, 4, 5, 6, 7])}>{t("Todos os dias")}</button></div><p className="repeat-summary">{t("Repete nos dias marcados, dentro das datas escolhidas. Para horários que atravessam a meia-noite, vale o dia de início.")}</p></fieldset></>}</>}
 <label className="field"><span>{t("Intervalo entre ciclos (segundos)")}</span><input type="number" min="1" max="3600" value={interval} disabled={pending} onChange={e => setInterval(Number(e.target.value))}/></label>
 <p className="repeat-summary">{t("Cada ciclo começa pela primeira etapa e verifica o resultado. O período continua contando durante pausas. ")}{mode === 'weekly' ? t("Ao fim de cada horário, interrompe a execução e aguarda o próximo dia marcado.") : t("Ao chegar ao fim, a execução é interrompida.")}{t(" Erros suspendem a repetição para revisão.")}</p>
 <p className="repeat-summary">{t("Mantenha o AgentSmith aberto, o Mac acordado e o Windows conectado. Uma nova execução será criada; o histórico anterior permanece salvo.")}</p>
 {!connected && <p role="status">{t("Conecte a máquina deste roteiro antes de iniciar.")}</p>}{error && <div className="alert" role="alert">{systemText(error)}</div>}
 <div className="button-row"><button className="button secondary" disabled={pending} onClick={close}>{t("Cancelar")}</button><button className="button primary" disabled={pending || !connected || (mode !== 'duration' && !!previewError)} onClick={() => void submit()}>{pending ? <Loader2 size={16} className="spin"/> : <Repeat size={16}/>} {pending ? t("Iniciando…") : t("Iniciar repetição")}</button></div>
 </section></div>;
}
