use crate::runtime::{
    calculate_totals, inactivity_requirement_seconds, period_label, period_start, users_in_any_role,
};
use crate::state::{
    ActiveVoiceSession, CompletedVoiceSession, InactiveEntry, LeaderboardEntry, StoredVoiceUser,
    VoiceMemberForReport,
};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

pub const LEGACY_EMBED_COLOR: u32 = 0x5865F2;
pub const LEGACY_FOOTER: &str = "XIII · Voice Activity";
pub const PERIOD_SELECT_PLACEHOLDER: &str = "Выберите период";
pub const PREVIOUS_LABEL: &str = "←";
pub const NEXT_LABEL: &str = "→";
pub const PUBLIC_STATS_PREVIEW_DESCRIPTION: &str = "Период: {label} · Страница N/M";
pub const INACTIVE_PREVIEW_DESCRIPTION: &str = "Период: {label}";
pub const PANEL_REFRESH_NOTICE: &str =
    "Панель автоматически обновляется из сохранённой голосовой статистики.";
pub const PUBLIC_EMPTY_MESSAGE: &str = "Нет доступных участников для отображения.";
pub const INACTIVE_EMPTY_MESSAGE: &str = "Нет участников с заданной ролью.";

pub fn voice_top_disabled_response(panel_channel_id: u64) -> String {
    format!(
        "Функционал этой команды отключён.\nГолосовую статистику теперь можно посмотреть в постоянной панели: <#{panel_channel_id}>"
    )
}

pub fn top_users_by_duration(sessions: &[CompletedVoiceSession]) -> Vec<(u64, i64)> {
    let mut totals = std::collections::BTreeMap::<u64, i64>::new();
    for session in sessions {
        *totals.entry(session.user_id).or_default() += session.duration_seconds;
    }
    let mut rows = totals.into_iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    rows
}

pub fn leaderboard_entries(
    users: &[StoredVoiceUser],
    completed: &[CompletedVoiceSession],
    active: &[ActiveVoiceSession],
    period_key: &str,
    page: usize,
    page_size: usize,
    now: DateTime<Utc>,
) -> Vec<LeaderboardEntry> {
    let names = users
        .iter()
        .map(|user| (user.user_id, user.display_name.clone()))
        .collect::<BTreeMap<_, _>>();
    let totals = calculate_totals(completed, active, period_start(period_key, now), now);
    let mut rows = totals.into_iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| display_name(&names, left.0).cmp(&display_name(&names, right.0)))
            .then_with(|| left.0.cmp(&right.0))
    });
    rows.into_iter()
        .enumerate()
        .skip(page.saturating_mul(page_size))
        .take(page_size)
        .map(|(index, (user_id, total_seconds))| LeaderboardEntry {
            rank: index + 1,
            user_id,
            display_name: display_name(&names, user_id),
            total_seconds,
            points: total_seconds / 3600,
        })
        .collect()
}

pub fn leaderboard_total_pages(
    completed: &[CompletedVoiceSession],
    active: &[ActiveVoiceSession],
    period_key: &str,
    page_size: usize,
    now: DateTime<Utc>,
) -> usize {
    let count = calculate_totals(completed, active, period_start(period_key, now), now).len();
    let page_size = page_size.max(1);
    count.max(1).div_ceil(page_size)
}

pub fn inactive_total_pages(
    members: &[VoiceMemberForReport],
    completed: &[CompletedVoiceSession],
    active: &[ActiveVoiceSession],
    inactive_role_id: u64,
    vacation_marker_role_id: u64,
    period_key: &str,
    page_size: usize,
    now: DateTime<Utc>,
) -> usize {
    let count = inactive_entries(
        members,
        completed,
        active,
        inactive_role_id,
        vacation_marker_role_id,
        period_key,
        0,
        usize::MAX,
        now,
    )
    .len();
    let page_size = page_size.max(1);
    count.max(1).div_ceil(page_size)
}

pub fn inactive_entries(
    members: &[VoiceMemberForReport],
    completed: &[CompletedVoiceSession],
    active: &[ActiveVoiceSession],
    inactive_role_id: u64,
    vacation_marker_role_id: u64,
    period_key: &str,
    page: usize,
    page_size: usize,
    now: DateTime<Utc>,
) -> Vec<InactiveEntry> {
    let totals = calculate_totals(completed, active, period_start(period_key, now), now);
    let required_seconds = inactivity_requirement_seconds(period_key);
    let mut rows = users_in_any_role(members, inactive_role_id)
        .into_iter()
        .map(|member| {
            let total_seconds = totals.get(&member.user_id).copied().unwrap_or(0);
            InactiveEntry {
                rank: 0,
                user_id: member.user_id,
                display_name: member.display_name.clone(),
                total_seconds,
                required_seconds,
                passed: total_seconds >= required_seconds,
                on_vacation: member.role_ids.contains(&vacation_marker_role_id),
            }
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.passed
            .cmp(&right.passed)
            .then_with(|| left.total_seconds.cmp(&right.total_seconds))
            .then_with(|| left.display_name.cmp(&right.display_name))
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
    rows.into_iter()
        .enumerate()
        .skip(page.saturating_mul(page_size))
        .take(page_size)
        .map(|(index, mut entry)| {
            entry.rank = index + 1;
            entry
        })
        .collect()
}

pub fn render_leaderboard_description(
    period_key: &str,
    page: usize,
    total_pages: usize,
    entries: &[LeaderboardEntry],
) -> String {
    let mut lines = vec![format!(
        "Период: {} · Страница {}/{}",
        period_label(period_key),
        page + 1,
        total_pages.max(1)
    )];
    lines.push(PANEL_REFRESH_NOTICE.to_owned());
    lines.push(String::new());
    if entries.is_empty() {
        lines.push(PUBLIC_EMPTY_MESSAGE.to_owned());
    } else {
        for entry in entries {
            lines.push(format!(
                "{} — {} · {}",
                leaderboard_prefix(entry.rank, &escape_markdown(&entry.display_name)),
                format_duration_compact(entry.total_seconds),
                format_points(entry.points)
            ));
        }
    }
    lines.join("\n")
}

pub fn render_inactive_description(period_key: &str, entries: &[InactiveEntry]) -> String {
    let mut lines = vec![
        format!("Период: {}", inactive_period_label(period_key)),
        String::new(),
    ];
    if entries.is_empty() {
        lines.push(INACTIVE_EMPTY_MESSAGE.to_owned());
    } else {
        for entry in entries {
            let status = if entry.passed { "✅" } else { "❌" };
            let vacation = if entry.on_vacation {
                " (отпуск)"
            } else {
                ""
            };
            lines.push(format!(
                "{}. {} {}{} — {} · {}",
                entry.rank,
                status,
                escape_markdown(&entry.display_name),
                vacation,
                format_duration_compact(entry.total_seconds),
                format_points(entry.total_seconds / 3600)
            ));
        }
    }
    lines.join("\n")
}

pub fn format_duration_compact(total_seconds: i64) -> String {
    let total_minutes = total_seconds.max(0) / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{hours}ч {minutes:02}м")
}

pub fn format_points(points: i64) -> String {
    let remainder_10 = points.rem_euclid(10);
    let remainder_100 = points.rem_euclid(100);
    let suffix = if remainder_10 == 1 && remainder_100 != 11 {
        "балл"
    } else if [2, 3, 4].contains(&remainder_10) && ![12, 13, 14].contains(&remainder_100) {
        "балла"
    } else {
        "баллов"
    };
    format!("{points} {suffix}")
}

pub fn inactive_period_label(period_key: &str) -> &'static str {
    match period_key {
        "7d" => "7 дней / 10ч",
        "14d" => "14 дней / 20ч",
        "30d" => "30 дней / 40ч",
        "60d" => "60 дней / 80ч",
        _ => "7 дней / 10ч",
    }
}

fn leaderboard_prefix(rank: usize, display_name: &str) -> String {
    match rank {
        1 => format!("🥇 {display_name}"),
        2 => format!("🥈 {display_name}"),
        3 => format!("🥉 {display_name}"),
        _ => format!("{rank}. {display_name}"),
    }
}

fn display_name(names: &BTreeMap<u64, String>, user_id: u64) -> String {
    names
        .get(&user_id)
        .cloned()
        .unwrap_or_else(|| user_id.to_string())
}

fn escape_markdown(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' | '*' | '_' | '~' | '`' | '|' | '>' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}
