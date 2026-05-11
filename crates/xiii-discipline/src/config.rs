#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisciplineRuntimeConfig {
    pub timeout_minutes: u64,
    pub warning_expires_days: u64,
    pub verbal_expires_days: u64,
    pub officer_role_ids: Vec<u64>,
}
