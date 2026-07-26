//! External adapters. Concrete database, cache, file, network, and system integrations live here.

pub mod database {
    #[derive(Debug, Default)]
    pub struct DatabaseAdapter;
}

pub mod cache {
    #[derive(Debug, Default)]
    pub struct CacheAdapter;
}

pub mod filesystem {
    #[derive(Debug, Default)]
    pub struct FileSystemAdapter;
}

pub mod network {
    #[derive(Debug, Default)]
    pub struct NetworkAdapter;
}

pub mod system {
    #[derive(Debug, Default)]
    pub struct SystemAdapter;
}
