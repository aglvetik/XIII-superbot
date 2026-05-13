use crate::repository::RecruitRecord;
use chrono::{DateTime, Utc};

pub const LEGACY_DECISION_COLOR: u32 = 0xF1C40F;
pub const LEGACY_ACCEPT_COLOR: u32 = 0x2ECC71;
pub const LEGACY_REJECT_COLOR: u32 = 0xE74C3C;
pub const LEGACY_EXTEND_COLOR: u32 = 0xE67E22;

pub const ACCEPT_BUTTON_LABEL: &str = "✅ Принять";
pub const REJECT_BUTTON_LABEL: &str = "❌ Отклонить";
pub const EXTEND_BUTTON_LABEL: &str = "⏳ Продлить стажировку";
pub const REJECT_MODAL_TITLE: &str = "Отклонение стажировки";
pub const REJECT_REASON_LABEL: &str = "Причина отклонения";
pub const EXTEND_MODAL_TITLE: &str = "Продление стажировки";
pub const EXTEND_DAYS_LABEL: &str = "Количество дней";
pub const EXTEND_REASON_LABEL: &str = "Причина продления";

pub const DECISION_PANEL_TITLE: &str = "🎖️ Решение по стажировке";
pub const DECISION_PREVIEW_DESCRIPTION: &str = "Стажёр / Discord ID / Статус";
pub const DECISION_FOOTER_PREVIEW: &str = "ID стажировки: {id}";

pub const ACCEPT_SUCCESS: &str = "✅ Рекрут принят в основной состав.";
pub const REJECT_SUCCESS: &str = "✅ Стажировка отклонена.";
pub const EXTEND_SUCCESS: &str = "✅ Стажировка продлена.";

pub const ACCEPT_DM_TITLE: &str = "🎀 Добро пожаловать в основной состав XIII Legion!";
pub const REJECT_DM_TITLE: &str = "⚠️ Стажировка не пройдена";
pub const EXTEND_DM_TITLE: &str = "⏳ Стажировка продлена";

pub const FIELD_DECISION: &str = "Решение";
pub const FIELD_EXTENSION: &str = "Продление";
pub const FIELD_REASON: &str = "Причина";
pub const FIELD_WARNINGS: &str = "Предупреждения";
pub const FIELD_DEADLINES: &str = "Сроки";
pub const FIELD_VOICE: &str = "Голос";
pub const FIELD_EXTENSIONS: &str = "Продлений";
pub const FIELD_IMPORTANT: &str = "Важно";
pub const REJECT_DM_FOOTER: &str = "С уважением, XIII Legion";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitEmbedDraft {
    pub title: String,
    pub description: Option<String>,
    pub fields: Vec<RenderField>,
    pub color: u32,
    pub footer: Option<String>,
}

pub fn decision_panel_title(_recruit: &crate::state::Recruit) -> String {
    DECISION_PANEL_TITLE.to_owned()
}

pub fn decision_panel_embed(recruit: &RecruitRecord, voice_seconds: i64) -> RecruitEmbedDraft {
    RecruitEmbedDraft {
        title: DECISION_PANEL_TITLE.to_owned(),
        description: Some(recruit_summary_description(recruit)),
        fields: recruit_summary_fields(recruit, voice_seconds),
        color: LEGACY_DECISION_COLOR,
        footer: Some(decision_footer(recruit.id)),
    }
}

pub fn processed_decision_embed(
    recruit: &RecruitRecord,
    voice_seconds: i64,
    decision_label: &str,
    admin_id: u64,
    color: u32,
    reason: Option<&str>,
    extension_days: Option<i64>,
    warnings: &[String],
) -> RecruitEmbedDraft {
    let mut fields = recruit_summary_fields(recruit, voice_seconds);
    fields.push(RenderField {
        name: FIELD_DECISION.to_owned(),
        value: format!("{decision_label}\nОбработал: <@{admin_id}>"),
        inline: false,
    });
    if let Some(days) = extension_days {
        fields.push(RenderField {
            name: FIELD_EXTENSION.to_owned(),
            value: format!("{days} дн."),
            inline: true,
        });
    }
    if let Some(reason) = reason.filter(|value| !value.trim().is_empty()) {
        fields.push(RenderField {
            name: FIELD_REASON.to_owned(),
            value: reason.trim().to_owned(),
            inline: false,
        });
    }
    if !warnings.is_empty() {
        fields.push(RenderField {
            name: FIELD_WARNINGS.to_owned(),
            value: warnings.join("\n"),
            inline: false,
        });
    }
    RecruitEmbedDraft {
        title: DECISION_PANEL_TITLE.to_owned(),
        description: Some(recruit_summary_description(recruit)),
        fields,
        color,
        footer: Some(decision_footer(recruit.id)),
    }
}

pub fn accepted_dm_embed() -> RecruitEmbedDraft {
    RecruitEmbedDraft {
        title: ACCEPT_DM_TITLE.to_owned(),
        description: Some(
            "👋 Привет, боец!\n\
Твоя стажировка в клане XIII Legion закончилась.\n\
🎀 Поздравляю, теперь ты в основном составе.\n\
Поменяй префикс в Squad на ✧︎XIII✧︎\n\
🚀 Проявляй активность и участвуй в жизни клана!"
                .to_owned(),
        ),
        fields: Vec::new(),
        color: LEGACY_ACCEPT_COLOR,
        footer: None,
    }
}

pub fn rejected_dm_embed(reason: &str) -> RecruitEmbedDraft {
    RecruitEmbedDraft {
        title: REJECT_DM_TITLE.to_owned(),
        description: Some(
            "Привет.\nК сожалению, ты не прошёл стажировку в XIII Legion.".to_owned(),
        ),
        fields: vec![
            RenderField {
                name: FIELD_REASON.to_owned(),
                value: reason.trim().to_owned(),
                inline: false,
            },
            RenderField {
                name: FIELD_IMPORTANT.to_owned(),
                value: "Теперь ты не имеешь права играть в Squad с префиксом клана и говорить где-либо, что являешься его участником.".to_owned(),
                inline: false,
            },
        ],
        color: LEGACY_REJECT_COLOR,
        footer: Some(REJECT_DM_FOOTER.to_owned()),
    }
}

pub fn extended_dm_embed(days: i64, reason: &str) -> RecruitEmbedDraft {
    RecruitEmbedDraft {
        title: EXTEND_DM_TITLE.to_owned(),
        description: Some(format!(
            "👋 Привет, стажёр!\nТебе продлили испытательный срок на {days} дн.\nПродолжай проявлять активность и участвуй в жизни клана."
        )),
        fields: vec![RenderField {
            name: FIELD_REASON.to_owned(),
            value: reason.trim().to_owned(),
            inline: false,
        }],
        color: LEGACY_EXTEND_COLOR,
        footer: None,
    }
}

pub fn accept_decision_label() -> &'static str {
    "✅ Принят"
}

pub fn reject_decision_label() -> &'static str {
    "❌ Отклонён"
}

pub fn extend_decision_label() -> &'static str {
    "⏳ Стажировка продлена"
}

fn recruit_summary_description(recruit: &RecruitRecord) -> String {
    format!(
        "Стажёр: <@{}>\nDiscord ID: `{}`\nСтатус: {}",
        recruit.user_id,
        recruit.user_id,
        status_label(&recruit.status)
    )
}

fn recruit_summary_fields(recruit: &RecruitRecord, voice_seconds: i64) -> Vec<RenderField> {
    vec![
        RenderField {
            name: FIELD_DEADLINES.to_owned(),
            value: format!(
                "Начало: {}\nДо: {}\nДлительность: {}",
                format_datetime_ru(&recruit.started_at),
                format_datetime_ru(&recruit.due_at),
                format_probation_length(&recruit.started_at, &recruit.due_at)
            ),
            inline: false,
        },
        RenderField {
            name: FIELD_VOICE.to_owned(),
            value: format_voice_duration(voice_seconds),
            inline: true,
        },
        RenderField {
            name: FIELD_EXTENSIONS.to_owned(),
            value: recruit.extensions_count.to_string(),
            inline: true,
        },
    ]
}

fn decision_footer(recruit_id: i64) -> String {
    format!("ID стажировки: {recruit_id}")
}

fn status_label(status: &str) -> String {
    match status {
        "active" => "Активная стажировка".to_owned(),
        "accepted" => "Принят в основной состав".to_owned(),
        "rejected" => "Отклонён".to_owned(),
        "extended" => "Активная стажировка".to_owned(),
        _ => status.to_owned(),
    }
}

fn format_datetime_ru(value: &str) -> String {
    parse_time(value)
        .map(|parsed| parsed.format("%d.%m.%Y %H:%M UTC").to_string())
        .unwrap_or_else(|| value.to_owned())
}

fn format_probation_length(started_at: &str, due_at: &str) -> String {
    let Some(started) = parse_time(started_at) else {
        return "не удалось определить".to_owned();
    };
    let Some(due) = parse_time(due_at) else {
        return "не удалось определить".to_owned();
    };
    let total_seconds = (due.timestamp() - started.timestamp()).max(0);
    let days = total_seconds / 86_400;
    let remainder = total_seconds % 86_400;
    if days > 0 && remainder == 0 {
        return format!("{days} {}", plural_ru(days, "день", "дня", "дней"));
    }
    format_duration_seconds(total_seconds)
}

fn format_voice_duration(seconds: i64) -> String {
    let total_minutes = seconds.max(0) / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{hours} ч {minutes} мин")
}

fn format_duration_seconds(seconds: i64) -> String {
    let safe_seconds = seconds.max(0);
    let days = safe_seconds / 86_400;
    let hours = (safe_seconds % 86_400) / 3_600;
    let minutes = (safe_seconds % 3_600) / 60;
    if days > 0 {
        format!("{days} дн. {hours} ч.")
    } else if hours > 0 {
        format!("{hours} ч {minutes} мин")
    } else {
        format!("{minutes} мин")
    }
}

fn plural_ru<'a>(value: i64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let remainder_10 = value.rem_euclid(10);
    let remainder_100 = value.rem_euclid(100);
    if remainder_10 == 1 && remainder_100 != 11 {
        one
    } else if (2..=4).contains(&remainder_10) && !(12..=14).contains(&remainder_100) {
        few
    } else {
        many
    }
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}
