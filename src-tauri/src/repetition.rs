use chrono::{Datelike, TimeZone};
use serde::{Deserialize, Serialize};
const MAX_PERIOD: u64 = 7 * 24 * 60 * 60 * 1000;
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Schedule {
    Duration {
        minutes: u32,
    },
    Weekly {
        weekdays: Vec<u32>,
        #[serde(rename = "startMinute")]
        start_minute: u32,
        #[serde(rename = "endMinute")]
        end_minute: u32,
        #[serde(default, rename = "startDate")]
        start_date: Option<String>,
        #[serde(default, rename = "endDate")]
        end_date: Option<String>,
    },
    Window {
        #[serde(rename = "startsAt")]
        starts_at: u64,
        #[serde(rename = "endsAt")]
        ends_at: u64,
    },
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepeatOptions {
    pub schedule: Schedule,
    pub interval_seconds: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepeatState {
    #[serde(default)]
    pub weekly: Option<WeeklySchedule>,
    pub starts_at: u64,
    pub ends_at: u64,
    pub interval_seconds: u32,
    pub cycle: u32,
    pub completed_cycles: u32,
    pub total_actions: u64,
    pub between_cycles: bool,
    pub next_cycle_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeeklySchedule {
    pub weekdays: Vec<u32>,
    pub start_minute: u32,
    pub end_minute: u32,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}
impl WeeklySchedule {
    fn validate(&self) -> Result<(), String> {
        if self.weekdays.is_empty()
            || self.weekdays.len() > 7
            || self.weekdays.iter().any(|d| !(1..=7).contains(d))
        {
            return Err("Selecione pelo menos um dia válido, de segunda a domingo.".into());
        }
        if self.start_minute >= 1440
            || self.end_minute >= 1440
            || self.start_minute == self.end_minute
        {
            return Err("Informe horários de início e fim válidos e diferentes.".into());
        }
        let (start, end) = (parse_date(&self.start_date)?, parse_date(&self.end_date)?);
        if start.zip(end).is_some_and(|(a, b)| b < a) {
            return Err("A data de fim deve ser igual ou posterior à data de início.".into());
        }
        Ok(())
    }
    pub fn next_window(&self, now: u64) -> Result<Option<(u64, u64)>, String> {
        self.next_window_in(now, &chrono::Local)
    }
    fn next_window_in<T: TimeZone>(
        &self,
        now: u64,
        zone: &T,
    ) -> Result<Option<(u64, u64)>, String> {
        self.validate()?;
        let timestamp = i64::try_from(now).map_err(|_| "Data inválida.")?;
        let today = zone
            .timestamp_millis_opt(timestamp)
            .single()
            .ok_or("Data inválida.")?
            .date_naive();
        let first = parse_date(&self.start_date)?;
        let last = parse_date(&self.end_date)?;
        let base = first.map_or(today, |date| today.max(date));
        let cutoff = last
            .map(|date| local_minute(zone, date.succ_opt().ok_or("Data inválida.")?, 0, false))
            .transpose()?;
        // A window belongs to its start day, including a window crossing midnight.
        for offset in -1..=7 {
            let day = base
                .checked_add_signed(chrono::Duration::days(offset))
                .ok_or("Data inválida.")?;
            if last.is_some_and(|date| day > date) {
                break;
            }
            if first.is_some_and(|date| day < date) {
                continue;
            }
            if !self.weekdays.contains(&day.weekday().number_from_monday()) {
                continue;
            }
            let end_day = if self.end_minute < self.start_minute {
                day.succ_opt().ok_or("Data inválida.")?
            } else {
                day
            };
            let start = local_minute(zone, day, self.start_minute, false)?;
            let end = local_minute(zone, end_day, self.end_minute, true)?;
            let end = cutoff.map_or(end, |limit| end.min(limit));
            if end > now && end > start {
                return Ok(Some((start, end)));
            }
        }
        Ok(None)
    }
}
fn parse_date(value: &Option<String>) -> Result<Option<chrono::NaiveDate>, String> {
    value
        .as_ref()
        .map(|value| {
            let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| "Informe uma data válida.")?;
            if !(1970..=9999).contains(&date.year())
                || date.format("%Y-%m-%d").to_string() != *value
            {
                return Err("Informe uma data válida.".into());
            }
            Ok(date)
        })
        .transpose()
}
fn local_minute<T: TimeZone>(
    zone: &T,
    day: chrono::NaiveDate,
    minute: u32,
    end: bool,
) -> Result<u64, String> {
    let local = day
        .and_hms_opt(minute / 60, minute % 60, 0)
        .ok_or("Horário inválido.")?;
    // Handle repeated clock times and advance past a daylight-saving gap.
    for offset in 0..=180 {
        let local = local
            .checked_add_signed(chrono::Duration::minutes(offset))
            .ok_or("Data inválida.")?;
        let result = zone.from_local_datetime(&local);
        let chosen = if end {
            result.latest()
        } else {
            result.earliest()
        };
        if let Some(time) = chosen {
            return u64::try_from(time.timestamp_millis()).map_err(|_| "Data inválida.".into());
        }
    }
    Err("Horário indisponível no fuso deste Mac.".into())
}
#[derive(Debug, PartialEq)]
pub enum Next {
    Expired,
    Wait(u64),
    Start,
    Continue,
}
impl RepeatOptions {
    pub fn resolve(&self, now: u64) -> Result<RepeatState, String> {
        let weekly = match &self.schedule {
            Schedule::Weekly {
                weekdays,
                start_minute,
                end_minute,
                start_date,
                end_date,
            } => Some(WeeklySchedule {
                weekdays: weekdays.clone(),
                start_minute: *start_minute,
                end_minute: *end_minute,
                start_date: start_date.clone(),
                end_date: end_date.clone(),
            }),
            _ => None,
        };
        let (start, end) = match &self.schedule {
            Schedule::Duration { minutes } if (1..=10080).contains(minutes) => {
                (now, now.saturating_add(*minutes as u64 * 60_000))
            }
            Schedule::Window { starts_at, ends_at } => (*starts_at, *ends_at),
            Schedule::Weekly { .. } => weekly
                .as_ref()
                .unwrap()
                .next_window(now)?
                .ok_or("Não há dia e horário disponíveis no período escolhido.")?,
            _ => return Err("Escolha uma duração de 1 minuto a 7 dias.".into()),
        };
        if end <= start
            || end <= now
            || end - start > MAX_PERIOD
            || (weekly.is_none() && start > now.saturating_add(MAX_PERIOD))
        {
            return Err("Escolha um período futuro válido, de até 7 dias.".into());
        }
        if !(1..=3600).contains(&self.interval_seconds) {
            return Err("O intervalo entre ciclos deve ser de 1 a 3600 segundos.".into());
        }
        Ok(RepeatState {
            weekly,
            starts_at: start,
            ends_at: end,
            interval_seconds: self.interval_seconds,
            cycle: 0,
            completed_cycles: 0,
            total_actions: 0,
            between_cycles: true,
            next_cycle_at: start,
        })
    }
}
impl RepeatState {
    pub fn advance_weekly_window(&mut self, now: u64) -> Result<bool, String> {
        let Some(weekly) = &self.weekly else {
            return Ok(false);
        };
        let Some((start, end)) = weekly.next_window(now.max(self.ends_at))? else {
            return Ok(false);
        };
        self.starts_at = start;
        self.ends_at = end;
        self.next_cycle_at = start;
        self.between_cycles = true;
        Ok(true)
    }

    pub fn next(&self, now: u64) -> Next {
        if now >= self.ends_at {
            Next::Expired
        } else if self.between_cycles && now < self.next_cycle_at {
            Next::Wait(self.next_cycle_at.min(self.ends_at))
        } else if self.between_cycles {
            Next::Start
        } else {
            Next::Continue
        }
    }
    pub fn complete_cycle(&mut self, now: u64) {
        self.completed_cycles += 1;
        self.between_cycles = true;
        self.next_cycle_at = now.saturating_add(self.interval_seconds as u64 * 1000);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn at(offset: i64, hour: u32) -> u64 {
        let day =
            chrono::NaiveDate::from_ymd_opt(2026, 9, 6).unwrap() + chrono::Duration::days(offset);
        chrono::FixedOffset::west_opt(3 * 3600)
            .unwrap()
            .from_local_datetime(&day.and_hms_opt(hour, 0, 0).unwrap())
            .unwrap()
            .timestamp_millis() as u64
    }
    #[test]
    fn weekly_selection_skips_unselected_days_and_wraps_the_week() {
        let zone = chrono::FixedOffset::west_opt(3 * 3600).unwrap();
        let week = WeeklySchedule {
            weekdays: vec![1, 2, 3, 4, 5],
            start_minute: 540,
            end_minute: 1080,
            start_date: None,
            end_date: None,
        };
        assert_eq!(
            week.next_window_in(at(0, 14), &zone).unwrap().unwrap(),
            (at(1, 9), at(1, 18))
        );
        assert_eq!(
            week.next_window_in(at(5, 19), &zone).unwrap().unwrap(),
            (at(8, 9), at(8, 18))
        );
        let sunday = WeeklySchedule {
            weekdays: vec![7],
            ..week.clone()
        };
        assert_eq!(
            sunday.next_window_in(at(0, 10), &zone).unwrap().unwrap(),
            (at(0, 9), at(0, 18))
        );
        assert_eq!(
            sunday.next_window_in(at(0, 19), &zone).unwrap().unwrap(),
            (at(7, 9), at(7, 18))
        );
    }
    #[test]
    fn overnight_window_belongs_to_the_selected_start_day() {
        let zone = chrono::FixedOffset::west_opt(3 * 3600).unwrap();
        let week = WeeklySchedule {
            weekdays: vec![1],
            start_minute: 1320,
            end_minute: 120,
            start_date: None,
            end_date: None,
        };
        assert_eq!(
            week.next_window_in(at(2, 1), &zone).unwrap().unwrap(),
            (at(1, 22), at(2, 2))
        );
        assert_eq!(
            week.next_window_in(at(2, 3), &zone).unwrap().unwrap(),
            (at(8, 22), at(9, 2))
        );
    }
    #[test]
    fn weekly_state_survives_reload_and_invalid_days_are_rejected() {
        let options: RepeatOptions = serde_json::from_str(r#"{"schedule":{"mode":"weekly","weekdays":[1,3,5],"startMinute":540,"endMinute":1080},"intervalSeconds":5}"#).unwrap();
        let mut state = options.resolve(crate::model::now()).unwrap();
        let old_end = state.ends_at;
        state.cycle = 3;
        state.completed_cycles = 2;
        let mut restored: RepeatState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert!(restored.advance_weekly_window(old_end).unwrap());
        assert!(restored.starts_at >= old_end);
        assert_eq!(restored.completed_cycles, 2);
        assert!(restored.between_cycles);
        assert_eq!(restored.weekly.unwrap().weekdays, vec![1, 3, 5]);
        for days in [vec![], vec![0], vec![8]] {
            assert!(WeeklySchedule {
                weekdays: days,
                start_minute: 540,
                end_minute: 1080,
                start_date: None,
                end_date: None,
            }
            .validate()
            .is_err());
        }
    }
    #[test]
    fn dates_bound_windows_and_clip_the_last_overnight_window() {
        let zone = chrono::FixedOffset::west_opt(3 * 3600).unwrap();
        let mut week: WeeklySchedule = serde_json::from_str(r#"{"weekdays":[1],"startMinute":1320,"endMinute":120,"startDate":"2026-09-07","endDate":"2026-09-07"}"#).unwrap();
        assert_eq!(
            week.next_window_in(at(1, 23), &zone).unwrap(),
            Some((at(1, 22), at(2, 0)))
        );
        assert_eq!(week.next_window_in(at(2, 0), &zone).unwrap(), None);
        week.start_date = Some("2026-09-08".into());
        week.end_date = None;
        week.weekdays = vec![1, 2];
        assert_eq!(
            week.next_window_in(at(2, 1), &zone).unwrap(),
            Some((at(2, 22), at(3, 2)))
        );
        week.start_date = Some("2026-10-05".into());
        week.end_date = Some("2026-10-31".into());
        assert_eq!(
            week.next_window_in(at(0, 14), &zone).unwrap(),
            Some((at(29, 22), at(30, 2)))
        );
    }
    #[test]
    fn future_dates_survive_reload_and_expire_after_pause_without_extending_range() {
        let options: RepeatOptions = serde_json::from_str(r#"{"schedule":{"mode":"weekly","weekdays":[1],"startMinute":540,"endMinute":1080,"startDate":"2026-10-05","endDate":"2026-10-05"},"intervalSeconds":5}"#).unwrap();
        let state = options.resolve(at(0, 14)).unwrap();
        assert!(state.starts_at > at(0, 14) + MAX_PERIOD);
        let mut restored: RepeatState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(
            restored.weekly.as_ref().unwrap().start_date.as_deref(),
            Some("2026-10-05")
        );
        assert_eq!(
            restored.weekly.as_ref().unwrap().end_date.as_deref(),
            Some("2026-10-05")
        );
        assert!(!restored.advance_weekly_window(restored.ends_at).unwrap());
        assert!(!restored.advance_weekly_window(at(40, 14)).unwrap());
        assert!(options.resolve(at(40, 14)).is_err());
    }
    #[test]
    fn invalid_dates_and_ranges_without_selected_days_are_rejected() {
        let zone = chrono::FixedOffset::west_opt(3 * 3600).unwrap();
        let mut week: WeeklySchedule =
            serde_json::from_str(r#"{"weekdays":[1],"startMinute":540,"endMinute":1080}"#).unwrap();
        assert!(week.start_date.is_none() && week.end_date.is_none());
        for value in ["2026-02-30", "2026-9-07", "2026-13-01", "", "1969-12-31"] {
            week.start_date = Some(value.into());
            assert!(week.validate().is_err());
        }
        week.start_date = Some("2026-09-08".into());
        week.end_date = Some("2026-09-07".into());
        assert!(week.validate().is_err());
        week.end_date = Some("2026-09-09".into());
        assert_eq!(week.next_window_in(at(0, 14), &zone).unwrap(), None);
    }
    #[test]
    fn duration_and_window_are_bounded() {
        let options = RepeatOptions {
            schedule: Schedule::Duration { minutes: 30 },
            interval_seconds: 5,
        };
        assert_eq!(options.resolve(1000).unwrap().ends_at, 1_801_000);
        for (start, end) in [(1000, 1000), (500, 900), (1000, u64::MAX)] {
            assert!(RepeatOptions {
                schedule: Schedule::Window {
                    starts_at: start,
                    ends_at: end
                },
                interval_seconds: 5
            }
            .resolve(1000)
            .is_err());
        }
        assert!(RepeatOptions {
            schedule: Schedule::Duration { minutes: 0 },
            interval_seconds: 5
        }
        .resolve(1000)
        .is_err());
    }
    #[test]
    fn interval_and_deadline_survive_pause_and_serialization() {
        let mut state = RepeatOptions {
            schedule: Schedule::Window {
                starts_at: 2000,
                ends_at: 10000,
            },
            interval_seconds: 3,
        }
        .resolve(1000)
        .unwrap();
        assert_eq!(state.next(1000), Next::Wait(2000));
        assert_eq!(state.next(2000), Next::Start);
        state.cycle = 1;
        state.between_cycles = false;
        assert_eq!(state.next(3000), Next::Continue);
        state.complete_cycle(4000);
        let restored: RepeatState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(restored.next(5000), Next::Wait(7000));
        assert_eq!(restored.next(7000), Next::Start);
        assert_eq!(restored.next(10000), Next::Expired);
        assert_eq!(restored.completed_cycles, 1);
    }
    #[test]
    fn expiry_wins_over_the_next_cycle() {
        let mut state = RepeatOptions {
            schedule: Schedule::Duration { minutes: 1 },
            interval_seconds: 60,
        }
        .resolve(0)
        .unwrap();
        state.complete_cycle(50000);
        assert_eq!(state.next(55000), Next::Wait(60000));
        assert_eq!(state.next(60000), Next::Expired);
    }
}
