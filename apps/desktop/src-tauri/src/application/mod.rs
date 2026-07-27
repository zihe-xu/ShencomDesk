//! Use-case orchestration. Application services coordinate domain behavior.

pub mod config_repository;
pub mod config_service;
pub mod event_bus;
pub mod file_service;
pub mod plugin_service;
pub mod task_service;
pub mod update_service;

pub mod health_service {
    use crate::domain::health::HealthStatus;

    pub struct HealthService;

    impl HealthService {
        pub fn check(uptime_seconds: u64) -> HealthStatus {
            HealthStatus::ready(uptime_seconds)
        }
    }
}

pub mod user_service {
    pub struct UserService;
}

pub mod sync_service {
    pub struct SyncService;
}
