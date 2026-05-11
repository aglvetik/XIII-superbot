#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacationRuntimeConfig {
    pub vacation_role_id: u64,
    pub officer_channel_id: u64,
    pub officer_ping_role_id: Option<u64>,
    pub max_days: u64,
}
