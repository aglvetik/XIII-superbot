use crate::commands::{can_moderate, valid_target, DisciplinePermission};
use crate::discord_io::{clan_removal_request, punishment_dm};
use crate::repository::{ActionLogDraft, IssuePunishmentDraft, LegacySqliteDisciplineRepository};
use crate::runtime::{escalation_for_new_punishment, expires_after_days};
use crate::state::{
    action_lock_allows, action_lock_key, ActionLock, EscalationOutcome, Punishment,
    PunishmentStatus, PunishmentType,
};

fn punishment(kind: PunishmentType) -> Punishment {
    Punishment {
        id: 1,
        user_id: 42,
        kind,
        status: PunishmentStatus::Active,
    }
}

#[test]
fn warning_verbal_strict_escalation_matrix_matches_policy() {
    assert_eq!(
        escalation_for_new_punishment(
            &[punishment(PunishmentType::Warning)],
            PunishmentType::Warning,
            42
        ),
        EscalationOutcome::Issue(PunishmentType::Verbal)
    );
    assert_eq!(
        escalation_for_new_punishment(
            &[punishment(PunishmentType::Verbal)],
            PunishmentType::Verbal,
            42
        ),
        EscalationOutcome::Issue(PunishmentType::Strict)
    );
    assert_eq!(
        escalation_for_new_punishment(
            &[punishment(PunishmentType::Strict)],
            PunishmentType::Strict,
            42
        ),
        EscalationOutcome::ClanRemoval
    );
}

#[test]
fn strict_punishments_do_not_expire() {
    assert_eq!(expires_after_days(PunishmentType::Warning, 7, 14), Some(7));
    assert_eq!(expires_after_days(PunishmentType::Verbal, 7, 14), Some(14));
    assert_eq!(expires_after_days(PunishmentType::Strict, 7, 14), None);
}

#[test]
fn permissions_allow_admin_manage_guild_or_officer_role() {
    assert_eq!(
        can_moderate(true, false, &[], &[]),
        DisciplinePermission::Allowed
    );
    assert_eq!(
        can_moderate(false, true, &[], &[]),
        DisciplinePermission::Allowed
    );
    assert_eq!(
        can_moderate(false, false, &[10], &[10]),
        DisciplinePermission::Allowed
    );
    assert_eq!(
        can_moderate(false, false, &[11], &[10]),
        DisciplinePermission::Denied
    );
}

#[test]
fn target_validation_blocks_bot_owner_and_non_member() {
    assert!(valid_target(true, false, true).is_err());
    assert!(valid_target(false, true, true).is_err());
    assert!(valid_target(false, false, false).is_err());
    assert!(valid_target(false, false, true).is_ok());
}

#[test]
fn action_lock_blocks_duplicate_active_action() {
    let key = action_lock_key("issue", 42);
    let locks = vec![ActionLock {
        key: key.clone(),
        expires_unix: 100,
    }];

    assert!(!action_lock_allows(&locks, &key, 50));
    assert!(action_lock_allows(&locks, &key, 101));
}

#[test]
fn clan_removal_role_model_preserves_protected_roles() {
    let request = clan_removal_request(42, &[1, 2, 3], &[2], 99);

    assert_eq!(request.user_id, 42);
    assert_eq!(request.remove_role_ids, vec![1, 3]);
    assert_eq!(request.add_guest_role_id, 99);
}

#[test]
fn dm_draft_is_explicit_and_user_scoped() {
    let dm = punishment_dm(42, "Warning", "Reason");

    assert_eq!(dm.user_id, 42);
    assert_eq!(dm.title, "Warning");
    assert_eq!(dm.body, "Reason");
}

#[test]
fn discipline_visual_constants_match_legacy_source() {
    assert_eq!(crate::render::LEGACY_BOARD_COLOR, 0x2F80ED);
    assert_eq!(crate::render::board_title(), "XIII — Активные наказания");
    assert_eq!(crate::render::PANEL_ISSUE_LABEL, "Выдать наказание");
    assert_eq!(crate::render::PANEL_REMOVE_LABEL, "Снять наказание");
    assert_eq!(crate::render::PANEL_HISTORY_LABEL, "История участника");
    assert_eq!(crate::render::BOARD_PREV_LABEL, "Назад");
    assert_eq!(crate::render::BOARD_NEXT_LABEL, "Вперед");
    assert_eq!(
        crate::render::EMPTY_BOARD_DESCRIPTION,
        "Активных наказаний нет."
    );
    assert_eq!(
        crate::render::ISSUE_ID_MODAL_TITLE,
        "Ввести ID или упоминание"
    );
    assert_eq!(crate::render::REMOVE_REASON_LABEL, "Причина снятия");
}

#[tokio::test]
async fn sqlite_punishment_lifecycle_and_action_log_work() {
    let path = temp_db_path("discipline_repo");
    let repo = LegacySqliteDisciplineRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();

    let punishment_id = repo
        .create_punishment(
            1,
            42,
            PunishmentType::Warning,
            "late",
            Some(99),
            100,
            Some(200),
        )
        .await
        .unwrap();
    assert_eq!(repo.active_punishments(1, 42).await.unwrap().len(), 1);

    let log_id = repo
        .insert_action_log(ActionLogDraft {
            guild_id: 1,
            action_type: "issue".to_owned(),
            user_id: 42,
            issuer_id: Some(99),
            punishment_id: Some(punishment_id),
            payload_json: serde_json::json!({"reason":"late"}).to_string(),
            created_at: 100,
        })
        .await
        .unwrap();
    assert!(log_id > 0);

    let expired = repo.expire_due_punishments(1, 250, 10).await.unwrap();
    assert_eq!(expired, vec![punishment_id]);
    assert!(repo.active_punishments(1, 42).await.unwrap().is_empty());
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sqlite_action_lock_is_idempotent_until_expiry() {
    let path = temp_db_path("discipline_lock");
    let repo = LegacySqliteDisciplineRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();

    assert!(repo.acquire_action_lock("issue:42", 100, 10).await.unwrap());
    assert!(!repo.acquire_action_lock("issue:42", 100, 20).await.unwrap());
    assert!(repo
        .acquire_action_lock("issue:42", 200, 101)
        .await
        .unwrap());
    repo.release_action_lock("issue:42").await.unwrap();
    assert!(repo
        .acquire_action_lock("issue:42", 300, 110)
        .await
        .unwrap());
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sqlite_issue_transaction_converts_and_logs() {
    let path = temp_db_path("discipline_issue_transaction");
    let repo = LegacySqliteDisciplineRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();

    let warning_id = repo
        .create_punishment(
            1,
            42,
            PunishmentType::Warning,
            "first warning",
            Some(7),
            100,
            Some(200),
        )
        .await
        .unwrap();
    let verbal_id = repo
        .issue_punishment_with_log(IssuePunishmentDraft {
            guild_id: 1,
            user_id: 42,
            kind: PunishmentType::Verbal,
            reason: "second warning escalated".to_owned(),
            issuer_id: Some(7),
            issued_at: 120,
            expires_at: Some(240),
            convert_active_ids: vec![warning_id],
            action_type: "issue".to_owned(),
            payload_json: serde_json::json!({"requested_type":"warning"}).to_string(),
        })
        .await
        .unwrap();

    let history = repo.punishment_history(1, 42, 10).await.unwrap();
    assert_eq!(repo.action_log_count().await.unwrap(), 1);
    assert!(history
        .iter()
        .any(|record| record.id == warning_id && record.status == "converted"));
    assert!(history
        .iter()
        .any(|record| record.id == verbal_id && record.status == "active"));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sqlite_remove_transaction_logs_and_is_idempotent() {
    let path = temp_db_path("discipline_remove_transaction");
    let repo = LegacySqliteDisciplineRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();

    let punishment_id = repo
        .create_punishment(
            1,
            42,
            PunishmentType::Verbal,
            "remove me",
            Some(7),
            100,
            Some(200),
        )
        .await
        .unwrap();

    assert!(repo
        .remove_punishment_with_log(punishment_id, 8, "appeal accepted", 130)
        .await
        .unwrap());
    assert!(!repo
        .remove_punishment_with_log(punishment_id, 8, "duplicate", 131)
        .await
        .unwrap());
    let history = repo.punishment_history(1, 42, 10).await.unwrap();
    assert_eq!(repo.action_log_count().await.unwrap(), 1);
    assert_eq!(history[0].status, "manually_removed");
    assert_eq!(history[0].removed_by_id, Some(8));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sqlite_expiration_with_logs_is_idempotent() {
    let path = temp_db_path("discipline_expiration_transaction");
    let repo = LegacySqliteDisciplineRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();

    let due_id = repo
        .create_punishment(
            1,
            42,
            PunishmentType::Warning,
            "expired",
            Some(7),
            100,
            Some(120),
        )
        .await
        .unwrap();

    assert_eq!(
        repo.expire_due_punishments_with_logs(1, 130, 10)
            .await
            .unwrap(),
        vec![due_id]
    );
    assert!(repo
        .expire_due_punishments_with_logs(1, 130, 10)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(repo.action_log_count().await.unwrap(), 1);
    let history = repo.punishment_history(1, 42, 10).await.unwrap();
    assert_eq!(history[0].status, "expired");
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
