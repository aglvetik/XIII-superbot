use crate::state::{OfficerReviewDraft, Ticket, TicketRecord, TicketStatus};

pub const LEGACY_PANEL_COLOR: u32 = 0x3498DB;
pub const LEGACY_PANEL_TITLE: &str = "⚔️ **XIII Legion** ⚔️ | Центр поддержки";
pub const LEGACY_PANEL_DESCRIPTION: &str = "📩 **Заявка** — Хочу вступить в клан\n\n🚨 **Жалоба** — Подать жалобу на игрока\n\n📈 **Повышение** — Подать заявку на повышение";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptMessage {
    pub author_id: u64,
    pub author_name: String,
    pub timestamp_utc: String,
    pub content: String,
    pub attachment_urls: Vec<String>,
}

pub fn panel_title() -> &'static str {
    LEGACY_PANEL_TITLE
}

pub fn panel_description() -> &'static str {
    LEGACY_PANEL_DESCRIPTION
}

pub fn transcript_text(ticket: &Ticket, messages: &[String]) -> String {
    let mut text = format!("Ticket #{}\nUser: {}\n\n", ticket.number, ticket.user_id);
    for message in messages {
        text.push_str(message);
        text.push('\n');
    }
    text
}

pub fn ticket_status_summary(ticket: &TicketRecord) -> String {
    format!(
        "ticket_id={} type={:?} status={:?} channel_id={} created_at={}",
        ticket.ticket_id,
        ticket.ticket_type,
        ticket.status,
        ticket
            .channel_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_owned()),
        ticket.created_at_utc
    )
}

pub fn member_history(ticket_rows: &[TicketRecord]) -> String {
    if ticket_rows.is_empty() {
        return "No ticket history found for this user.".to_owned();
    }
    ticket_rows
        .iter()
        .map(ticket_status_summary)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn close_result_text(ticket: &TicketRecord) -> String {
    match ticket.status {
        TicketStatus::Closed => format!(
            "Ticket {} is closed. Reopen is available until {}.",
            ticket
                .ticket_name
                .clone()
                .unwrap_or_else(|| ticket.ticket_id.to_string()),
            ticket
                .reopen_until_utc
                .as_deref()
                .unwrap_or("the configured cutoff")
        ),
        _ => "Ticket close did not change state.".to_owned(),
    }
}

pub fn officer_review_text(draft: &OfficerReviewDraft) -> String {
    format!(
        "Sheet row {} is ready for officer review. Signature {}. Target channel {}.",
        draft.sheet_row, draft.signature, draft.target_ticket_channel_id
    )
}

pub fn transcript_model(ticket: &TicketRecord, messages: &[String]) -> String {
    let mut text = format!(
        "Ticket transcript\nTicket ID: {}\nTicket Name: {}\nUser ID: {}\nStatus: {:?}\n\n",
        ticket.ticket_id,
        ticket.ticket_name.as_deref().unwrap_or("-"),
        ticket.opener_id,
        ticket.status
    );
    for message in messages {
        text.push_str(message);
        text.push('\n');
    }
    text
}

pub fn transcript_html(ticket: &TicketRecord, messages: &[TranscriptMessage]) -> String {
    let mut html = String::new();
    html.push_str("<!doctype html><html><head><meta charset=\"utf-8\"><title>");
    html.push_str(&html_escape(
        ticket.ticket_name.as_deref().unwrap_or("ticket transcript"),
    ));
    html.push_str("</title></head><body>");
    html.push_str("<h1>Ticket transcript</h1>");
    html.push_str("<dl>");
    html.push_str(&format!("<dt>Ticket ID</dt><dd>{}</dd>", ticket.ticket_id));
    html.push_str(&format!(
        "<dt>Ticket Name</dt><dd>{}</dd>",
        html_escape(ticket.ticket_name.as_deref().unwrap_or("-"))
    ));
    html.push_str(&format!("<dt>User ID</dt><dd>{}</dd>", ticket.opener_id));
    html.push_str(&format!("<dt>Status</dt><dd>{:?}</dd>", ticket.status));
    html.push_str("</dl><hr>");
    for message in messages {
        html.push_str("<article>");
        html.push_str(&format!(
            "<header><strong>{}</strong> <code>{}</code> <time>{}</time></header>",
            html_escape(&message.author_name),
            message.author_id,
            html_escape(&message.timestamp_utc)
        ));
        html.push_str("<pre>");
        html.push_str(&html_escape(&sanitize_mentions(&message.content)));
        html.push_str("</pre>");
        if !message.attachment_urls.is_empty() {
            html.push_str("<ul>");
            for url in &message.attachment_urls {
                html.push_str("<li>");
                html.push_str(&html_escape(url));
                html.push_str("</li>");
            }
            html.push_str("</ul>");
        }
        html.push_str("</article>");
    }
    html.push_str("</body></html>");
    html
}

pub fn sanitize_mentions(content: &str) -> String {
    content
        .replace("@everyone", "@\u{200b}everyone")
        .replace("@here", "@\u{200b}here")
        .replace("<@&", "<@\u{200b}&")
        .replace("<@", "<@\u{200b}")
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
