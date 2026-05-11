use crate::commands::{inactive_check_allowed, member_has_vacation_marker};
use crate::render::{top_users_by_duration, voice_top_disabled_response};
use crate::runtime::{
    active_session_cutover_warning, classify_voice_update, close_session, reconcile_actions,
    should_track_channel, sum_duration_seconds, VoiceTrackingAction,
};
use crate::state::{
    ActiveVoiceSession, CompletedVoiceSession, LiveVoiceMember, VoiceActivityCutoverState,
    VoiceSession,
};
use std::path::PathBuf;

#[test]
fn duration_calculation_closes_positive_sessions_only() {
    let session = VoiceSession {
        user_id: 42,
        channel_id: 10,
        started_unix: 100,
        ended_unix: None,
    };

    assert_eq!(close_session(&session, 160).unwrap().duration_seconds, 60);
    assert!(close_session(&session, 90).is_none());
}

#[test]
fn ignored_channels_are_not_tracked() {
    assert!(!should_track_channel(
        1498022116682104914,
        &[1498022116682104914]
    ));
    assert!(should_track_channel(1, &[1498022116682104914]));
}

#[test]
fn active_session_cutover_warns_when_live_sessions_exist() {
    assert!(active_session_cutover_warning(10)
        .unwrap()
        .contains("10 active"));
    assert!(active_session_cutover_warning(0).is_none());
}

#[test]
fn top_users_sort_by_duration_descending() {
    let sessions = vec![
        CompletedVoiceSession {
            id: None,
            guild_id: 1,
            user_id: 1,
            channel_id: 1,
            started_at: "2026-01-01T00:00:00Z".to_owned(),
            ended_at: "2026-01-01T00:00:10Z".to_owned(),
            duration_seconds: 10,
            close_reason: "normal".to_owned(),
        },
        CompletedVoiceSession {
            id: None,
            guild_id: 1,
            user_id: 2,
            channel_id: 1,
            started_at: "2026-01-01T00:00:00Z".to_owned(),
            ended_at: "2026-01-01T00:00:20Z".to_owned(),
            duration_seconds: 20,
            close_reason: "normal".to_owned(),
        },
        CompletedVoiceSession {
            id: None,
            guild_id: 1,
            user_id: 1,
            channel_id: 2,
            started_at: "2026-01-01T00:00:00Z".to_owned(),
            ended_at: "2026-01-01T00:00:15Z".to_owned(),
            duration_seconds: 15,
            close_reason: "normal".to_owned(),
        },
    ];

    assert_eq!(sum_duration_seconds(&sessions), 45);
    assert_eq!(top_users_by_duration(&sessions)[0], (1, 25));
}

#[test]
fn vacation_marker_and_channel_restriction_are_detected() {
    assert!(member_has_vacation_marker(
        &[1498113605768314921],
        1498113605768314921
    ));
    assert!(inactive_check_allowed(
        1499669325685198888,
        1499669325685198888
    ));
    assert!(voice_top_disabled_response(1500963695327707236).contains("1500963695327707236"));
    assert!(voice_top_disabled_response(1500963695327707236)
        .contains("Функционал этой команды отключён"));
}

#[test]
fn voice_activity_visual_text_matches_legacy_source() {
    assert_eq!(crate::runtime::period_label("7d"), "7 дней");
    assert_eq!(crate::runtime::period_label("14d"), "2 недели");
    assert_eq!(crate::runtime::period_label("30d"), "30 дней");
    assert_eq!(crate::runtime::period_label("all"), "Всё время");
    assert!(
        crate::render::render_leaderboard_description("7d", 0, 1, &[])
            .contains("Нет доступных участников для отображения.")
    );
    assert!(crate::render::render_inactive_description("7d", &[])
        .contains("Нет участников с заданной ролью."));
}

#[test]
fn leaderboard_and_inactive_rows_escape_markdown_names() {
    let leaderboard = crate::render::render_leaderboard_description(
        "7d",
        0,
        2,
        &[crate::state::LeaderboardEntry {
            rank: 4,
            user_id: 10,
            display_name: "name_*_test".to_owned(),
            total_seconds: 7200,
            points: 2,
        }],
    );
    assert!(leaderboard.contains("name\\_\\*\\_test"));

    let inactive = crate::render::render_inactive_description(
        "7d",
        &[crate::state::InactiveEntry {
            rank: 1,
            user_id: 10,
            display_name: "name_*_test".to_owned(),
            total_seconds: 7200,
            required_seconds: 36000,
            passed: false,
            on_vacation: false,
        }],
    );
    assert!(inactive.contains("name\\_\\*\\_test"));
}

#[test]
fn voice_update_classification_respects_ignored_channels() {
    assert_eq!(
        classify_voice_update(None, Some(10), &[99]),
        VoiceTrackingAction::Join { channel_id: 10 }
    );
    assert_eq!(
        classify_voice_update(Some(10), Some(20), &[99]),
        VoiceTrackingAction::Move {
            from_channel_id: 10,
            to_channel_id: 20
        }
    );
    assert_eq!(
        classify_voice_update(Some(10), None, &[99]),
        VoiceTrackingAction::Leave { channel_id: 10 }
    );
    assert_eq!(
        classify_voice_update(None, Some(99), &[99]),
        VoiceTrackingAction::Ignored
    );
}

#[test]
fn startup_reconciliation_closes_stale_and_opens_missing_sessions() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-05-09T10:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let active = vec![
        ActiveVoiceSession {
            guild_id: 1,
            user_id: 10,
            channel_id: 100,
            started_at: "2026-05-09T09:00:00Z".to_owned(),
            last_seen_at: "2026-05-09T09:30:00Z".to_owned(),
            recovered: false,
        },
        ActiveVoiceSession {
            guild_id: 1,
            user_id: 20,
            channel_id: 200,
            started_at: "2026-05-09T09:00:00Z".to_owned(),
            last_seen_at: "2026-05-09T09:30:00Z".to_owned(),
            recovered: false,
        },
    ];
    let live = vec![
        LiveVoiceMember {
            user_id: 20,
            channel_id: 201,
            display_name: "Mover".to_owned(),
            username: Some("mover".to_owned()),
            is_bot: false,
        },
        LiveVoiceMember {
            user_id: 30,
            channel_id: 300,
            display_name: "Recovered".to_owned(),
            username: None,
            is_bot: false,
        },
    ];

    let actions = reconcile_actions(1, &active, &live, &[], now);

    assert!(actions.iter().any(|action| matches!(
        action,
        crate::runtime::ReconcileAction::CloseStale { user_id: 10, .. }
    )));
    assert!(actions.iter().any(|action| matches!(
        action,
        crate::runtime::ReconcileAction::UpdateChannel {
            user_id: 20,
            channel_id: 201,
            ..
        }
    )));
    assert!(actions.iter().any(|action| matches!(
        action,
        crate::runtime::ReconcileAction::OpenRecovered { session, .. } if session.user_id == 30
    )));
}

#[tokio::test]
async fn cutover_closes_active_sessions_once_and_preserves_history() {
    let repo = temp_voice_repo("cutover_once").await;
    repo.create_or_replace_active_session(&ActiveVoiceSession {
        guild_id: 1,
        user_id: 10,
        channel_id: 100,
        started_at: "2026-05-10T10:00:00Z".to_owned(),
        last_seen_at: "2026-05-10T10:10:00Z".to_owned(),
        recovered: false,
    })
    .await
    .unwrap();
    repo.create_or_replace_active_session(&ActiveVoiceSession {
        guild_id: 1,
        user_id: 20,
        channel_id: 200,
        started_at: "2026-05-10T10:30:00Z".to_owned(),
        last_seen_at: "2026-05-10T10:30:00Z".to_owned(),
        recovered: false,
    })
    .await
    .unwrap();

    let first = repo
        .close_all_active_sessions_at_cutover(1, "2026-05-10T11:00:00Z")
        .await
        .unwrap();
    assert_eq!(first.closed_sessions.len(), 2);
    assert!(first
        .closed_sessions
        .iter()
        .all(|row| row.completed_row_inserted));
    assert_eq!(first.closed_sessions[0].ended_at, "2026-05-10T11:00:00Z");
    assert_eq!(first.closed_sessions[1].ended_at, "2026-05-10T11:00:00Z");
    let second = repo
        .close_all_active_sessions_at_cutover(1, "2026-05-10T11:00:00Z")
        .await
        .unwrap();
    assert!(second.closed_sessions.is_empty());

    let active = repo.list_active_sessions(1).await.unwrap();
    assert!(active.is_empty());
    let completed = repo
        .fetch_completed_sessions_since(1, Some("2026-05-01T00:00:00Z"))
        .await
        .unwrap();
    assert_eq!(completed.len(), 2);
    assert!(completed.iter().any(|row| row.user_id == 10));
    assert!(completed.iter().any(|row| row.user_id == 20));
}

#[tokio::test]
async fn cutover_clamps_negative_duration_and_state_json_is_safe() {
    let repo = temp_voice_repo("cutover_negative").await;
    repo.create_or_replace_active_session(&ActiveVoiceSession {
        guild_id: 1,
        user_id: 10,
        channel_id: 100,
        started_at: "2026-05-10T12:00:00Z".to_owned(),
        last_seen_at: "2026-05-10T12:00:00Z".to_owned(),
        recovered: false,
    })
    .await
    .unwrap();

    let closed = repo
        .close_all_active_sessions_at_cutover(1, "2026-05-10T11:00:00Z")
        .await
        .unwrap();
    assert_eq!(closed.closed_sessions.len(), 1);
    assert_eq!(closed.closed_sessions[0].duration_seconds, 0);

    let state = VoiceActivityCutoverState {
        source: "voice_finalize_cutover".to_owned(),
        policy: "closed_active_at_cutover".to_owned(),
        guild_id: 1,
        cutover_at_utc: "2026-05-10T11:00:00Z".to_owned(),
        active_sessions_before: 1,
        closed_sessions: closed.closed_sessions.clone(),
        note: "DISCORD_TOKEN should never appear here".to_owned(),
    };
    let json = serde_json::to_string(&state).unwrap();
    assert!(json.contains("closed_active_at_cutover"));
    assert!(!json.contains("PRIVATE_KEY"));
}

fn temp_db_path(label: &str) -> PathBuf {
    let unique = format!(
        "{}_{}_{}.sqlite",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}

async fn temp_voice_repo(label: &str) -> crate::repository::LegacySqliteVoiceActivityRepository {
    let path = temp_db_path(label);
    let repo =
        crate::repository::LegacySqliteVoiceActivityRepository::open_writable_for_tests(&path)
            .await
            .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    repo
}
