use crate::state::{
    ActiveVoiceSession, CompletedVoiceSession, LiveVoiceMember, StoredVoiceUser, VoiceSession,
};
use chrono::{DateTime, Duration, Utc};
use std::collections::{BTreeMap, BTreeSet};

pub fn should_track_channel(channel_id: u64, ignored_channel_ids: &[u64]) -> bool {
    !ignored_channel_ids.contains(&channel_id)
}

pub fn close_session(session: &VoiceSession, ended_unix: i64) -> Option<CompletedVoiceSession> {
    let duration = ended_unix - session.started_unix;
    (duration > 0).then_some(CompletedVoiceSession {
        id: None,
        guild_id: 0,
        user_id: session.user_id,
        channel_id: session.channel_id,
        started_at: session.started_unix.to_string(),
        ended_at: ended_unix.to_string(),
        duration_seconds: duration,
        close_reason: "normal".to_owned(),
    })
}

pub fn sum_duration_seconds(sessions: &[CompletedVoiceSession]) -> i64 {
    sessions
        .iter()
        .map(|session| session.duration_seconds)
        .sum()
}

pub fn active_session_cutover_warning(active_count: usize) -> Option<String> {
    (active_count > 0).then(|| {
        format!(
            "{active_count} active voice sessions require explicit cutover; do not enable writer blindly"
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceTrackingAction {
    Ignored,
    Join {
        channel_id: u64,
    },
    Leave {
        channel_id: u64,
    },
    Move {
        from_channel_id: u64,
        to_channel_id: u64,
    },
    Stay {
        channel_id: u64,
    },
}

pub fn classify_voice_update(
    previous_channel_id: Option<u64>,
    current_channel_id: Option<u64>,
    ignored_channel_ids: &[u64],
) -> VoiceTrackingAction {
    let previous = previous_channel_id.filter(|id| should_track_channel(*id, ignored_channel_ids));
    let current = current_channel_id.filter(|id| should_track_channel(*id, ignored_channel_ids));
    match (previous, current) {
        (None, None) => VoiceTrackingAction::Ignored,
        (None, Some(channel_id)) => VoiceTrackingAction::Join { channel_id },
        (Some(channel_id), None) => VoiceTrackingAction::Leave { channel_id },
        (Some(from_channel_id), Some(to_channel_id)) if from_channel_id != to_channel_id => {
            VoiceTrackingAction::Move {
                from_channel_id,
                to_channel_id,
            }
        }
        (Some(channel_id), Some(_)) => VoiceTrackingAction::Stay { channel_id },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileAction {
    CloseStale {
        guild_id: u64,
        user_id: u64,
        ended_at: String,
        reason: String,
    },
    UpdateChannel {
        guild_id: u64,
        user_id: u64,
        channel_id: u64,
        last_seen_at: String,
    },
    Touch {
        guild_id: u64,
        user_id: u64,
        last_seen_at: String,
    },
    OpenRecovered {
        session: ActiveVoiceSession,
        user: StoredVoiceUser,
    },
}

pub fn reconcile_actions(
    guild_id: u64,
    db_sessions: &[ActiveVoiceSession],
    live_members: &[LiveVoiceMember],
    ignored_channel_ids: &[u64],
    now: DateTime<Utc>,
) -> Vec<ReconcileAction> {
    let now_iso = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let live_by_user = live_members
        .iter()
        .filter(|member| !member.is_bot)
        .filter(|member| should_track_channel(member.channel_id, ignored_channel_ids))
        .map(|member| (member.user_id, member))
        .collect::<BTreeMap<_, _>>();
    let active_by_user = db_sessions
        .iter()
        .map(|session| (session.user_id, session))
        .collect::<BTreeMap<_, _>>();

    let mut actions = Vec::new();
    for session in db_sessions {
        match live_by_user.get(&session.user_id).copied() {
            None => actions.push(ReconcileAction::CloseStale {
                guild_id: session.guild_id,
                user_id: session.user_id,
                ended_at: session.last_seen_at.clone(),
                reason: "startup_last_seen".to_owned(),
            }),
            Some(live) if live.channel_id != session.channel_id => {
                actions.push(ReconcileAction::UpdateChannel {
                    guild_id: session.guild_id,
                    user_id: session.user_id,
                    channel_id: live.channel_id,
                    last_seen_at: now_iso.clone(),
                });
            }
            Some(_) => actions.push(ReconcileAction::Touch {
                guild_id: session.guild_id,
                user_id: session.user_id,
                last_seen_at: now_iso.clone(),
            }),
        }
    }

    for live in live_by_user.values() {
        if active_by_user.contains_key(&live.user_id) {
            continue;
        }
        actions.push(ReconcileAction::OpenRecovered {
            session: ActiveVoiceSession {
                guild_id,
                user_id: live.user_id,
                channel_id: live.channel_id,
                started_at: now_iso.clone(),
                last_seen_at: now_iso.clone(),
                recovered: true,
            },
            user: StoredVoiceUser {
                user_id: live.user_id,
                display_name: live.display_name.clone(),
                username: live.username.clone(),
                last_seen_at: now_iso.clone(),
            },
        });
    }

    actions
}

pub fn period_start(period_key: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match period_key {
        "7d" => Some(now - Duration::days(7)),
        "14d" => Some(now - Duration::days(14)),
        "30d" => Some(now - Duration::days(30)),
        "60d" => Some(now - Duration::days(60)),
        "all" => None,
        _ => Some(now - Duration::days(7)),
    }
}

pub fn period_label(period_key: &str) -> &'static str {
    match period_key {
        "7d" => "7 дней",
        "14d" => "2 недели",
        "30d" => "30 дней",
        "60d" => "60 дней",
        "all" => "Всё время",
        _ => "7 дней",
    }
}

pub fn inactivity_requirement_seconds(period_key: &str) -> i64 {
    match period_key {
        "7d" => 10 * 3600,
        "14d" => 20 * 3600,
        "30d" => 40 * 3600,
        "60d" => 80 * 3600,
        _ => 10 * 3600,
    }
}

pub fn calculate_totals(
    completed: &[CompletedVoiceSession],
    active: &[ActiveVoiceSession],
    period_start: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> BTreeMap<u64, i64> {
    let mut totals = BTreeMap::new();
    for session in completed {
        let Some(started_at) = parse_rfc3339_utc(&session.started_at) else {
            continue;
        };
        let Some(ended_at) = parse_rfc3339_utc(&session.ended_at) else {
            continue;
        };
        let overlap = overlap_seconds(started_at, ended_at, period_start, now);
        if overlap > 0 {
            *totals.entry(session.user_id).or_default() += overlap;
        }
    }
    for session in active {
        let Some(started_at) = parse_rfc3339_utc(&session.started_at) else {
            continue;
        };
        let overlap = overlap_seconds(started_at, now, period_start, now);
        if overlap > 0 {
            *totals.entry(session.user_id).or_default() += overlap;
        }
    }
    totals
}

pub fn parse_page_custom_id(custom_id: &str, prefix: &str) -> Option<(String, i64)> {
    if custom_id == format!("{prefix}:previous") {
        return Some(("7d".to_owned(), -1));
    }
    if custom_id == format!("{prefix}:next") {
        return Some(("7d".to_owned(), 1));
    }
    custom_id
        .strip_prefix(&format!("{prefix}:period:"))
        .map(|period| (period.to_owned(), 0))
}

pub fn users_in_any_role<'a>(
    members: &'a [crate::state::VoiceMemberForReport],
    role_id: u64,
) -> Vec<&'a crate::state::VoiceMemberForReport> {
    let mut rows = members
        .iter()
        .filter(|member| !member.is_bot && member.role_ids.contains(&role_id))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.display_name
            .to_ascii_lowercase()
            .cmp(&right.display_name.to_ascii_lowercase())
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
    rows
}

pub fn member_ids_from_active(active: &[ActiveVoiceSession]) -> BTreeSet<u64> {
    active.iter().map(|session| session.user_id).collect()
}

fn overlap_seconds(
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    period_start: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> i64 {
    let start = period_start
        .map(|period| started_at.max(period))
        .unwrap_or(started_at);
    let end = ended_at.min(now);
    (end - start).num_seconds().max(0)
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}
