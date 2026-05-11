use crate::repository::google_form_signature;
use crate::state::{
    GoogleFormRow, OfficerReviewDraft, ReservedTicket, Ticket, TicketStatus, TicketType,
};

pub const LEGACY_GOOGLE_START_ROW: i64 = 13;
pub const LEGACY_GOOGLE_END_COLUMN: &str = "W";
pub const LEGACY_GOOGLE_TICKET_NUMBER_INDEX: usize = 19;
pub const DEFAULT_MAX_OPEN_TICKETS_PER_USER: i64 = 2;
pub const DEFAULT_REOPEN_WINDOW_HOURS: i64 = 5;
pub const TRANSCRIPT_FETCH_LIMIT: usize = 1000;

pub fn next_ticket_number(current_counter_value: i64) -> i64 {
    current_counter_value + 1
}

pub fn lifecycle_transition(current: TicketStatus, action: &str) -> Result<TicketStatus, String> {
    match (current, action) {
        (TicketStatus::Reserved, "open") => Ok(TicketStatus::Open),
        (TicketStatus::Open, "close") => Ok(TicketStatus::Closed),
        (TicketStatus::Closed, "reopen") => Ok(TicketStatus::Open),
        (TicketStatus::Closed, "delete") => Ok(TicketStatus::Deleted),
        (TicketStatus::Open, "delete") => Ok(TicketStatus::Deleted),
        _ => Err(format!(
            "invalid ticket transition from {current:?} using {action}"
        )),
    }
}

pub fn ticket_channel_name(ticket_type: TicketType, number: i64) -> String {
    format!("{}-{number}", ticket_type.channel_prefix())
}

pub fn custom_ticket_channel_name(input: &str, fallback_number: i64) -> String {
    let mut cleaned = String::new();
    for ch in input.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            cleaned.push(ch.to_ascii_lowercase());
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !cleaned.ends_with('-') {
            cleaned.push('-');
        }
        if cleaned.len() >= 80 {
            break;
        }
    }
    let cleaned = cleaned.trim_matches('-');
    if cleaned.is_empty() {
        ticket_channel_name(TicketType::Custom, fallback_number)
    } else {
        cleaned.to_owned()
    }
}

pub fn open_tickets_for_user(tickets: &[Ticket], user_id: u64) -> Vec<Ticket> {
    tickets
        .iter()
        .filter(|ticket| ticket.user_id == user_id && ticket.status == TicketStatus::Open)
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketCreationPlan {
    pub reserved: ReservedTicket,
    pub opener_user_id: u64,
    pub channel_name: String,
    pub ping_role_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketLifecyclePlan {
    pub action: TicketLifecycleAction,
    pub channel_id: u64,
    pub transcript_required: bool,
    pub dm_user: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketLifecycleAction {
    Close,
    Reopen,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GooglePollDecision {
    pub row: GoogleFormRow,
    pub signature: String,
    pub action: GooglePollAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GooglePollAction {
    SkipAlreadyProcessed,
    QueueOfficerReview(OfficerReviewDraft),
}

pub fn build_creation_plan(
    reserved: ReservedTicket,
    opener_user_id: u64,
    ping_role_id: Option<u64>,
) -> TicketCreationPlan {
    TicketCreationPlan {
        channel_name: reserved.ticket_name.clone(),
        reserved,
        opener_user_id,
        ping_role_id,
    }
}

pub fn lifecycle_plan(action: TicketLifecycleAction, channel_id: u64) -> TicketLifecyclePlan {
    TicketLifecyclePlan {
        action,
        channel_id,
        transcript_required: matches!(
            action,
            TicketLifecycleAction::Close | TicketLifecycleAction::Delete
        ),
        dm_user: matches!(
            action,
            TicketLifecycleAction::Close | TicketLifecycleAction::Reopen
        ),
    }
}

pub fn ping_role_for_ticket_type(
    ticket_type: TicketType,
    application_ping_role_id: u64,
    other_ping_role_id: u64,
    idea_ping_role_id: u64,
) -> Option<u64> {
    match ticket_type {
        TicketType::Application => non_zero(application_ping_role_id),
        TicketType::Complaint | TicketType::Custom => non_zero(other_ping_role_id),
        TicketType::Idea => non_zero(idea_ping_role_id),
    }
}

pub fn should_open_officer_review(row_processed: bool, signature_processed: bool) -> bool {
    !row_processed && !signature_processed
}

pub fn google_poll_decision(
    row: GoogleFormRow,
    already_processed_row: bool,
    already_processed_signature: bool,
    target_ticket_channel_id: u64,
    ticket_number: Option<i64>,
    applicant_name: Option<String>,
) -> GooglePollDecision {
    let signature = google_form_signature(&row);
    let action = if should_open_officer_review(already_processed_row, already_processed_signature) {
        GooglePollAction::QueueOfficerReview(OfficerReviewDraft {
            sheet_row: row.sheet_row,
            signature: signature.clone(),
            target_ticket_channel_id,
            ticket_number,
            applicant_name,
        })
    } else {
        GooglePollAction::SkipAlreadyProcessed
    };
    GooglePollDecision {
        row,
        signature,
        action,
    }
}

pub fn ticket_number_from_google_row(row: &GoogleFormRow) -> Option<i64> {
    row.values
        .get(LEGACY_GOOGLE_TICKET_NUMBER_INDEX)
        .and_then(|value| {
            let digits = value
                .chars()
                .filter(|ch| ch.is_ascii_digit())
                .collect::<String>();
            digits.parse::<i64>().ok()
        })
}

pub fn officer_review_description(row: &GoogleFormRow, ticket_number: Option<i64>) -> String {
    let ticket = ticket_number
        .map(|number| number.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    let preview = row
        .values
        .iter()
        .take(6)
        .enumerate()
        .map(|(idx, value)| format!("{}: {}", idx + 1, value))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Google Forms row {}\nTicket number: {}\n\n{}",
        row.sheet_row, ticket, preview
    )
}

pub fn accept_application_text() -> &'static str {
    "Application accepted. The applicant roles/nickname were updated when permissions allowed."
}

pub fn reject_application_text() -> &'static str {
    "Application rejected."
}

pub fn close_dm_text(ticket_name: &str) -> String {
    format!("Your ticket `{ticket_name}` was closed. Thank you for contacting XIII.")
}

pub fn reopen_dm_text(ticket_name: &str) -> String {
    format!("Your ticket `{ticket_name}` was reopened.")
}

fn non_zero(value: u64) -> Option<u64> {
    (value != 0).then_some(value)
}
