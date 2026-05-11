use thiserror::Error;
use xiii_core::{ModuleManifest, SchedulerJobDescriptor};

pub mod jobs;
pub mod non_overlap;
pub mod run_loop;

#[derive(Debug, Error)]
pub enum SchedulerPlanError {
    #[error("scheduler execution is disabled in scaffold mode")]
    ExecutionDisabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonOverlapPolicy {
    SkipIfRunning,
    QueueOne,
    ForbidDuplicateProcess,
}

#[derive(Debug, Clone, Default)]
pub struct SchedulerRegistry {
    jobs: Vec<SchedulerJobDescriptor>,
}

impl SchedulerRegistry {
    pub fn from_manifests(manifests: &[ModuleManifest]) -> Self {
        Self {
            jobs: manifests
                .iter()
                .flat_map(|manifest| manifest.scheduler_jobs.clone())
                .collect(),
        }
    }

    pub fn jobs(&self) -> &[SchedulerJobDescriptor] {
        &self.jobs
    }
}
