// `setup::setup` mirrors the test helper layout; renaming would ripple through
// every test, so silence the module_inception lint here.
#[allow(clippy::module_inception)]
pub mod setup;
pub mod setup_constants;