#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketRuntimeConfig {
    pub panel_channel_id: u64,
    pub open_category_id: u64,
    pub transcript_channel_id: u64,
    pub support_role_id: u64,
    pub google_poll_seconds: u64,
}
