pub use crate::{RedactedConfigEntry, SecretStatus, SecretValue};

pub fn is_secret_like_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.contains("TOKEN")
        || upper.contains("SECRET")
        || upper.contains("PASSWORD")
        || upper.contains("PRIVATE_KEY")
        || upper.contains("WEBHOOK")
        || upper.contains("CREDENTIAL")
}
