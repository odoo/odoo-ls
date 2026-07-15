//! Live inspector window (`--gui`) for exploring the server's entry points, symbol tree, and file infos.

mod app;
mod snapshot;

pub use app::run;
