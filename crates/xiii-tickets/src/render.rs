use crate::state::{OfficerReviewDraft, Ticket, TicketRecord, TicketStatus, TicketType};
use chrono::{DateTime, Utc};

pub const LEGACY_PANEL_COLOR: u32 = 0x3498DB;
pub const LEGACY_COMPLAINT_COLOR: u32 = 0xE74C3C;
pub const LEGACY_PROMOTION_COLOR: u32 = 0xF1C40F;
pub const LEGACY_OFFICER_REVIEW_COLOR: u32 = 0x2ECC71;
pub const LEGACY_PANEL_TITLE: &str = "⚔️ **XIII Legion** ⚔️ | Центр поддержки";
pub const LEGACY_PANEL_DESCRIPTION: &str = "📩 **Заявка** — Хочу вступить в клан\n\n🚨 **Жалоба** — Подать жалобу на игрока\n\n📈 **Повышение** — Подать заявку на повышение";
pub const APPLICATION_FORM_URL: &str = "https://forms.gle/jL7W5Y7b1rEnVdFk8";
pub const TICKET_CREATED_TITLE: &str = "Тикет создан";
pub const CLOSE_CONFIRM_TITLE: &str = "Подтверждение";
pub const CLOSE_CONFIRM_DESCRIPTION: &str = "Вы уверены, что хотите закрыть тикет?";
pub const CLOSE_CANCELLED_TEXT: &str = "Закрытие тикета отменено ✅";
pub const DELETE_SUCCESS_TEXT: &str = "Тикет удаляется ✅";
pub const REOPEN_SUCCESS_TEXT: &str = "Тикет переоткрыт ✅";
pub const CLOSE_SUCCESS_TEXT: &str = "Тикет закрыт ✅";
pub const STAFF_NOTES_MODAL_TITLE: &str = "Заметки персонала";
pub const STAFF_NOTES_MODAL_LABEL: &str = "Заметка";
pub const STAFF_NOTE_PREFIX: &str = "Заметка персонала:";
pub const STAFF_NOTE_ADDED_TEXT: &str = "Заметка добавлена.";
pub const STAFF_NOTE_DELETED_TEXT: &str = "Заметки удаляются ✅";
pub const TRANSCRIPT_ATTACHED_TEXT: &str = "Транскрипт тикета приложен.";
pub const TICKET_CLOSE_LABEL: &str = "🔒 Закрыть тикет";
pub const TICKET_CLOSE_CONFIRM_LABEL: &str = "Да, закрыть";
pub const TICKET_CLOSE_CANCEL_LABEL: &str = "Отмена";
pub const TICKET_STAFF_NOTES_LABEL: &str = "📝 Заметки персонала";
pub const TICKET_DELETE_LABEL: &str = "🗑️ Удалить тикет";
pub const TICKET_REOPEN_LABEL: &str = "🔓 Переоткрыть тикет";
pub const TICKET_NOTES_DELETE_LABEL: &str = "🗑 Удалить заметки";
pub const APP_DECISION_ACCEPT_LABEL: &str = "✅ Принять";
pub const APP_DECISION_REJECT_LABEL: &str = "❌ Отклонить";
pub const CLOSE_RESULT_SENT_TITLE: &str = "Транскрипт отправлен";
pub const CLOSE_RESULT_SENT_DESCRIPTION: &str =
    "Копия тикета сохранена в канале транскриптов и отправлена пользователю в личные сообщения.";
pub const CLOSE_RESULT_SAVED_TITLE: &str = "Транскрипт сохранён";
pub const CLOSE_RESULT_SAVED_DESCRIPTION: &str =
    "Копия тикета сохранена в канале транскриптов, но не удалось отправить её пользователю в личные сообщения.";
pub const CLOSE_RESULT_FAILED_TITLE: &str = "Тикет закрыт";
pub const CLOSE_RESULT_FAILED_DESCRIPTION: &str =
    "Не удалось сохранить транскрипт. Проверьте логи приложения.";
pub const CLOSE_CANCELLED_TITLE: &str = "Закрытие тикета отменено";
pub const CLOSE_CANCELLED_DESCRIPTION: &str = "Тикет останется открытым.";
pub const TRANSCRIPT_SUMMARY_TITLE: &str = "Тикет закрыт";
pub const TRANSCRIPT_FIELD_TICKET: &str = "Тикет";
pub const TRANSCRIPT_FIELD_NUMBER: &str = "Номер";
pub const TRANSCRIPT_FIELD_TYPE: &str = "Тип";
pub const TRANSCRIPT_FIELD_OPENED_BY: &str = "Открыл";
pub const TRANSCRIPT_FIELD_CLOSED_BY: &str = "Закрыл";
pub const TRANSCRIPT_FIELD_PARTICIPANTS: &str = "Участников";
pub const TRANSCRIPT_FIELD_OPENED_AT: &str = "Открыт";
pub const TRANSCRIPT_FIELD_CLOSED_AT: &str = "Закрыт";
pub const UNKNOWN_VALUE: &str = "Не удалось определить";
pub const LEGACY_CLOSE_SUCCESS_COLOR: u32 = 0x2ECC71;
pub const LEGACY_CLOSE_WARNING_COLOR: u32 = 0xE67E22;
pub const LEGACY_CLOSE_FAILURE_COLOR: u32 = 0x607D8B;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedOfficerReviewScore {
    pub value: f64,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptMessage {
    pub author_id: u64,
    pub author_name: String,
    pub timestamp_utc: String,
    pub content: String,
    pub attachment_urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketEmbedDraft {
    pub title: String,
    pub description: Option<String>,
    pub fields: Vec<RenderField>,
    pub color: u32,
    pub footer: Option<String>,
}

pub fn panel_title() -> &'static str {
    LEGACY_PANEL_TITLE
}

pub fn panel_description() -> &'static str {
    LEGACY_PANEL_DESCRIPTION
}

pub fn close_confirmation_embed() -> TicketEmbedDraft {
    TicketEmbedDraft {
        title: CLOSE_CONFIRM_TITLE.to_owned(),
        description: Some(CLOSE_CONFIRM_DESCRIPTION.to_owned()),
        fields: Vec::new(),
        color: LEGACY_COMPLAINT_COLOR,
        footer: None,
    }
}

pub fn close_cancelled_embed() -> TicketEmbedDraft {
    TicketEmbedDraft {
        title: CLOSE_CANCELLED_TITLE.to_owned(),
        description: Some(CLOSE_CANCELLED_DESCRIPTION.to_owned()),
        fields: Vec::new(),
        color: LEGACY_CLOSE_SUCCESS_COLOR,
        footer: None,
    }
}

pub fn close_result_embed(transcript_saved: bool, dm_sent: bool) -> TicketEmbedDraft {
    let (title, description, color) = if transcript_saved && dm_sent {
        (
            CLOSE_RESULT_SENT_TITLE,
            CLOSE_RESULT_SENT_DESCRIPTION,
            LEGACY_CLOSE_SUCCESS_COLOR,
        )
    } else if transcript_saved {
        (
            CLOSE_RESULT_SAVED_TITLE,
            CLOSE_RESULT_SAVED_DESCRIPTION,
            LEGACY_CLOSE_WARNING_COLOR,
        )
    } else {
        (
            CLOSE_RESULT_FAILED_TITLE,
            CLOSE_RESULT_FAILED_DESCRIPTION,
            LEGACY_CLOSE_FAILURE_COLOR,
        )
    };
    TicketEmbedDraft {
        title: title.to_owned(),
        description: Some(description.to_owned()),
        fields: Vec::new(),
        color,
        footer: None,
    }
}

pub fn application_form_message(opener_mention: &str) -> String {
    format!(
        "## Приветствую, {opener_mention}!\n### Чтобы вступить, заполни короткую анкету по ссылке:\n### {}",
        APPLICATION_FORM_URL
    )
}

pub fn promotion_request_message(opener_mention: &str, ping_role_mention: &str) -> String {
    format!(
        "## Здравствуйте, {opener_mention}!\n### На какую должность вы претендуете?\n### Опишите почему вы достойный кандидат.\n### {ping_role_mention} займется вашей заявкой."
    )
}

pub fn complaint_main_message(opener_mention: &str, ping_role_mention: &str) -> String {
    format!("{opener_mention} Жалоба принята. {ping_role_mention} рассмотрят её в ближайшее время.")
}

pub fn complaint_embed_description() -> &'static str {
    "🚨 **Жалоба на игрока**\nПожалуйста, укажите:\n• Ник/ID игрока\n• Что произошло (по фактам)\n• Время/сервер/место (если важно)\n• Скрины/видео (если есть)\n"
}

pub fn custom_ticket_description() -> &'static str {
    "Используйте этот канал для обсуждения темы тикета."
}

pub fn applicant_test_failed_channel_text() -> &'static str {
    "### К сожалению вы не прошли тест, мы не можем принять вас.\n### Если не осталось вопросов закройте тикет.\n### С уважением XIII Legion."
}

pub fn applicant_test_passed_channel_text(interviewer_role_id: u64) -> String {
    format!(
        "### Поздравляю вы прошли тест.\n### Напиши тут время, когда удобно пройти краткое собеседование у <@&{interviewer_role_id}>."
    )
}

pub fn accept_application_channel_text(interviewer_role_id: u64) -> String {
    format!(
        "### Вы приняты на испытательный срок, он продлится 2 недели.\n### По окончании испытательного срока <@&{interviewer_role_id}> примет решение о окончательном принятии в клан."
    )
}

pub fn reject_application_channel_text() -> &'static str {
    "### К сожалению вы не прошли собеседование, мы не можем принять вас.\n### Если не осталось вопросов закройте тикет.\n### С уважением XIII Legion."
}

pub fn close_dm_content(ticket_name: &str) -> String {
    format!("📩 Ваш тикет `{ticket_name}` был закрыт!\nСпасибо за обращение 💙")
}

pub fn reopen_dm_content(ticket_name: &str) -> String {
    format!("Ваш тикет `{ticket_name}` был переоткрыт.")
}

pub fn reopen_channel_message(actor_id: u64) -> String {
    format!("🔄 Тикет переоткрыт модератором <@{actor_id}>.")
}

pub fn transcript_summary_embed(
    ticket: &TicketRecord,
    closer_id: u64,
    closed_at: DateTime<Utc>,
    participant_count: Option<usize>,
) -> TicketEmbedDraft {
    TicketEmbedDraft {
        title: TRANSCRIPT_SUMMARY_TITLE.to_owned(),
        description: None,
        fields: vec![
            RenderField {
                name: TRANSCRIPT_FIELD_TICKET.to_owned(),
                value: ticket
                    .ticket_name
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| UNKNOWN_VALUE.to_owned()),
                inline: true,
            },
            RenderField {
                name: TRANSCRIPT_FIELD_NUMBER.to_owned(),
                value: ticket_number_text(ticket),
                inline: true,
            },
            RenderField {
                name: TRANSCRIPT_FIELD_TYPE.to_owned(),
                value: ticket_type_label(ticket.ticket_type).to_owned(),
                inline: true,
            },
            RenderField {
                name: TRANSCRIPT_FIELD_OPENED_BY.to_owned(),
                value: format!("<@{}>", ticket.opener_id),
                inline: true,
            },
            RenderField {
                name: TRANSCRIPT_FIELD_CLOSED_BY.to_owned(),
                value: format!("<@{closer_id}>"),
                inline: true,
            },
            RenderField {
                name: TRANSCRIPT_FIELD_PARTICIPANTS.to_owned(),
                value: participant_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| UNKNOWN_VALUE.to_owned()),
                inline: true,
            },
            RenderField {
                name: TRANSCRIPT_FIELD_OPENED_AT.to_owned(),
                value: format_legacy_datetime(Some(ticket.created_at_utc.as_str())),
                inline: false,
            },
            RenderField {
                name: TRANSCRIPT_FIELD_CLOSED_AT.to_owned(),
                value: closed_at.format("%d.%m.%Y %H:%M:%S UTC").to_string(),
                inline: false,
            },
        ],
        color: LEGACY_CLOSE_FAILURE_COLOR,
        footer: None,
    }
}

pub fn officer_review_description(values: &[String], ticket_number: Option<i64>) -> String {
    fn value(values: &[String], index: usize) -> &str {
        values
            .get(index)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("—")
    }

    let score_raw = value(values, 2);
    let parsed_score = parse_officer_review_score(score_raw);
    let result_text = if parsed_score
        .as_ref()
        .map(|score| score.value >= 7.0)
        .unwrap_or(false)
    {
        "✅ Тест пройден"
    } else {
        "❌ Тест не пройден"
    };
    let score_display = parsed_score
        .as_ref()
        .map(|score| score.display.as_str())
        .unwrap_or(score_raw);
    let score_section = score_display.to_owned();

    format!(
        "🧾 Заявка XIII Legion\n\n📊 Номер тикета\n{}\n\n👤 Имя Steam\n{}\n\n🎮 Steam ID\n{}\n\n📊 Баллы\n{}\n\n🏠 Бывший клан\n{}\n\n⏱️ Время в Squad\n{}\n\n🤝 Готовы ли вы поддерживать дружеские отношения между соклановцами?\n{}\n\n🫃🏻 Сколько вам лет?\n{}\n\n💑 Как вы узнали о нашем клане?\n{}\n\n📌 Результат\n{}\n\nПеред принятием решения проведите собеседование с игроком.",
        ticket_number
            .map(|value| value.to_string())
            .unwrap_or_else(|| "—".to_owned()),
        value(values, 3),
        value(values, 4),
        score_section,
        value(values, 16),
        value(values, 17),
        value(values, 18),
        value(values, 22),
        value(values, 21),
        result_text
    )
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
            "Тикет {} закрыт. Переоткрытие доступно до {}.",
            ticket
                .ticket_name
                .clone()
                .unwrap_or_else(|| ticket.ticket_id.to_string()),
            ticket
                .reopen_until_utc
                .as_deref()
                .unwrap_or("настроенного срока")
        ),
        _ => "Состояние тикета не изменилось.".to_owned(),
    }
}

pub fn officer_review_text(draft: &OfficerReviewDraft) -> String {
    format!(
        "Sheet row {} is ready for officer review. Signature {}. Target channel {}.",
        draft.sheet_row, draft.signature, draft.target_ticket_channel_id
    )
}

pub fn parse_officer_review_score(raw: &str) -> Option<ParsedOfficerReviewScore> {
    let numbers = numeric_tokens(raw);
    let first = numbers.first()?;
    let value = first.parse::<f64>().ok()?;
    let display = if raw.contains('/') && numbers.len() >= 2 {
        format!(
            "{} / {}",
            format_score_number(value),
            format_score_number(numbers[1].parse::<f64>().ok()?)
        )
    } else {
        format_score_number(value)
    };
    Some(ParsedOfficerReviewScore { value, display })
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

fn ticket_type_label(ticket_type: TicketType) -> &'static str {
    match ticket_type {
        TicketType::Application => "Заявка на вступление",
        TicketType::Complaint => "Жалоба на игрока",
        TicketType::Idea => "Заявка на повышение",
        TicketType::Custom => "Пользовательский тикет",
    }
}

fn ticket_number_text(ticket: &TicketRecord) -> String {
    ticket
        .ticket_name
        .as_deref()
        .and_then(|name| name.rsplit('-').next())
        .and_then(|value| value.parse::<i64>().ok())
        .map(|value| value.to_string())
        .unwrap_or_else(|| UNKNOWN_VALUE.to_owned())
}

fn format_legacy_datetime(value: Option<&str>) -> String {
    value
        .and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())
        .map(|time| {
            time.with_timezone(&Utc)
                .format("%d.%m.%Y %H:%M:%S UTC")
                .to_string()
        })
        .unwrap_or_else(|| UNKNOWN_VALUE.to_owned())
}

fn numeric_tokens(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut seen_separator = false;

    for ch in raw.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
            continue;
        }
        if (ch == '.' || ch == ',') && !current.is_empty() && !seen_separator {
            current.push('.');
            seen_separator = true;
            continue;
        }
        if !current.is_empty() {
            if current.ends_with('.') {
                current.pop();
            }
            if !current.is_empty() {
                tokens.push(current.clone());
            }
            current.clear();
            seen_separator = false;
        }
    }

    if !current.is_empty() {
        if current.ends_with('.') {
            current.pop();
        }
        if !current.is_empty() {
            tokens.push(current);
        }
    }

    tokens
}

fn format_score_number(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        let mut text = value.to_string();
        if text.contains('.') {
            while text.ends_with('0') {
                text.pop();
            }
            if text.ends_with('.') {
                text.pop();
            }
        }
        text
    }
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
