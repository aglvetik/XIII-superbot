use crate::commands::{
    can_accept_application, can_moderate_tickets, can_use_custom_ticket_command, is_accept_prefix,
    is_panel_prefix, is_reject_prefix,
};
use crate::discord_io::{
    allowed_mentions_are_limited, close_confirmation_payload, closed_ticket_owner_overwrite,
    officer_review_payload, permission_overwrites_for_ticket, reopen_ticket_owner_overwrite,
    ticket_open_payload, ticket_panel_button_specs, transcript_payload, TicketChannelCreateRequest,
};
use crate::google::{
    google_sheet_range, google_sheets_read_plan, values_to_rows, GoogleSheetsPollConfig,
};
use crate::interactions::{route_ticket_component, route_ticket_panel, TicketComponentRoute};
use crate::render::{
    member_history, panel_description, panel_title, transcript_html, transcript_model,
    transcript_text, TranscriptMessage, LEGACY_PANEL_COLOR,
};
use crate::repository::{google_form_signature, LegacySqliteTicketRepository};
use crate::runtime::{
    build_creation_plan, google_poll_decision, lifecycle_plan, lifecycle_transition,
    next_ticket_number, open_tickets_for_user, ping_role_for_ticket_type, ticket_channel_name,
    GooglePollAction, TicketLifecycleAction,
};
use crate::state::{GoogleFormRow, Ticket, TicketStatus, TicketType};
use chrono::{TimeZone, Utc};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[test]
fn ticket_counter_continues_from_legacy_value() {
    assert_eq!(next_ticket_number(23), 24);
}

#[test]
fn custom_id_routing_preserves_panel_buttons_and_lifecycle_buttons() {
    assert_eq!(route_ticket_panel("panel_apply"), Some("application"));
    assert_eq!(route_ticket_panel("panel_question"), Some("complaint"));
    assert_eq!(route_ticket_panel("panel_idea"), Some("idea"));
    assert_eq!(
        route_ticket_component("ticket_close_confirm"),
        Some(TicketComponentRoute::CloseConfirm)
    );
    assert_eq!(
        route_ticket_component("app_decision_reject"),
        Some(TicketComponentRoute::ApplicationReject)
    );
}

#[test]
fn ticket_panel_visual_constants_match_legacy_source() {
    assert_eq!(LEGACY_PANEL_COLOR, 0x3498DB);
    assert_eq!(panel_title(), "⚔️ **XIII Legion** ⚔️ | Центр поддержки");
    assert!(panel_description().contains("📩 **Заявка**"));
    assert!(panel_description().contains("🚨 **Жалоба**"));
    assert!(panel_description().contains("📈 **Повышение**"));

    let buttons = ticket_panel_button_specs();
    assert_eq!(buttons[0].custom_id, "panel_apply");
    assert_eq!(buttons[0].label, "📩 Подать заявку на вступление");
    assert_eq!(buttons[1].custom_id, "panel_question");
    assert_eq!(buttons[1].label, "🚨 Подать жалобу");
    assert_eq!(buttons[2].custom_id, "panel_idea");
    assert_eq!(buttons[2].label, "Заявка на повышение");
}

#[test]
fn lifecycle_transitions_match_close_reopen_delete() {
    assert_eq!(
        lifecycle_transition(TicketStatus::Reserved, "open").unwrap(),
        TicketStatus::Open
    );
    assert_eq!(
        lifecycle_transition(TicketStatus::Open, "close").unwrap(),
        TicketStatus::Closed
    );
    assert_eq!(
        lifecycle_transition(TicketStatus::Closed, "reopen").unwrap(),
        TicketStatus::Open
    );
    assert_eq!(
        lifecycle_transition(TicketStatus::Closed, "delete").unwrap(),
        TicketStatus::Deleted
    );
}

#[test]
fn command_permissions_and_prefixes_are_preserved() {
    assert!(can_use_custom_ticket_command(&[10], &[10]));
    assert!(can_moderate_tickets(&[20], &[20]));
    assert!(can_accept_application(&[30], &[30]));
    assert!(is_accept_prefix("!accept"));
    assert!(is_accept_prefix("!принять"));
    assert!(is_reject_prefix("!reject"));
    assert!(is_reject_prefix("!отклонить"));
    assert!(is_panel_prefix("!panel"));
}

#[test]
fn ticket_channel_and_transcript_are_stable() {
    let ticket = Ticket {
        id: 1,
        number: 24,
        ticket_type: TicketType::Application,
        status: TicketStatus::Open,
        channel_id: Some(100),
        user_id: 42,
    };

    assert_eq!(
        ticket_channel_name(ticket.ticket_type, ticket.number),
        "application-24"
    );
    assert_eq!(open_tickets_for_user(&[ticket.clone()], 42).len(), 1);
    assert!(transcript_text(&ticket, &["hello".to_owned()]).contains("Ticket #24"));
}

#[test]
fn permission_and_allowed_mentions_models_are_limited() {
    let reserved = crate::state::ReservedTicket {
        ticket_id: 1,
        number: 24,
        ticket_name: "application-24".to_owned(),
        ticket_type: TicketType::Application,
    };
    let plan = build_creation_plan(reserved, 42, Some(777));
    let channel = TicketChannelCreateRequest::from_plan(1, 2, 3, vec![4, 5], &plan);

    assert_eq!(channel.permission_plan().user_id, 42);
    assert!(channel.permission_plan().deny_everyone_read);

    let payload = ticket_open_payload(&plan, TicketType::Application);
    assert_eq!(payload.allowed_role_mentions, vec![777]);
    assert!(allowed_mentions_are_limited(
        &payload.allowed_role_mentions,
        &[777]
    ));
    assert!(!allowed_mentions_are_limited(
        &payload.allowed_role_mentions,
        &[778]
    ));
    assert!(close_confirmation_payload()
        .allowed_role_mentions
        .is_empty());
}

#[test]
fn ticket_permission_overwrites_are_scoped_to_ticket_participants() {
    let overwrites = permission_overwrites_for_ticket(1, 42, 10, &[11, 12], &[99]);

    assert_eq!(overwrites.len(), 6);
    assert!(overwrites
        .iter()
        .any(|overwrite| overwrite.id.get() == 1 && !overwrite.deny.is_empty()));
    assert!(overwrites
        .iter()
        .any(|overwrite| overwrite.id.get() == 42 && !overwrite.allow.is_empty()));
    assert!(overwrites
        .iter()
        .any(|overwrite| overwrite.id.get() == 10 && !overwrite.allow.is_empty()));
    assert!(overwrites
        .iter()
        .any(|overwrite| overwrite.id.get() == 99 && !overwrite.allow.is_empty()));

    let closed = closed_ticket_owner_overwrite(42);
    assert_eq!(closed.id.get(), 42);
    assert!(!closed.deny.is_empty());
    assert!(closed.allow.is_empty());

    let reopened = reopen_ticket_owner_overwrite(42);
    assert_eq!(reopened.id.get(), 42);
    assert!(!reopened.allow.is_empty());
    assert!(reopened.deny.is_empty());
}

#[test]
fn google_dedupe_signature_and_review_decision_are_stable() {
    let row = GoogleFormRow {
        sheet_row: 14,
        values: vec!["user".to_owned(), "steam".to_owned()],
    };
    let sig = google_form_signature(&row);
    assert_eq!(sig.len(), 64);

    let decision = google_poll_decision(
        row.clone(),
        false,
        false,
        123,
        Some(24),
        Some("Name".into()),
    );
    assert_eq!(decision.signature, sig);
    assert!(matches!(
        decision.action,
        GooglePollAction::QueueOfficerReview(_)
    ));

    let skipped = google_poll_decision(row, true, false, 123, None, None);
    assert!(matches!(
        skipped.action,
        GooglePollAction::SkipAlreadyProcessed
    ));
}

#[test]
fn google_read_plan_redacts_credentials_and_builds_range() {
    let config = GoogleSheetsPollConfig {
        credentials_file: PathBuf::from("credentials.json"),
        sheet_id: "sheet-secret-id".to_owned(),
        sheet_name: "Form Responses 1".to_owned(),
        start_row: 3,
        end_column: "V".to_owned(),
    };
    let plan = google_sheets_read_plan(&config);

    assert_eq!(plan.range, "'Form Responses 1'!A3:V");
    assert_eq!(plan.credentials_status, "<SET>");
    assert_eq!(plan.sheet_id_status, "<SET>");
    assert!(!format!("{plan:?}").contains("sheet-secret-id"));
    assert_eq!(google_sheet_range(&config), "'Form Responses 1'!A3:V");
}

#[test]
fn google_values_convert_to_stable_rows_without_network() {
    let rows = values_to_rows(
        3,
        vec![
            vec![serde_json::json!("ticket 12"), serde_json::json!(8.5)],
            vec![serde_json::Value::Null, serde_json::json!("Steam")],
        ],
    );

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].sheet_row, 3);
    assert_eq!(rows[0].values, vec!["ticket 12", "8.5"]);
    assert_eq!(rows[1].sheet_row, 4);
    assert_eq!(rows[1].values, vec!["", "Steam"]);
}

#[test]
fn transcript_and_officer_payloads_do_not_leak_secret_markers() {
    let transcript = transcript_payload(500, "application-24", "hello".to_owned());
    let officer = officer_review_payload(600, "application-24", "review".to_owned(), vec![700]);

    assert_eq!(transcript.filename, "transcript-application-24.html");
    assert_eq!(officer.allowed_role_mentions, vec![700]);
    assert!(!transcript.body.contains("DISCORD_TOKEN"));
    assert!(!officer.description.contains("PRIVATE_KEY"));
}

#[test]
fn transcript_html_sanitizes_mentions_and_preserves_attachments() {
    let ticket = crate::state::TicketRecord {
        ticket_id: 24,
        ticket_name: Some("application-24".to_owned()),
        opener_id: 42,
        ticket_type: TicketType::Application,
        channel_id: Some(9001),
        status: TicketStatus::Closed,
        created_at_utc: "2026-05-10T12:00:00Z".to_owned(),
        closed_at_utc: Some("2026-05-10T13:00:00Z".to_owned()),
        reopen_until_utc: None,
    };
    let html = transcript_html(
        &ticket,
        &[TranscriptMessage {
            author_id: 7,
            author_name: "Officer <XIII>".to_owned(),
            timestamp_utc: "2026-05-10T12:05:00Z".to_owned(),
            content: "@everyone see <@&123> and <@42>".to_owned(),
            attachment_urls: vec!["https://example.invalid/file.png".to_owned()],
        }],
    );

    assert!(html.contains("application-24"));
    assert!(html.contains("Officer &lt;XIII&gt;"));
    assert!(html.contains("https://example.invalid/file.png"));
    assert!(!html.contains("@everyone"));
    assert!(!html.contains("<@&123>"));
    assert!(!html.contains("<@42>"));
    assert!(!html.contains("DISCORD_TOKEN"));
}

#[tokio::test]
async fn sqlite_counter_reservation_finalization_and_rollback_are_transactional() {
    let (dir, db) = test_db_path("counter_reservation");
    let repo = LegacySqliteTicketRepository::open_writable_for_tests(&db)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    let now = Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap();

    let first = repo
        .reserve_ticket(42, TicketType::Application, now, 2)
        .await
        .unwrap();
    assert_eq!(first.number, 1);
    assert_eq!(
        repo.counter_value_async(TicketType::Application.counter_name())
            .await
            .unwrap(),
        1
    );
    assert!(repo
        .finalize_ticket_open(first.ticket_id, &first.ticket_name, 9001)
        .await
        .unwrap());

    let second = repo
        .reserve_ticket(42, TicketType::Application, now, 2)
        .await
        .unwrap();
    assert_eq!(second.number, 2);
    assert!(repo
        .rollback_reserved_ticket(second.ticket_id)
        .await
        .unwrap());

    let counts = repo.counts().await.unwrap();
    assert_eq!(counts.open_tickets, 1);
    assert_eq!(counts.reserved_tickets, 0);
    cleanup_dir(dir);
}

#[tokio::test]
async fn sqlite_lifecycle_close_reopen_delete_is_idempotent() {
    let (dir, db) = test_db_path("lifecycle");
    let repo = LegacySqliteTicketRepository::open_writable_for_tests(&db)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    let now = Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap();

    let reserved = repo
        .reserve_ticket(42, TicketType::Idea, now, 2)
        .await
        .unwrap();
    repo.finalize_ticket_open(reserved.ticket_id, &reserved.ticket_name, 9002)
        .await
        .unwrap();

    let closed = repo
        .mark_ticket_closed_by_channel(9002, now, 5)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(closed.status, TicketStatus::Closed);
    assert!(repo
        .mark_ticket_closed_by_channel(9002, now, 5)
        .await
        .unwrap()
        .is_none());

    assert!(repo
        .reopen_ticket_record(reserved.ticket_id, Some(9003), Some("idea-1"))
        .await
        .unwrap());
    assert!(!repo
        .reopen_ticket_record(reserved.ticket_id, None, None)
        .await
        .unwrap());
    assert!(repo.mark_ticket_deleted_by_channel(9003).await.unwrap());
    cleanup_dir(dir);
}

#[tokio::test]
async fn sqlite_google_form_dedupe_is_persistent() {
    let (dir, db) = test_db_path("google_dedupe");
    let repo = LegacySqliteTicketRepository::open_writable_for_tests(&db)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    let now = Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap();
    let row = GoogleFormRow {
        sheet_row: 99,
        values: vec!["a".into(), "b".into()],
    };
    let signature = google_form_signature(&row);

    assert!(!repo.processed_form_row_exists(99).await.unwrap());
    assert!(!repo
        .form_signature_processed_async(&signature)
        .await
        .unwrap());
    repo.mark_form_processed(99, &signature, now).await.unwrap();
    assert!(repo.processed_form_row_exists(99).await.unwrap());
    assert!(repo
        .form_signature_processed_async(&signature)
        .await
        .unwrap());
    cleanup_dir(dir);
}

#[tokio::test]
async fn sqlite_history_read_does_not_mutate() {
    let (dir, db) = test_db_path("history_read");
    let repo = LegacySqliteTicketRepository::open_writable_for_tests(&db)
        .await
        .unwrap();
    repo.create_schema_for_tests().await.unwrap();
    let now = Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap();
    let reserved = repo
        .reserve_ticket(42, TicketType::Complaint, now, 2)
        .await
        .unwrap();
    repo.finalize_ticket_open(reserved.ticket_id, &reserved.ticket_name, 9004)
        .await
        .unwrap();
    let before = repo.counts().await.unwrap();
    let rows = repo.tickets_for_user_async(42).await.unwrap();
    let rendered = member_history(&rows);
    let after = repo.counts().await.unwrap();

    assert!(rendered.contains("ticket_id="));
    assert_eq!(before.tickets, after.tickets);
    assert_eq!(before.open_tickets, after.open_tickets);
    assert!(transcript_model(&rows[0], &["hello".into()]).contains("Ticket transcript"));
    cleanup_dir(dir);
}

#[test]
fn lifecycle_plan_requires_transcript_for_close_and_delete() {
    assert!(lifecycle_plan(TicketLifecycleAction::Close, 1).transcript_required);
    assert!(!lifecycle_plan(TicketLifecycleAction::Reopen, 1).transcript_required);
    assert!(lifecycle_plan(TicketLifecycleAction::Delete, 1).transcript_required);
    assert_eq!(
        ping_role_for_ticket_type(TicketType::Idea, 1, 2, 3),
        Some(3)
    );
}

fn test_db_path(name: &str) -> (PathBuf, PathBuf) {
    let suffix = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("xiii-ticket-tests-{name}-{suffix}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let db = dir.join("tickets.db");
    (dir, db)
}

fn cleanup_dir(dir: PathBuf) {
    let _ = fs::remove_dir_all(dir);
}
