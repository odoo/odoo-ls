pub mod allocator;

use std::sync::OnceLock;
pub static TIMING_DOWNCASTER: OnceLock<tracing_timing::LayerDowncaster<tracing_timing::group::ByName, tracing_timing::group::ByMessage>> = OnceLock::new();
pub mod args;
pub mod cli_backend;
pub mod constants;
pub mod core;
pub mod threads;
pub mod features;
pub mod fifo_ptr_weak_hash_set;
pub mod server;
pub mod tasks;
pub mod utils;
pub mod crash_buffer;