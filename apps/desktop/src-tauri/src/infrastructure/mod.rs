//! External adapters. Concrete database, cache, file, logging, network, and system integrations live here.

pub mod auth;
pub mod database;
pub mod filesystem;
pub mod logging;
pub mod plugins;
pub mod updater;

pub mod cache {
    #[derive(Debug, Default)]
    pub struct CacheAdapter;
}

pub mod network {
    #[derive(Debug, Default)]
    pub struct NetworkAdapter;
}

pub mod system {
    #[derive(Debug, Default)]
    pub struct SystemAdapter;
}
