//! TendKit's per-user files: the data directory, the latest-version cache, the
//! window size, app settings, and the OpenCommit files TendKit edits.

mod app_settings;
mod fs_util;
pub mod latest_cache;
pub mod opencommit_config;
pub mod paths;
pub mod shell_context;
pub mod window_size;

pub use app_settings::AppSettings;
pub use latest_cache::LatestVersionCache;
pub use opencommit_config::OpenCommitConfig;
pub use window_size::WindowSize;
