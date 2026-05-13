use crate::discord_io::{accept_transition, decision_dm_content, reject_transition};
use crate::repository::{LegacySqliteRecruitRepository, RecruitRecord};
use crate::runtime::{
    due_recruits, next_status, should_ping_decision_roles, should_send_due_panel,
    voice_channel_is_tracked, voice_duration_seconds,
};
use crate::state::{
    Recruit, RecruitDecision, RecruitDecisionPanel, RecruitStatus, RecruitVoiceSession,
};

fn active_recruit(due_unix: i64) -> Recruit {
    Recruit {
        id: 1,
        guild_id: 1,
        user_id: 973660882242519150,
        status: RecruitStatus::Active,
        due_unix,
        last_decision_message_id: Some(1501259037357117641),
        last_decision_channel_id: Some(1500136438791147651),
    }
}

#[test]
fn due_checker_finds_active_due_recruits_only() {
    let recruits = vec![
        active_recruit(10),
        Recruit {
            status: RecruitStatus::Accepted,
            ..active_recruit(5)
        },
    ];

    assert_eq!(due_recruits(&recruits, 10).len(), 1);
}

#[test]
fn automatic_due_panels_ping_but_manual_panels_do_not() {
    assert!(should_ping_decision_roles(true));
    assert!(!should_ping_decision_roles(false));
}

#[test]
fn excluded_voice_channel_is_not_tracked() {
    assert!(!voice_channel_is_tracked(
        1498022116682104914,
        Some(1498022116682104914)
    ));
    assert!(voice_channel_is_tracked(1, Some(1498022116682104914)));
}

#[test]
fn accept_and_reject_role_transitions_are_preserved() {
    let accept = accept_transition(42, 10, 20);
    assert_eq!(accept.remove_role_ids, vec![10]);
    assert_eq!(accept.add_role_ids, vec![20]);

    let reject = reject_transition(42, 10, 11, 30);
    assert_eq!(reject.remove_role_ids, vec![10, 11]);
    assert_eq!(reject.add_role_ids, vec![30]);
}

#[test]
fn decisions_map_to_expected_statuses() {
    assert_eq!(
        next_status(&RecruitDecision::Accept),
        RecruitStatus::Accepted
    );
    assert_eq!(
        next_status(&RecruitDecision::Reject {
            reason: "no".to_owned()
        }),
        RecruitStatus::Rejected
    );
}

#[test]
fn due_panel_is_idempotent_for_automatic_messages() {
    let recruit = active_recruit(10);

    assert!(should_send_due_panel(&recruit, &[], 10));
    assert!(!should_send_due_panel(
        &recruit,
        &[RecruitDecisionPanel {
            recruit_id: recruit.id,
            channel_id: 1,
            message_id: 2,
            automatic: true,
        }],
        10
    ));
}

#[test]
fn voice_duration_accumulates_positive_time_only() {
    let session = RecruitVoiceSession {
        recruit_id: 1,
        user_id: 42,
        channel_id: 5,
        joined_unix: 100,
    };

    assert_eq!(voice_duration_seconds(&session, 160), 60);
    assert_eq!(voice_duration_seconds(&session, 90), 0);
}

#[test]
fn decision_dm_is_user_scoped() {
    let dm = decision_dm_content(42, "Accepted");

    assert_eq!(dm.user_id, 42);
    assert_eq!(dm.content.as_deref(), Some("Accepted"));
    assert!(dm.embed.is_none());
}

#[test]
fn recruit_visual_constants_match_legacy_source() {
    assert_eq!(crate::render::LEGACY_DECISION_COLOR, 0xF1C40F);
    assert_eq!(
        crate::render::decision_panel_title(&active_recruit(10)),
        "🎖️ Решение по стажировке"
    );
    assert_eq!(crate::render::ACCEPT_BUTTON_LABEL, "✅ Принять");
    assert_eq!(crate::render::REJECT_BUTTON_LABEL, "❌ Отклонить");
    assert_eq!(crate::render::EXTEND_BUTTON_LABEL, "⏳ Продлить стажировку");
    assert_eq!(crate::render::REJECT_MODAL_TITLE, "Отклонение стажировки");
    assert_eq!(crate::render::EXTEND_MODAL_TITLE, "Продление стажировки");
    assert_eq!(
        crate::render::ACCEPT_SUCCESS,
        "✅ Рекрут принят в основной состав."
    );
    assert_eq!(crate::render::REJECT_SUCCESS, "✅ Стажировка отклонена.");
    assert_eq!(crate::render::EXTEND_SUCCESS, "✅ Стажировка продлена.");
}

#[test]
fn recruit_decision_and_dm_embeds_match_legacy_copy() {
    let recruit = RecruitRecord {
        id: 7,
        guild_id: 1,
        user_id: 42,
        status: "active".to_owned(),
        started_at: "2026-05-01T00:00:00Z".to_owned(),
        due_at: "2026-05-15T00:00:00Z".to_owned(),
        completed_at: None,
        extensions_count: 2,
        last_decision_message_id: Some(10),
        last_decision_channel_id: Some(11),
        created_at: "2026-05-01T00:00:00Z".to_owned(),
        updated_at: "2026-05-01T00:00:00Z".to_owned(),
    };

    let panel = crate::render::decision_panel_embed(&recruit, 3660);
    assert_eq!(panel.title, "🎖️ Решение по стажировке");
    assert!(panel
        .description
        .as_deref()
        .unwrap_or_default()
        .contains("Стажёр: <@42>"));
    assert_eq!(panel.fields[0].name, "Сроки");
    assert!(panel.fields[0]
        .value
        .contains("Начало: 01.05.2026 00:00 UTC"));
    assert_eq!(panel.fields[1].value, "1 ч 1 мин");
    assert_eq!(panel.fields[2].value, "2");
    assert_eq!(panel.footer.as_deref(), Some("ID стажировки: 7"));

    let accepted = crate::render::accepted_dm_embed();
    assert_eq!(accepted.title, crate::render::ACCEPT_DM_TITLE);
    assert!(accepted
        .description
        .as_deref()
        .unwrap_or_default()
        .contains("Поменяй префикс в Squad на ✧︎XIII✧︎"));

    let rejected = crate::render::rejected_dm_embed("Недостаточно активности");
    assert_eq!(rejected.title, crate::render::REJECT_DM_TITLE);
    assert_eq!(rejected.fields[0].name, "Причина");
    assert!(rejected.fields[1].value.contains("не имеешь права играть"));

    let extended = crate::render::extended_dm_embed(3, "Нужно больше времени");
    assert_eq!(extended.title, crate::render::EXTEND_DM_TITLE);
    assert!(extended
        .description
        .as_deref()
        .unwrap_or_default()
        .contains("на 3 дн."));
}

#[tokio::test]
async fn sqlite_decision_completion_preserves_decision_log() {
    let path = temp_db_path("recruit_decision");
    let repo = LegacySqliteRecruitRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    let started = chrono::DateTime::parse_from_rfc3339("2026-05-01T00:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let due = chrono::DateTime::parse_from_rfc3339("2026-05-08T00:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let recruit = repo
        .create_active_recruit(1, 42, started, due)
        .await
        .unwrap();

    assert!(repo
        .complete_with_decision(recruit.id, "accepted", "accepted", 99, None, due)
        .await
        .unwrap());
    assert!(!repo
        .complete_with_decision(recruit.id, "accepted", "accepted", 99, None, due)
        .await
        .unwrap());
    assert!(repo.list_active_recruits(1).await.unwrap().is_empty());
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sqlite_voice_session_open_close_accumulates_duration() {
    let path = temp_db_path("recruit_voice");
    let repo = LegacySqliteRecruitRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    let start = chrono::DateTime::parse_from_rfc3339("2026-05-09T10:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let end = chrono::DateTime::parse_from_rfc3339("2026-05-09T10:05:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let session = repo.open_voice_session(1, 42, 5, start).await.unwrap();
    let duplicate = repo.open_voice_session(1, 42, 5, start).await.unwrap();
    assert_eq!(session.id, duplicate.id);

    let closed = repo
        .close_open_voice_sessions(1, 42, end, false)
        .await
        .unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].duration_seconds, 300);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sqlite_extend_clears_decision_message_for_new_due_panel() {
    let path = temp_db_path("recruit_extend");
    let repo = LegacySqliteRecruitRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    let started = chrono::Utc::now();
    let due = started + chrono::Duration::days(7);
    let new_due = due + chrono::Duration::days(3);
    let recruit = repo
        .create_active_recruit(1, 77, started, due)
        .await
        .unwrap();
    assert!(repo
        .set_decision_message(recruit.id, 10, 11, started)
        .await
        .unwrap());
    assert!(repo
        .extend_with_decision(recruit.id, 99, new_due, "more time", 3, started)
        .await
        .unwrap());
    let updated = repo.get_recruit_by_id(recruit.id).await.unwrap().unwrap();
    assert_eq!(updated.last_decision_message_id, None);
    assert_eq!(updated.extensions_count, 1);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sqlite_voice_seconds_for_recruit_counts_closed_and_open_sessions() {
    let path = temp_db_path("recruit_voice_total");
    let repo = LegacySqliteRecruitRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    let started = chrono::DateTime::parse_from_rfc3339("2026-05-01T00:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let due = chrono::DateTime::parse_from_rfc3339("2026-05-15T00:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let recruit = repo
        .create_active_recruit(1, 42, started, due)
        .await
        .unwrap();
    repo.open_voice_session(
        1,
        42,
        5,
        chrono::DateTime::parse_from_rfc3339("2026-05-02T10:00:00+00:00")
            .unwrap()
            .with_timezone(&chrono::Utc),
    )
    .await
    .unwrap();
    repo.close_open_voice_sessions(
        1,
        42,
        chrono::DateTime::parse_from_rfc3339("2026-05-02T10:30:00+00:00")
            .unwrap()
            .with_timezone(&chrono::Utc),
        false,
    )
    .await
    .unwrap();
    repo.open_voice_session(
        1,
        42,
        6,
        chrono::DateTime::parse_from_rfc3339("2026-05-03T10:00:00+00:00")
            .unwrap()
            .with_timezone(&chrono::Utc),
    )
    .await
    .unwrap();

    let total = repo
        .voice_seconds_for_recruit(
            &recruit,
            chrono::DateTime::parse_from_rfc3339("2026-05-03T10:15:00+00:00")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
        .await
        .unwrap();
    assert_eq!(total, 2700);
    let _ = std::fs::remove_file(path);
}

fn temp_db_path(label: &str) -> std::path::PathBuf {
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
