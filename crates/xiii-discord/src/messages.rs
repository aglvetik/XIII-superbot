#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordMessageTarget {
    pub channel_id: u64,
    pub message_id: u64,
}
