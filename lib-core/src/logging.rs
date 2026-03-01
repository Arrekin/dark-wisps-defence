//! # Logging System
//!
//! Channel-based logging. [`Log`] is a RAII builder: configure via method
//! chaining, sent to the channel on drop. [`LogBuffer`] stores entries for
//! in-game use; only its drain system holds `ResMut<LogBuffer>`, all readers
//! use `Res<LogBuffer>`.
//!
//! ## Usage
//!
//! ```rust
//! Log::info().dev().tag(Tag::GameLoading).message("EntityMap populated");
//!
//! Log::warn().player()
//!     .tags([Tag::Wave, Tag::Combat])
//!     .message(format!("Wave {} started with {} enemies", wave, count));
//! ```

use crossbeam_channel::{Receiver, Sender};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt;
use std::sync::OnceLock;
use std::time::SystemTime;

use crate::lib_prelude::*;

const MAX_LOG_ENTRIES: usize = 1000;

static LOG_SENDER: OnceLock<Sender<LogEntryData>> = OnceLock::new();

// ── Plugin ────────────────────────────────────────────────────────────────────

pub struct LoggingPlugin;
impl Plugin for LoggingPlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        LOG_SENDER.set(sender).ok();
        app
            .insert_resource(LogBuffer::new(receiver, MAX_LOG_ENTRIES))
            .add_systems(PreUpdate, LogBuffer::drain_system)
            ;
    }
}

// ── Prelude ───────────────────────────────────────────────────────────────────

pub mod logging_prelude {
    pub use super::{Audience, Log, LogBuffer, LogEntryData, LogLevel, Tag};
}

// ── LogLevel ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}
impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Debug => "DEBUG",
            Self::Info  => "INFO ",
            Self::Warn  => "WARN ",
            Self::Error => "ERROR",
        })
    }
}

// ── Audience ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    Developer,
    Player,
}
impl fmt::Display for Audience {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Developer => "Dev   ",
            Self::Player    => "Player",
        })
    }
}

// ── Tag ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tag {
    GameLoad,
    GameSave,
    Build,
    Wave,
    Ui,
    Editor,
}

// ── LogEntryData ──────────────────────────────────────────────────────────────

pub struct LogEntryData {
    pub level: LogLevel,
    pub audience: Audience,
    pub tags: HashSet<Tag>,
    pub message: Cow<'static, str>,
    pub timestamp: SystemTime,
}

// ── Log ───────────────────────────────────────────────────────────────────────

/// RAII log builder. Sends [`LogEntryData`] to the channel on drop.
///
/// Defaults to `Audience::Developer` if `.dev()` or `.player()` is not called.
pub struct Log {
    inner: Option<LogEntryData>,
}
impl Log {
    fn new(level: LogLevel) -> Self {
        Self {
            inner: Some(LogEntryData {
                level,
                audience: Audience::Developer,
                tags: HashSet::new(),
                message: Cow::Borrowed(""),
                timestamp: SystemTime::now(),
            }),
        }
    }

    pub fn debug() -> Self { Self::new(LogLevel::Debug) }
    pub fn info()  -> Self { Self::new(LogLevel::Info) }
    pub fn warn()  -> Self { Self::new(LogLevel::Warn) }
    pub fn error() -> Self { Self::new(LogLevel::Error) }

    pub fn dev(mut self) -> Self {
        if let Some(ref mut entry) = self.inner {
            entry.audience = Audience::Developer;
        }
        self
    }

    pub fn player(mut self) -> Self {
        if let Some(ref mut entry) = self.inner {
            entry.audience = Audience::Player;
        }
        self
    }

    /// Accepts both `&'static str` (zero allocation) and `String` (e.g. from `format!()`).
    pub fn message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        if let Some(ref mut entry) = self.inner {
            entry.message = message.into();
        }
        self
    }

    pub fn tag(mut self, tag: Tag) -> Self {
        if let Some(ref mut entry) = self.inner {
            entry.tags.insert(tag);
        }
        self
    }

    pub fn tags(mut self, iter: impl IntoIterator<Item = Tag>) -> Self {
        if let Some(ref mut entry) = self.inner {
            entry.tags.extend(iter);
        }
        self
    }
}
impl Drop for Log {
    fn drop(&mut self) {
        if let Some(entry) = self.inner.take() {
            if let Some(sender) = LOG_SENDER.get() {
                sender.send(entry).ok();
            }
        }
    }
}

// ── LogBuffer ─────────────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct LogBuffer {
    receiver: Receiver<LogEntryData>,
    entries: VecDeque<LogEntryData>,
    max_size: usize,
}
impl LogBuffer {
    fn new(receiver: Receiver<LogEntryData>, max_size: usize) -> Self {
        Self {
            receiver,
            entries: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = &LogEntryData> {
        self.entries.iter()
    }

    fn push(&mut self, entry: LogEntryData) {
        if self.entries.len() >= self.max_size {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    fn drain_system(mut log_buffer: ResMut<Self>) {
        while let Ok(entry) = log_buffer.receiver.try_recv() {
            emit_to_bevy_log(&entry);
            log_buffer.push(entry);
        }
    }
}

fn emit_to_bevy_log(entry: &LogEntryData) {
    let message = format_entry_for_display(entry);
    match entry.level {
        LogLevel::Debug => bevy::log::debug!("{}", message),
        LogLevel::Info  => bevy::log::info!("{}", message),
        LogLevel::Warn  => bevy::log::warn!("{}", message),
        LogLevel::Error => bevy::log::error!("{}", message),
    }
}

fn format_entry_for_display(entry: &LogEntryData) -> String {
    if entry.tags.is_empty() {
        format!("[{}] {}", entry.audience, entry.message)
    } else {
        let tags_display = entry.tags.iter()
            .map(|tag| format!("{tag:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{}][{}] {}", entry.audience, tags_display, entry.message)
    }
}
