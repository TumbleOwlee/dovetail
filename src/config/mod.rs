//! Configuration: user-level profiles and repository entries, the repository-level file, and
//! how the active repository's settings are resolved and written.

pub mod error;
pub mod origin;
pub mod paths;
pub mod profile;
pub mod schema;
pub mod store;

pub use error::ConfigError;
pub use origin::Origin;
pub use schema::{Board, Kind, Profile, Remote, Section, UserConfig};
pub use store::{Settings, Source};
