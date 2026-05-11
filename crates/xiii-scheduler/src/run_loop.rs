#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerLoopPlan {
    pub job_name: String,
    pub interval_seconds: u64,
    pub non_overlap: bool,
}
