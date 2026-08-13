pub mod activity_composition;
pub mod ai;
#[cfg(feature = "desktop")]
pub mod ai_executor;
pub mod ai_review;
pub mod app;
pub mod browser;
pub mod browser_watcher;
#[cfg(feature = "desktop")]
pub mod browser_watcher_server;
pub mod classifier;
pub mod db;
pub mod domain;
pub mod edition;
pub mod idle;
pub mod knowledge_graph;
pub mod legacy;
pub mod monitor;
pub mod monitor_continuity;
pub mod report;
pub mod segment_overlap;
pub mod trend_analysis;
pub mod trends;
pub mod work_ledger;

#[cfg(feature = "desktop")]
pub mod desktop;
#[cfg(all(feature = "desktop", target_os = "macos"))]
pub mod macos_collector;
#[cfg(all(feature = "desktop", target_os = "windows"))]
pub mod windows_collector;
