//! Reads, merges, and resolves `odools.toml` configuration files into the
//! runtime [`ConfigEntry`]. The pipeline is fully spec-driven: every setting
//! is declared once as a `FieldSpec` row in `config_key_spec.rs`. For module
//! layout and the "add a field" contract, see `README.md`.

pub mod render;

mod build;
mod config_key_spec;
mod parse;
mod paths;
mod runtime;
mod schema_gen;
mod spec;
mod stages;
mod value;
mod version;

// Public API
pub use config_key_spec::ConfigKey;
pub use render::ConfigView;
pub use runtime::{ConfigEntry, needs_restart};
pub use schema_gen::config_json_schema;
pub use value::{
    DEFAULT_PROFILE_NAME, DiagMissingImportsMode, DiagnosticFilter, DiagnosticFilterPathType,
};

use crate::threads::SessionInfo;
use crate::utils::HashMap;

/// The runtime configuration map (profile name → resolved config).
type ConfigMap = HashMap<String, ConfigEntry>;

/// Resolve configuration for the session's workspace folders + config file into
/// the runtime config map and the render `ConfigView`.
pub fn get_configuration(session: &mut SessionInfo) -> Result<(ConfigMap, ConfigView), String> {
    self::build::resolve(session)
}
