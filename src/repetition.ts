import {t,getLanguage} from './i18n';
export type RepeatOptions = {
    schedule: {
        mode: 'duration';
        minutes: number;
    } | {
        mode: 'window';
        startsAt: number;
        endsAt: number;
    } | ({
        mode: 'weekly';
    } & WeeklySchedule);
    intervalSeconds: number;
};
export type RepeatState = {
    weekly?: WeeklySchedule | null;
    startsAt: number;
    endsAt: number;
    intervalSeconds: number;
    cycle: number;
    completedCycles: number;
    totalActions: number;
    betweenCycles: boolean;
    nextCycleAt: number;
};
export function timeWindow(from: string, to: string, now = new Date()): {
    startsAt: number;
    endsAt: number;
} {
    const valid = (value: string) => /^([01]\d|2[0-3]):[0-5]\d$/.test(value);
    if (!valid(from) || !valid(to) || from === to)
        throw new Error(t("Informe hor\u00E1rios de in\u00EDcio e fim diferentes."));
    const at = (value: string) => { const d = new Date(now); const [h, m] = value.split(':').map(Number); d.setHours(h, m, 0, 0); return d; };
    const start = at(from), end = at(to);
    if (end <= start) {
        end.setDate(end.getDate() + 1);
        if (now < start) {
            const previousEnd = at(to);
            if (now < previousEnd) {
                start.setDate(start.getDate() - 1);
                end.setDate(end.getDate() - 1);
            }
        }
    }
    if (end <= now) {
        start.setDate(start.getDate() + 1);
        end.setDate(end.getDate() + 1);
    }
    return { startsAt: start.getTime(), endsAt: end.getTime() };
}
export function formatMoment(value: number) { return new Date(value).toLocaleString(getLanguage(), { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' }); }
export const weekDays = [{ day: 1, label: 'Seg', name: 'Segunda-feira' }, { day: 2, label: 'Ter', name: 'Terça-feira' }, { day: 3, label: 'Qua', name: 'Quarta-feira' }, { day: 4, label: 'Qui', name: 'Quinta-feira' }, { day: 5, label: 'Sex', name: 'Sexta-feira' }, { day: 6, label: 'Sáb', name: 'Sábado' }, { day: 7, label: 'Dom', name: 'Domingo' }];
export function weeklyWindow(from: string, to: string, days: number[], now = new Date(), dates: DateRange = {}) {
    timeWindow(from, to, now);
    if (!days.length || days.some(day => !Number.isInteger(day) || day < 1 || day > 7))
        throw new Error(t("Selecione pelo menos um dia da semana."));
    const first = parseDate(dates.startDate), last = parseDate(dates.endDate);
    if (first && last && last < first)
        throw new Error(t("A data de fim deve ser igual ou posterior \u00E0 data de in\u00EDcio."));
    const base = first && first > now ? first : now;
    const cutoff = last ? new Date(last) : undefined;
    if (cutoff)
        cutoff.setDate(cutoff.getDate() + 1);
    const [fh, fm] = from.split(':').map(Number), [th, tm] = to.split(':').map(Number);
    for (let offset = -1; offset <= 7; offset++) {
        const start = new Date(base);
        start.setDate(start.getDate() + offset);
        start.setHours(fh, fm, 0, 0);
        const day = new Date(start);
        day.setHours(0, 0, 0, 0);
        if (last && day > last)
            break;
        if (first && day < first)
            continue;
        if (!days.includes((start.getDay() + 6) % 7 + 1))
            continue;
        const end = new Date(start);
        end.setHours(th, tm, 0, 0);
        if (to < from)
            end.setDate(end.getDate() + 1);
        if (cutoff && end > cutoff)
            end.setTime(cutoff.getTime());
        if (end > now && end > start)
            return { startsAt: start.getTime(), endsAt: end.getTime() };
    }
    throw new Error(t("N\u00E3o h\u00E1 dia e hor\u00E1rio dispon\u00EDveis no per\u00EDodo escolhido."));
}
export type DateRange = {
    startDate?: string | null;
    endDate?: string | null;
};
export type WeeklySchedule = DateRange & {
    weekdays: number[];
    startMinute: number;
    endMinute: number;
};
export function dateValue(date = new Date()) { return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`; }
function parseDate(value?: string | null) {
    if (!value)
        return undefined;
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value))
        throw new Error(t("Informe uma data v\u00E1lida."));
    const [y, m, d] = value.split('-').map(Number), date = new Date(y, m - 1, d);
    if (y < 1970 || dateValue(date) !== value)
        throw new Error(t("Informe uma data v\u00E1lida."));
    return date;
}
export function formatDate(value: string) { const [y,m,d]=value.split('-').map(Number);return new Date(y,m-1,d).toLocaleDateString(getLanguage()); }
