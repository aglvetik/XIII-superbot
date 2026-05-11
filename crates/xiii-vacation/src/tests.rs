use crate::commands::validate_vacation_role_split;
use crate::discord_io::officer_review_ping;
use crate::render::active_panel_lines;
use crate::repository::LegacySqliteVacationRepository;
use crate::runtime::{
    approve_request, early_end_vacation, expiry_action, ExpiryAction, VacationDecision,
};
use crate::state::{
    has_active_vacation, validate_new_request, VacationRecord, VacationRequestDraft, VacationStatus,
};

#[test]
fn role_id_conflict_is_rejected() {
    assert!(validate_vacation_role_split(1498022112131289214, 1498113605768314921).is_ok());
    assert!(validate_vacation_role_split(1, 1).is_err());
}

#[test]
fn approve_request_adds_actual_vacation_role() {
    assert_eq!(
        approve_request(9, 1498022112131289214),
        VacationDecision::Approve {
            request_id: 9,
            add_role_id: 1498022112131289214
        }
    );
}

#[test]
fn expiry_is_idempotent_for_non_active_records() {
    let ended = VacationRecord {
        id: 1,
        user_id: 2,
        role_id: 3,
        status: VacationStatus::Ended,
        started_unix: 0,
        expected_end_unix: 10,
        reason: "trip".to_owned(),
    };

    assert_eq!(expiry_action(&ended, 20), ExpiryAction::Ignore);
}

#[test]
fn active_vacation_blocks_duplicate_request() {
    let records = vec![VacationRecord {
        id: 1,
        user_id: 42,
        role_id: 3,
        status: VacationStatus::Active,
        started_unix: 0,
        expected_end_unix: 100,
        reason: "trip".to_owned(),
    }];

    assert!(has_active_vacation(&records, 42));
    assert!(!has_active_vacation(&records, 43));
}

#[test]
fn active_panel_renders_active_records_only() {
    let records = vec![
        VacationRecord {
            id: 1,
            user_id: 42,
            role_id: 3,
            status: VacationStatus::Active,
            started_unix: 50,
            expected_end_unix: 100,
            reason: "family trip".to_owned(),
        },
        VacationRecord {
            id: 2,
            user_id: 43,
            role_id: 3,
            status: VacationStatus::Expired,
            started_unix: 10,
            expected_end_unix: 90,
            reason: "expired".to_owned(),
        },
    ];

    let lines = active_panel_lines(&records);

    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("<@42>"));
    assert!(lines[0].contains("Причина: family trip"));
    assert!(lines[0].contains("<t:50:d>"));
    assert!(lines[0].contains("<t:100:R>"));
}

#[test]
fn request_creation_validates_duplicate_and_duration() {
    let active = vec![VacationRecord {
        id: 1,
        user_id: 42,
        role_id: 3,
        status: VacationStatus::Active,
        started_unix: 0,
        expected_end_unix: 100,
        reason: "trip".to_owned(),
    }];
    let draft = VacationRequestDraft {
        user_id: 42,
        start_unix: 0,
        expected_end_unix: 86_400,
        reason: "trip".to_owned(),
    };

    assert!(validate_new_request(&active, &draft, 14).is_err());
    assert!(validate_new_request(&[], &draft, 14).is_ok());
    assert!(validate_new_request(
        &[],
        &VacationRequestDraft {
            expected_end_unix: 2_000_000,
            ..draft
        },
        1
    )
    .is_err());
}

#[test]
fn early_end_removes_actual_vacation_role() {
    assert_eq!(
        early_end_vacation(6, 1498022112131289214),
        VacationDecision::EarlyEnd {
            vacation_id: 6,
            remove_role_id: 1498022112131289214
        }
    );
}

#[test]
fn officer_ping_allows_only_configured_role() {
    let ping = officer_review_ping(Some(123), 9);

    assert_eq!(ping.allowed_role_mentions, vec![123]);
    assert_eq!(ping.content, "<@&123>");

    let no_ping = officer_review_ping(None, 9);
    assert!(no_ping.allowed_role_mentions.is_empty());
    assert!(no_ping.content.is_empty());
}

#[test]
fn vacation_visual_constants_match_legacy_source() {
    assert_eq!(crate::render::LEGACY_PANEL_COLOR, 0x5865F2);
    assert_eq!(crate::render::LEGACY_STATUS_PENDING_COLOR, 0xFEE75C);
    assert_eq!(crate::render::LEGACY_FOOTER, "XIII Vacation System");
    assert_eq!(crate::render::request_panel_title(), "Отпуск XIII");
    assert!(crate::render::REQUEST_PANEL_DESCRIPTION.contains("Подай заявку на отпуск"));
    assert_eq!(
        crate::render::REQUEST_BUTTON_LABEL,
        "Подать заявку на отпуск"
    );
    assert_eq!(crate::render::REQUEST_MODAL_TITLE, "Заявка на отпуск");
    assert_eq!(crate::render::REQUEST_MODAL_DAYS_LABEL, "На сколько дней?");
    assert_eq!(crate::render::REQUEST_MODAL_REASON_LABEL, "Причина отпуска");
    assert_eq!(crate::render::APPROVE_BUTTON_LABEL, "Принять");
    assert_eq!(crate::render::REJECT_BUTTON_LABEL, "Отклонить");
    assert_eq!(
        crate::render::ACTIVE_PANEL_EMPTY,
        "Сейчас активных отпусков нет."
    );
    assert_eq!(
        crate::commands::VACATIONS_DISABLED_RESPONSE,
        "Функционал команды отключён. Список активных отпусков можно посмотреть тут: <#{channel_id}>."
    );
}

#[tokio::test]
async fn sqlite_request_approve_and_end_are_transactional() {
    let path = temp_db_path("vacation_repo");
    let repo = LegacySqliteVacationRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-05-09T10:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let request_id = repo
        .create_request(1, 42, 3, "family trip", now)
        .await
        .unwrap();
    assert!(repo
        .create_request(1, 42, 3, "duplicate", now)
        .await
        .unwrap_err()
        .contains("pending"));

    let vacation = repo
        .approve_request_and_create_vacation(request_id, 99, 1498022112131289214, now)
        .await
        .unwrap();
    assert_eq!(vacation.request_id, request_id);
    assert_eq!(vacation.status, "ACTIVE");
    assert!(repo
        .create_request(1, 42, 3, "active duplicate", now)
        .await
        .unwrap_err()
        .contains("active"));

    let ended = repo
        .end_vacation(vacation.id, 42, "EARLY_USER", now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ended.status, "ENDED");
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sqlite_reject_is_idempotency_guarded() {
    let path = temp_db_path("vacation_reject");
    let repo = LegacySqliteVacationRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    let now = chrono::Utc::now();
    let request_id = repo.create_request(1, 43, 1, "busy", now).await.unwrap();

    repo.reject_request(request_id, 99, now).await.unwrap();
    assert!(repo.reject_request(request_id, 99, now).await.is_err());
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
