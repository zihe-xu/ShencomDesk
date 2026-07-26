use std::{future::Future, pin::Pin};

use crate::utils::AppError;

pub type ConfigRepositoryFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

/// Application-owned persistence boundary for configuration values.
///
/// Infrastructure adapters implement this trait without leaking SQLx or SQLite
/// details into application services.
pub trait ConfigRepository: Send + Sync {
    fn get<'a>(&'a self, key: &'a str) -> ConfigRepositoryFuture<'a, Option<String>>;

    fn set<'a>(&'a self, key: &'a str, value: &'a str) -> ConfigRepositoryFuture<'a, ()>;

    fn delete<'a>(&'a self, key: &'a str) -> ConfigRepositoryFuture<'a, ()>;
}
