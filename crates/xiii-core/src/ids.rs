#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiscordSnowflake(pub u64);

impl DiscordSnowflake {
    pub fn parse(value: &str) -> Result<Self, String> {
        let parsed = value
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("invalid Discord snowflake: {value}"))?;
        if parsed == 0 {
            Err("Discord snowflake must not be zero".to_owned())
        } else {
            Ok(Self(parsed))
        }
    }
}
