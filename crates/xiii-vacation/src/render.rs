use crate::state::{VacationRecord, VacationStatus};

pub const LEGACY_PANEL_COLOR: u32 = 0x5865F2;
pub const LEGACY_STATUS_PENDING_COLOR: u32 = 0xFEE75C;
pub const LEGACY_STATUS_APPROVED_COLOR: u32 = 0x57F287;
pub const LEGACY_STATUS_REJECTED_COLOR: u32 = 0xED4245;
pub const LEGACY_FOOTER: &str = "XIII Vacation System";

pub const REQUEST_PANEL_TITLE: &str = "Отпуск XIII";
pub const REQUEST_PANEL_DESCRIPTION: &str = "Нужно временно отойти от активности?\nПодай заявку на отпуск через кнопку ниже.\n\nУкажи количество дней и причину.\nОфицеры рассмотрят заявку и примут решение.";
pub const REQUEST_BUTTON_LABEL: &str = "Подать заявку на отпуск";

pub const REQUEST_MODAL_TITLE: &str = "Заявка на отпуск";
pub const REQUEST_MODAL_DAYS_LABEL: &str = "На сколько дней?";
pub const REQUEST_MODAL_DAYS_PLACEHOLDER: &str = "Например: 3";
pub const REQUEST_MODAL_REASON_LABEL: &str = "Причина отпуска";
pub const REQUEST_MODAL_REASON_PLACEHOLDER: &str = "Коротко объясни причину";

pub const OFFICER_REVIEW_TITLE: &str = "Новая заявка на отпуск";
pub const OFFICER_FIELD_USER: &str = "Участник";
pub const OFFICER_FIELD_DAYS: &str = "Количество дней";
pub const OFFICER_FIELD_REASON: &str = "Причина";
pub const OFFICER_FIELD_STATUS: &str = "Статус";
pub const OFFICER_FIELD_DECIDED_BY: &str = "Рассмотрел";
pub const OFFICER_REVIEW_PREVIEW: &str = "Участник / Количество дней / Причина / Статус";
pub const OFFICER_STATUS_PENDING: &str = "Ожидает рассмотрения";
pub const OFFICER_STATUS_APPROVED: &str = "Одобрено";
pub const OFFICER_STATUS_REJECTED: &str = "Отклонено";
pub const APPROVE_BUTTON_LABEL: &str = "Принять";
pub const REJECT_BUTTON_LABEL: &str = "Отклонить";

pub const ACTIVE_PANEL_TITLE: &str = "Активные отпуска XIII";
pub const ACTIVE_PANEL_EMPTY: &str = "Сейчас активных отпусков нет.";
pub const ACTIVE_PANEL_COUNT_PREVIEW: &str = "Сейчас в отпуске: N";
pub const ACTIVE_PANEL_TRUNCATED_PREVIEW: &str = "Показаны первые N отпусков из M.";

pub const SUBMITTED_RESPONSE: &str =
    "Ваша заявка на отпуск отправлена. Офицеры рассмотрят её в ближайшее время.";
pub const APPROVED_RESPONSE: &str = "Заявка одобрена. Роль отпуска выдана.";
pub const REJECTED_RESPONSE: &str = "Заявка отклонена.";
pub const END_PROMPT_RESPONSE: &str = "Вы уверены, что хотите досрочно закончить отпуск?";
pub const ENDED_RESPONSE: &str = "Вы досрочно закончили отпуск. Роль отпуска снята.";
pub const END_CANCELLED_RESPONSE: &str = "Завершение отпуска отменено.";
pub const ALREADY_ENDED_RESPONSE: &str = "Отпуск уже завершён.";
pub const INVALID_DURATION_RESPONSE: &str = "Некорректная длительность отпуска.";
pub const NO_REASON: &str = "Не указана";

pub const APPROVED_DM_TITLE: &str = "Отпуск одобрен";
pub const APPROVED_DM_DESCRIPTION: &str = "Ваша заявка на отпуск была одобрена.\nВам выдана роль отпуска.\n\nВы можете досрочно закончить отпуск в любой момент через кнопку ниже.";
pub const APPROVED_DM_DAYS_FIELD: &str = "Количество дней";
pub const APPROVED_DM_END_FIELD: &str = "Окончание отпуска";

pub const REJECTED_DM_TITLE: &str = "Заявка на отпуск отклонена";
pub const REJECTED_DM_DESCRIPTION: &str = "Ваша заявка на отпуск была отклонена офицерами.";

pub const EXPIRED_DM_TITLE: &str = "Отпуск завершён";
pub const EXPIRED_DM_DESCRIPTION: &str = "Ваш отпуск завершён. Роль отпуска снята.";

pub const EARLY_END_BUTTON_LABEL: &str = "Досрочно закончить отпуск";
pub const EARLY_END_CONFIRM_LABEL: &str = "Да, закончить";
pub const EARLY_END_CANCEL_LABEL: &str = "Отмена";

pub const ACTIVE_VACATION_DISPLAY_LIMIT: usize = 20;
pub const ACTIVE_VACATION_REASON_LIMIT: usize = 60;
pub const ACTIVE_VACATION_DESCRIPTION_BUDGET: usize = 4000;

pub fn request_panel_title() -> &'static str {
    REQUEST_PANEL_TITLE
}

pub fn active_panel_lines(records: &[VacationRecord]) -> Vec<String> {
    active_panel_lines_with_truncation(records).0
}

pub fn active_panel_lines_with_truncation(records: &[VacationRecord]) -> (Vec<String>, usize) {
    let active = records
        .iter()
        .filter(|record| record.status == VacationStatus::Active)
        .collect::<Vec<_>>();

    let mut lines = Vec::new();
    for (index, record) in active.iter().enumerate() {
        if lines.len() >= ACTIVE_VACATION_DISPLAY_LIMIT {
            break;
        }
        let item = format!(
            "\n\n**{}.** <@{}> • {} → {} • {}\n> Причина: {}",
            index + 1,
            record.user_id,
            discord_timestamp(record.started_unix, "d"),
            discord_timestamp(record.expected_end_unix, "d"),
            discord_timestamp(record.expected_end_unix, "R"),
            trim_embed_reason(&record.reason),
        );

        let current_len: usize = lines.iter().map(String::len).sum();
        if current_len + item.len() > ACTIVE_VACATION_DESCRIPTION_BUDGET && !lines.is_empty() {
            break;
        }
        lines.push(item.trim_start_matches("\n\n").to_owned());
    }

    let truncated_count = active.len().saturating_sub(lines.len());
    (lines, truncated_count)
}

pub fn active_panel_description(records: &[VacationRecord]) -> String {
    let active_count = records
        .iter()
        .filter(|record| record.status == VacationStatus::Active)
        .count();
    let (lines, truncated_count) = active_panel_lines_with_truncation(records);
    if lines.is_empty() {
        return ACTIVE_PANEL_EMPTY.to_owned();
    }

    let mut description = String::new();
    description.push_str(&format!("Сейчас в отпуске: {active_count}"));
    description.push_str("\n\n");
    description.push_str(&lines.join("\n\n"));
    if truncated_count > 0 {
        description.push_str(&format!(
            "\n\nПоказаны первые {} отпусков из {}.",
            lines.len(),
            active_count
        ));
    }
    description
}

pub fn discord_timestamp(unix: i64, style: &str) -> String {
    format!("<t:{unix}:{style}>")
}

pub fn trim_embed_reason(reason: &str) -> String {
    let reason = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    if reason.is_empty() {
        return NO_REASON.to_owned();
    }
    let runes = reason.chars().collect::<Vec<_>>();
    if runes.len() <= ACTIVE_VACATION_REASON_LIMIT {
        return reason;
    }
    runes[..ACTIVE_VACATION_REASON_LIMIT]
        .iter()
        .collect::<String>()
        + "..."
}
