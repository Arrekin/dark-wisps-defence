mod system;

pub use system::{Audience, Log, LogBuffer, LogEntryData, LogLevel, LoggingPlugin, Tag};

/// The universal logging surface — only what (nearly) every file legitimately needs.
/// Specialized types (`LogBuffer`, `LogEntryData`, `Audience`, `LogLevel`) are imported
/// directly from the crate root by the few consumers that need them.
pub mod prelude {
    pub use crate::system::{Log, Tag};
}
