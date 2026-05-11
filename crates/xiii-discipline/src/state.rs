#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PunishmentType {
    Warning,
    Verbal,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PunishmentStatus {
    Active,
    Expired,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Punishment {
    pub id: i64,
    pub user_id: u64,
    pub kind: PunishmentType,
    pub status: PunishmentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionLock {
    pub key: String,
    pub expires_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscalationOutcome {
    Issue(PunishmentType),
    ClanRemoval,
}

pub fn action_lock_key(action: &str, user_id: u64) -> String {
    format!("{action}:{user_id}")
}

pub fn action_lock_allows(locks: &[ActionLock], key: &str, now_unix: i64) -> bool {
    !locks
        .iter()
        .any(|lock| lock.key == key && lock.expires_unix > now_unix)
}
