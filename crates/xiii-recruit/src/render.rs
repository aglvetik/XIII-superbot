use crate::state::Recruit;

pub const LEGACY_DECISION_COLOR: u32 = 0xF1C40F;
pub const ACCEPT_BUTTON_LABEL: &str = "✅ Принять";
pub const REJECT_BUTTON_LABEL: &str = "❌ Отклонить";
pub const EXTEND_BUTTON_LABEL: &str = "⏳ Продлить стажировку";
pub const REJECT_MODAL_TITLE: &str = "Отклонение стажировки";
pub const REJECT_REASON_LABEL: &str = "Причина отклонения";
pub const EXTEND_MODAL_TITLE: &str = "Продление стажировки";
pub const EXTEND_DAYS_LABEL: &str = "Количество дней";
pub const EXTEND_REASON_LABEL: &str = "Причина продления";
pub const DECISION_PANEL_TITLE: &str = "🎖️ Решение по стажировке";
pub const DECISION_PREVIEW_DESCRIPTION: &str = "Стажёр / Discord ID / Статус / Срок до";
pub const DECISION_FOOTER_PREVIEW: &str = "ID стажировки: {id}";
pub const ACCEPT_SUCCESS: &str = "✅ Рекрут принят в основной состав.";
pub const REJECT_SUCCESS: &str = "✅ Стажировка отклонена.";
pub const EXTEND_SUCCESS: &str = "✅ Стажировка продлена.";
pub const ACCEPT_DM_TITLE: &str = "🎀 Добро пожаловать в основной состав XIII Legion!";
pub const REJECT_DM_TITLE: &str = "⚠️ Стажировка не пройдена";
pub const EXTEND_DM_TITLE: &str = "⏳ Стажировка продлена";

pub fn decision_panel_title(recruit: &Recruit) -> String {
    let _ = recruit;
    DECISION_PANEL_TITLE.to_owned()
}
