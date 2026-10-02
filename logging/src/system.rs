//! # Logging System
//!
//! Channel-based logging. [`Log`] is a RAII builder: configure via method
//! chaining, sent to the channel on drop. [`LogBuffer`] stores entries for
//! in-game use; only its drain system holds `ResMut<LogBuffer>`, all readers
//! use `Res<LogBuffer>`.
//!
//! ## Usage
//!
//! ```ignore
//! Log::info().dev().tag(Tag::GameLoad).message("EntityMap populated");
//!
//! Log::warn().player()
//!     .tags([Tag::Wave, Tag::Resources])
//!     .message(format!("Wave {} started with {} enemies", wave, count));
//! ```
//!
//! Inside functions, logs tied to a statement read better as annotations: `#[log_tags]`
//! sets the tags once and `#[warn_dev("...")]`-style attributes mark the statements that
//! log. Its documentation lists what each annotated statement shape expands to.
//!
//! Changes to `#[log_tags]` (in `lib-derive`) must keep `cargo test -p lib-derive -p logging`
//! passing: `lib-derive` checks each shape's expansion, `logging` checks which branch logs.

use std::{borrow::Cow, collections::VecDeque, fmt, sync::OnceLock, time::SystemTime};

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use strum::{AsRefStr, EnumCount, EnumIter, IntoEnumIterator};

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
            .add_systems(PreUpdate, LogBuffer::drain_system);
    }
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Developer => "Dev   ",
            Self::Player    => "Player",
        })
    }
}

// ── Tag ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumCount, AsRefStr)]
pub enum Tag {
    GameLoad,
    GameSave,
    Build,
    Wave,
    Ui,
    Editor,
    Objectives,
    Resources,
    Forge,
    Research,
    Byoaic,
    Units,
    Shards,
    MapObjects,
}

// ── TagSet ────────────────────────────────────────────────────────────────────

const _: () = assert!(Tag::COUNT <= u32::BITS as usize, "TagSet holds one bit per Tag in a u32: widen it");

/// A set of [`Tag`]s, one bit per variant: it never allocates, and adding a tag twice
/// keeps one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TagSet(u32);
impl TagSet {
    fn bit(tag: Tag) -> u32 {
        1 << tag as u32
    }

    pub fn insert(&mut self, tag: Tag) {
        self.0 |= Self::bit(tag);
    }

    pub fn remove(&mut self, tag: Tag) {
        self.0 &= !Self::bit(tag);
    }

    pub fn contains(self, tag: Tag) -> bool {
        self.0 & Self::bit(tag) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether the two sets share at least one tag.
    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// Tags in declaration order.
    pub fn iter(self) -> impl Iterator<Item = Tag> {
        Tag::iter().filter(move |tag| self.contains(*tag))
    }
}
impl Extend<Tag> for TagSet {
    fn extend<I: IntoIterator<Item = Tag>>(&mut self, tags: I) {
        for tag in tags {
            self.insert(tag);
        }
    }
}
/// `Tag, Tag` in declaration order, the order [`Tag::iter`] yields.
impl fmt::Display for TagSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, tag) in self.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            formatter.write_str(tag.as_ref())?;
        }
        Ok(())
    }
}
impl FromIterator<Tag> for TagSet {
    fn from_iter<I: IntoIterator<Item = Tag>>(tags: I) -> Self {
        let mut set = Self::default();
        set.extend(tags);
        set
    }
}

// ── LogEntryData ──────────────────────────────────────────────────────────────

pub struct LogEntryData {
    pub level: LogLevel,
    pub audience: Audience,
    pub tags: TagSet,
    pub message: Cow<'static, str>,
    pub timestamp: SystemTime,
}
impl LogEntryData {
    fn emit_to_bevy_log(&self) {
        match self.level {
            LogLevel::Debug => bevy::log::debug!("{self}"),
            LogLevel::Info  => bevy::log::info!("{self}"),
            LogLevel::Warn  => bevy::log::warn!("{self}"),
            LogLevel::Error => bevy::log::error!("{self}"),
        }
    }
}
/// `[Audience][Tag, Tag] message`. The level is left out: `bevy::log` prints it.
impl fmt::Display for LogEntryData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}]", self.audience)?;
        if !self.tags.is_empty() {
            write!(formatter, "[{}]", self.tags)?;
        }
        write!(formatter, " {}", self.message)
    }
}

// ── LogMessage ────────────────────────────────────────────────────────────────

/// Anything [`Log::message`] accepts. A message known at compile time — a `&'static str`,
/// or `format_args!` without placeholders — is stored borrowed, without allocating.
pub struct LogMessage(Cow<'static, str>);
impl From<&'static str> for LogMessage {
    fn from(message: &'static str) -> Self { Self(Cow::Borrowed(message)) }
}
impl From<String> for LogMessage {
    fn from(message: String) -> Self { Self(Cow::Owned(message)) }
}
impl From<Cow<'static, str>> for LogMessage {
    fn from(message: Cow<'static, str>) -> Self { Self(message) }
}
impl From<fmt::Arguments<'_>> for LogMessage {
    fn from(arguments: fmt::Arguments<'_>) -> Self {
        match arguments.as_str() {
            Some(literal) => Self(Cow::Borrowed(literal)),
            None => Self(Cow::Owned(arguments.to_string())),
        }
    }
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
                tags: TagSet::default(),
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
        if let Some(entry) = &mut self.inner {
            entry.audience = Audience::Developer;
        }
        self
    }

    pub fn player(mut self) -> Self {
        if let Some(entry) = &mut self.inner {
            entry.audience = Audience::Player;
        }
        self
    }

    /// Accepts `&'static str`, `String`, `Cow<'static, str>` and `format_args!(..)`.
    /// `format_args!` allocates only when the message has something to format.
    pub fn message(mut self, message: impl Into<LogMessage>) -> Self {
        if let Some(entry) = &mut self.inner {
            entry.message = message.into().0;
        }
        self
    }

    pub fn tag(mut self, tag: Tag) -> Self {
        if let Some(entry) = &mut self.inner {
            entry.tags.insert(tag);
        }
        self
    }

    pub fn tags(mut self, tags: impl IntoIterator<Item = Tag>) -> Self {
        if let Some(entry) = &mut self.inner {
            entry.tags.extend(tags);
        }
        self
    }
}
impl Drop for Log {
    fn drop(&mut self) {
        if let Some(entry) = self.inner.take() && let Some(sender) = LOG_SENDER.get() {
            sender.send(entry).ok();
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
            entry.emit_to_bevy_log();
            log_buffer.push(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Tag, TagSet};

    #[test]
    fn tag_set_behaves_as_a_set() {
        let mut tags: TagSet = [Tag::Wave, Tag::Build, Tag::Wave].into_iter().collect();
        assert_eq!(tags.iter().collect::<Vec<_>>(), [Tag::Build, Tag::Wave]);
        assert!(tags.contains(Tag::Wave) && !tags.contains(Tag::Ui));

        tags.remove(Tag::Wave);
        assert_eq!(tags.iter().collect::<Vec<_>>(), [Tag::Build]);
        assert!(tags.intersects([Tag::Build, Tag::Ui].into_iter().collect()));
        assert!(!tags.intersects([Tag::Ui].into_iter().collect()));
        assert!(!tags.intersects(TagSet::default()));

        tags.remove(Tag::Build);
        assert!(tags.is_empty());
    }
}
