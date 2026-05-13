use crate::repository::DisciplinePunishmentRecord;
use crate::state::PunishmentType;

pub const LEGACY_BOARD_COLOR: u32 = 0x2F80ED;
pub const LEGACY_HISTORY_EMPTY_COLOR: u32 = 0x42B883;
pub const BOARD_TITLE: &str = "XIII — Активные наказания";
pub const HISTORY_TITLE: &str = "XIII — история наказаний";

pub const PANEL_ISSUE_LABEL: &str = "Выдать наказание";
pub const PANEL_REMOVE_LABEL: &str = "Снять наказание";
pub const PANEL_HISTORY_LABEL: &str = "История участника";
pub const BOARD_PREV_LABEL: &str = "Назад";
pub const BOARD_NEXT_LABEL: &str = "Вперед";
pub const PICKER_PREV_LABEL: &str = "◀ Назад";
pub const PICKER_NEXT_LABEL: &str = "▶ Вперёд";
pub const PICKER_ID_LABEL: &str = "🔍 Ввести ID/упоминание";
pub const PICKER_CANCEL_LABEL: &str = "Отмена";

pub const EMPTY_BOARD_DESCRIPTION: &str = "Активных наказаний нет.";
pub const EMPTY_HISTORY_TEMPLATE: &str = "<@{user_id}> не имеет записей в истории.";
pub const HISTORY_OVERSIZE_NOTE: &str = "История слишком большая, показана первая часть.";

pub const BOARD_SUMMARY_PREVIEW: &str =
    "Участников с наказаниями: N\nПредупреждений: N · Устных: N · Строгих: N";
pub const BOARD_FOOTER_PREVIEW: &str = "Страница N/M • Обновлено {ru-RU}";

pub const ISSUE_TARGET_PROMPT: &str = "Выбери участника, которому нужно выдать наказание.";
pub const REMOVE_TARGET_PROMPT: &str = "Выбери участника, у которого нужно снять наказание.";
pub const HISTORY_TARGET_PROMPT: &str = "Выбери участника, историю которого нужно посмотреть.";
pub const ISSUE_PICKER_CONTENT: &str =
    "Выбери участника активного состава XIII. Показаны только участники с ролью состава, без ботов.";
pub const ISSUE_PICKER_EMPTY_CONTENT: &str =
    "В активном составе XIII нет участников для выбора. Можно ввести ID или упоминание вручную.";
pub const USER_SELECT_PLACEHOLDER: &str = "Выбери участника";
pub const ISSUE_PICKER_PLACEHOLDER_PREFIX: &str = "Активный состав XIII";
pub const ISSUE_TYPE_PLACEHOLDER: &str = "Выбери тип наказания";
pub const ISSUE_TYPE_CONTENT_TEMPLATE: &str =
    "Участник: <@{user_id}>. Теперь выбери тип наказания.";
pub const ISSUE_SESSION_EXPIRED: &str = "Сессия выбора истекла. Открой выдачу наказания заново.";
pub const REMOVE_SESSION_EXPIRED: &str =
    "Эта панель больше не активна. Открой снятие наказания заново.";
pub const HISTORY_SESSION_EXPIRED: &str = "Эта панель больше не активна. Открой историю заново.";
pub const MEMBER_NOT_SELECTED: &str = "Участник не выбран.";
pub const INVALID_MEMBER_ID: &str = "Не удалось распознать участника. Вставь ID или упоминание.";
pub const INVALID_PUNISHMENT_ID: &str = "Не удалось распознать ID наказания.";
pub const INVALID_HISTORY_TARGET: &str = "Не удалось распознать участника для истории.";
pub const INVALID_ISSUE_TARGET: &str = "Не удалось распознать участника для выдачи наказания.";
pub const INVALID_REMOVE_TARGET: &str = "Не удалось распознать участника для снятия наказания.";
pub const INVALID_TYPE_TEXT: &str = "Неверный тип наказания. Используй warning, verbal или strict.";
pub const ACTION_IN_PROGRESS: &str = "Действие уже выполняется. Подожди несколько секунд.";
pub const ISSUE_CANCELLED: &str = "Выдача наказания отменена.";
pub const REMOVE_SELECT_PLACEHOLDER: &str = "Выбери наказание";
pub const REMOVE_SELECT_CONTENT_TEMPLATE: &str =
    "Участник: <@{user_id}>. Выбери конкретную запись для снятия.{suffix}";
pub const REMOVE_NO_ACTIVE_TEMPLATE: &str = "<@{user_id}> не имеет активных наказаний.";
pub const REMOVE_SUFFIX_TEMPLATE: &str = "\nПоказаны первые {shown} из {total} активных наказаний.";
pub const REMOVE_SUCCESS_TEMPLATE: &str = "Наказание #{punishment_id} ({kind}) снято.";

pub const ISSUE_ID_MODAL_TITLE: &str = "Ввести ID или упоминание";
pub const ISSUE_ID_MODAL_LABEL: &str = "ID или упоминание участника";
pub const ISSUE_REASON_LABEL: &str = "Причина";
pub const REMOVE_MODAL_TITLE: &str = "Снять наказание";
pub const REMOVE_REASON_LABEL: &str = "Причина снятия";
pub const PUNISHMENT_ID_LABEL: &str = "ID наказания";

pub const WARNING_LABEL: &str = "Предупреждение";
pub const VERBAL_LABEL: &str = "Устный выговор";
pub const STRICT_LABEL: &str = "Строгий выговор";

pub const STATUS_ACTIVE: &str = "Активно";
pub const STATUS_EXPIRED: &str = "Истекло";
pub const STATUS_CONVERTED: &str = "Конвертировано";
pub const STATUS_REMOVED: &str = "Снято вручную";
pub const SYSTEM_LABEL: &str = "Система";

pub const HISTORY_PAGE_LIMIT: usize = 3600;
pub const BOARD_DESCRIPTION_LIMIT: usize = 3600;

pub fn board_title() -> &'static str {
    BOARD_TITLE
}

pub fn punishment_type_label(kind: PunishmentType) -> &'static str {
    match kind {
        PunishmentType::Warning => WARNING_LABEL,
        PunishmentType::Verbal => VERBAL_LABEL,
        PunishmentType::Strict => STRICT_LABEL,
    }
}

pub fn punishment_status_label(status: &str) -> &str {
    match status {
        "active" => STATUS_ACTIVE,
        "expired" => STATUS_EXPIRED,
        "converted" => STATUS_CONVERTED,
        "manually_removed" => STATUS_REMOVED,
        _ => status,
    }
}

pub fn board_pages(rows: &[DisciplinePunishmentRecord]) -> Vec<String> {
    if rows.is_empty() {
        return vec![EMPTY_BOARD_DESCRIPTION.to_owned()];
    }

    let mut blocks = Vec::new();
    for kind in [
        PunishmentType::Strict,
        PunishmentType::Verbal,
        PunishmentType::Warning,
    ] {
        let typed_rows = rows
            .iter()
            .filter(|row| row.kind == kind)
            .collect::<Vec<_>>();
        if typed_rows.is_empty() {
            continue;
        }
        for (index, row) in typed_rows.iter().enumerate() {
            let active_count_for_level = typed_rows
                .iter()
                .filter(|other| other.user_id == row.user_id)
                .count();
            let entry = format_board_entry(row, active_count_for_level, index + 1);
            if index == 0 {
                blocks.push(format!("{}\n{}", section_title(kind), entry));
            } else {
                blocks.push(entry);
            }
        }
    }

    let mut pages = Vec::new();
    let mut current = String::new();
    for block in blocks {
        let next = if current.is_empty() {
            block.clone()
        } else {
            format!("{current}\n{block}")
        };
        if next.len() > BOARD_DESCRIPTION_LIMIT && !current.is_empty() {
            pages.push(current);
            current = block;
        } else {
            current = next;
        }
    }
    if !current.is_empty() {
        pages.push(current);
    }
    if pages.is_empty() {
        pages.push(EMPTY_BOARD_DESCRIPTION.to_owned());
    }
    pages
}

pub fn board_summary_text(rows: &[DisciplinePunishmentRecord]) -> String {
    let affected_members = rows
        .iter()
        .map(|row| row.user_id)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let warning = rows
        .iter()
        .filter(|row| row.kind == PunishmentType::Warning)
        .count();
    let verbal = rows
        .iter()
        .filter(|row| row.kind == PunishmentType::Verbal)
        .count();
    let strict = rows
        .iter()
        .filter(|row| row.kind == PunishmentType::Strict)
        .count();
    format!(
        "Участников с наказаниями: {affected_members}\nПредупреждений: {warning} · Устных: {verbal} · Строгих: {strict}"
    )
}

pub fn board_description(rows: &[DisciplinePunishmentRecord], page: usize) -> (String, usize) {
    let pages = board_pages(rows);
    let clamped_page = page.min(pages.len().saturating_sub(1));
    let summary = board_summary_text(rows);
    (format!("{summary}\n\n{}", pages[clamped_page]), pages.len())
}

pub fn board_footer(page: usize, total_pages: usize, updated_label: &str) -> String {
    format!(
        "Страница {} / {} • Обновлено {}",
        page + 1,
        total_pages.max(1),
        updated_label
    )
}

pub fn history_empty_description(user_id: u64) -> String {
    EMPTY_HISTORY_TEMPLATE.replace("{user_id}", &user_id.to_string())
}

pub fn history_pages(rows: &[DisciplinePunishmentRecord]) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }

    let mut pages = Vec::new();
    let mut current = String::new();
    for line in rows.iter().map(format_history_row) {
        let next = if current.is_empty() {
            line.clone()
        } else {
            format!("{current}\n\n{line}")
        };
        if next.len() > HISTORY_PAGE_LIMIT && !current.is_empty() {
            pages.push(current);
            current = line;
        } else {
            current = next;
        }
    }
    if !current.is_empty() {
        pages.push(current);
    }
    if pages.len() > 10 {
        let mut limited = pages.into_iter().take(10).collect::<Vec<_>>();
        if let Some(last) = limited.last_mut() {
            last.push_str("\n\n");
            last.push_str(HISTORY_OVERSIZE_NOTE);
        }
        return limited;
    }
    pages
}

pub fn history_footer(user_id: u64, page: usize, total_pages: usize) -> String {
    format!(
        "<@{user_id}> • Страница {} / {}",
        page + 1,
        total_pages.max(1)
    )
}

pub fn issue_picker_placeholder(page: usize, total_pages: usize) -> String {
    format!(
        "{ISSUE_PICKER_PLACEHOLDER_PREFIX} · страница {} / {}",
        page + 1,
        total_pages.max(1)
    )
}

pub fn issue_type_content(user_id: u64) -> String {
    ISSUE_TYPE_CONTENT_TEMPLATE.replace("{user_id}", &user_id.to_string())
}

pub fn remove_no_active_message(user_id: u64) -> String {
    REMOVE_NO_ACTIVE_TEMPLATE.replace("{user_id}", &user_id.to_string())
}

pub fn remove_selection_content(user_id: u64, shown: usize, total: usize) -> String {
    let suffix = if total > shown {
        REMOVE_SUFFIX_TEMPLATE
            .replace("{shown}", &shown.to_string())
            .replace("{total}", &total.to_string())
    } else {
        String::new()
    };
    REMOVE_SELECT_CONTENT_TEMPLATE
        .replace("{user_id}", &user_id.to_string())
        .replace("{suffix}", &suffix)
}

pub fn remove_success_message(punishment_id: i64, kind: PunishmentType) -> String {
    REMOVE_SUCCESS_TEMPLATE
        .replace("{punishment_id}", &punishment_id.to_string())
        .replace("{kind}", punishment_type_label(kind))
}

fn section_title(kind: PunishmentType) -> &'static str {
    match kind {
        PunishmentType::Strict => "**Строгие выговоры**",
        PunishmentType::Verbal => "**Устные выговоры**",
        PunishmentType::Warning => "**Предупреждения**",
    }
}

fn format_board_entry(
    row: &DisciplinePunishmentRecord,
    active_count_for_level: usize,
    entry_number: usize,
) -> String {
    let reason = truncate_text(&row.reason, 100);
    if row.kind == PunishmentType::Strict {
        return format!(
            "{}. <@{}> — {}/2 · выдано {}\nПричина: {}",
            entry_number,
            row.user_id,
            active_count_for_level,
            discord_date(row.issued_at),
            reason
        );
    }

    let expires = row
        .expires_at
        .map(discord_relative)
        .unwrap_or_else(|| "не истекает".to_owned());
    format!(
        "{}. <@{}> — выдано {} · срок: {}\nПричина: {}",
        entry_number,
        row.user_id,
        discord_date(row.issued_at),
        expires,
        reason
    )
}

fn format_history_row(row: &DisciplinePunishmentRecord) -> String {
    let expires = row
        .expires_at
        .map(discord_date_time)
        .unwrap_or_else(|| "Не истекает".to_owned());
    let converted = row
        .converted_into_id
        .map(|id| format!("\nКонвертировано в запись #{id}"))
        .unwrap_or_default();
    let removed = if row.status == "manually_removed" {
        format!(
            "\nСнято: {} • {}\nПричина снятия: {}",
            row.removed_at
                .map(discord_date_time)
                .unwrap_or_else(|| "неизвестно".to_owned()),
            user_label(row.removed_by_id),
            truncate_text(row.removed_reason.as_deref().unwrap_or("не указана"), 240)
        )
    } else {
        String::new()
    };
    format!(
        "#{} • **{}** • {}\nВыдано: {} • Истекает: {} • Выдал: {}\nПричина: {}{}{}",
        row.id,
        punishment_type_label(row.kind),
        punishment_status_label(&row.status),
        discord_date_time(row.issued_at),
        expires,
        user_label(row.issuer_id),
        truncate_text(&row.reason, 300),
        converted,
        removed
    )
}

fn user_label(user_id: Option<u64>) -> String {
    user_id
        .map(|id| format!("<@{id}>"))
        .unwrap_or_else(|| SYSTEM_LABEL.to_owned())
}

fn discord_date(unix: i64) -> String {
    format!("<t:{unix}:d>")
}

fn discord_date_time(unix: i64) -> String {
    format!("<t:{unix}:f>")
}

fn discord_relative(unix: i64) -> String {
    format!("<t:{unix}:R>")
}

fn truncate_text(input: &str, max_chars: usize) -> String {
    let trimmed = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars = trimmed.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        trimmed
    } else {
        chars[..max_chars].iter().collect::<String>() + "..."
    }
}
