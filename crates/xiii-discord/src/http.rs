#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordHttpSafety {
    pub reads_enabled: bool,
    pub writes_enabled: bool,
    pub allowed_mentions_disabled_by_default: bool,
}

impl DiscordHttpSafety {
    pub fn read_only() -> Self {
        Self {
            reads_enabled: true,
            writes_enabled: false,
            allowed_mentions_disabled_by_default: true,
        }
    }
}
