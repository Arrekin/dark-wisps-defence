//! # Log annotations
//!
//! `#[log_tags]` (from `lib-derive`) expands `<level>_<audience>` annotations inside a
//! function. The `!` forms below exist only to reject use outside such a function: inside
//! one, `#[log_tags]` rewrites them before they are resolved.

#[macro_export]
macro_rules! debug_dev { ($($arguments:tt)*) => { compile_error!("`debug_dev!` works only inside a `#[log_tags]` function") }; }
#[macro_export]
macro_rules! debug_player { ($($arguments:tt)*) => { compile_error!("`debug_player!` works only inside a `#[log_tags]` function") }; }
#[macro_export]
macro_rules! info_dev { ($($arguments:tt)*) => { compile_error!("`info_dev!` works only inside a `#[log_tags]` function") }; }
#[macro_export]
macro_rules! info_player { ($($arguments:tt)*) => { compile_error!("`info_player!` works only inside a `#[log_tags]` function") }; }
#[macro_export]
macro_rules! warn_dev { ($($arguments:tt)*) => { compile_error!("`warn_dev!` works only inside a `#[log_tags]` function") }; }
#[macro_export]
macro_rules! warn_player { ($($arguments:tt)*) => { compile_error!("`warn_player!` works only inside a `#[log_tags]` function") }; }
#[macro_export]
macro_rules! error_dev { ($($arguments:tt)*) => { compile_error!("`error_dev!` works only inside a `#[log_tags]` function") }; }
#[macro_export]
macro_rules! error_player { ($($arguments:tt)*) => { compile_error!("`error_player!` works only inside a `#[log_tags]` function") }; }

/// Runs every annotation shape through a live `LoggingPlugin` and checks which branch logged.
/// `LoggingPlugin` connects a process-wide sender only once, so all shapes share one test.
#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use bevy::prelude::*;

    use crate::{LogBuffer, LogLevel::{self, *}, LoggingPlugin, prelude::*};

    #[log_tags(Tag::GameLoad)]
    fn let_else(value: Option<i32>) -> i32 {
        #[warn_dev("let-else failed")]
        let Some(number) = value else { return -1 };
        number
    }

    #[log_tags(Tag::Build)]
    fn guard_if(paid: bool) -> Option<i32> {
        #[info_player("guard failed")]
        if !paid { return None; }
        Some(1)
    }

    #[log_tags(Tag::Forge)]
    fn action_if(done: bool) -> i32 {
        let mut stored = 0;
        #[info_player("action finished {item}")]
        if done { let item = 7; stored = item; }
        stored
    }

    #[log_tags(Tag::Wave)]
    fn arm_value(kind: &str) -> Option<i32> {
        match kind {
            "known" => Some(1),
            #[warn_dev("unknown arm {other}")]
            other => { let _ = other; None }
        }
    }

    #[log_tags(Tag::Wave)]
    fn arm_continue(items: &[&str]) -> i32 {
        let mut count = 0;
        for item in items {
            let value = match *item {
                "ok" => 1,
                #[warn_dev("skipped {item}")]
                _ => continue,
            };
            count += value;
        }
        count
    }

    #[log_tags(Tag::GameSave)]
    fn question_mark(value: Result<i32, String>) -> Result<i32, String> {
        #[info_dev("question mark passed {number}")]
        let number = value?;
        Ok(number)
    }

    #[log_tags(Tag::GameSave)]
    fn nested_question_marks(value: Option<Option<i32>>) -> Option<i32> {
        #[info_dev("nested question marks passed")]
        let number = value??;
        Some(number)
    }

    #[log_tags(Tag::Ui)]
    fn after_let() -> usize {
        #[debug_dev("rows {}", rows.len())]
        let rows = [1, 2, 3];
        rows.len()
    }

    #[log_tags(Tag::Ui)]
    fn tail_value() -> i32 {
        #[info_dev("tail")]
        40 + 2
    }

    #[log_tags(Tag::Ui)]
    fn exit_statement(early: bool) -> i32 {
        if early {
            #[warn_dev("returning early")]
            return 1;
        }
        2
    }

    #[log_tags(Tag::Ui)]
    fn exit_with_question_mark(value: Result<i32, String>, early: bool) -> Result<i32, String> {
        if early {
            #[warn_dev("returning through question mark")]
            return Ok(value? + 1);
        }
        Ok(0)
    }

    #[log_tags(Tag::Ui)]
    fn exit_with_coercion(name: &String, early: bool) -> &str {
        if early {
            #[debug_dev("returning coerced {name}")]
            return name;
        }
        ""
    }

    #[log_tags(Tag::Ui, Tag::Wave)]
    fn escape_hatch(kind: u8) -> u8 {
        info_dev!("macro statement");
        match kind {
            0 => warn_dev!("macro arm"),
            _ => {}
        }
        kind
    }

    struct Holder;
    impl Holder {
        #[log_tags(Tag::Build)]
        fn method(&self, value: Option<i32>) -> i32 {
            #[warn_dev("method let-else")]
            let Some(number) = value else { return 0 };
            number
        }
    }

    fn logged(app: &mut App) -> Vec<(LogLevel, String)> {
        app.update();
        app.world().resource::<LogBuffer>().entries()
            .map(|entry| (entry.level, entry.to_string()))
            .collect()
    }

    #[test]
    fn annotations_log_on_the_branch_they_describe() {
        let mut app = App::new();
        app.add_plugins(LoggingPlugin);

        assert_eq!(let_else(Some(5)), 5);
        assert_eq!(guard_if(true), Some(1));
        assert_eq!(action_if(false), 0);
        assert_eq!(arm_value("known"), Some(1));
        assert_eq!(arm_continue(&["ok", "ok"]), 2);
        assert_eq!(question_mark(Ok(3)), Ok(3));
        assert_eq!(nested_question_marks(Some(Some(1))), Some(1));
        assert_eq!(exit_statement(false), 2);
        assert_eq!(exit_with_question_mark(Err("boom".into()), true), Err("boom".into()));
        assert_eq!(exit_with_coercion(&"ada".to_string(), false), "");
        assert_eq!(escape_hatch(1), 1);
        assert_eq!(Holder.method(Some(4)), 4);
        let expected = [
            (Info, "[Dev   ][GameSave] question mark passed 3"),
            (Info, "[Dev   ][GameSave] nested question marks passed"),
            (Info, "[Dev   ][Wave, Ui] macro statement"),
        ].map(|(level, line)| (level, line.to_string()));
        assert_eq!(logged(&mut app), expected);

        assert_eq!(let_else(None), -1);
        assert_eq!(guard_if(false), None);
        assert_eq!(action_if(true), 7);
        assert_eq!(arm_value("mystery"), None);
        assert_eq!(arm_continue(&["ok", "bad", "ok"]), 2);
        assert_eq!(question_mark(Err("boom".into())), Err("boom".into()));
        assert_eq!(nested_question_marks(Some(None)), None);
        assert_eq!(after_let(), 3);
        assert_eq!(tail_value(), 42);
        assert_eq!(exit_statement(true), 1);
        assert_eq!(exit_with_question_mark(Ok(1), true), Ok(2));
        assert_eq!(exit_with_coercion(&"ada".to_string(), true), "ada");
        assert_eq!(escape_hatch(0), 0);
        assert_eq!(Holder.method(None), 0);
        let expected = [
            (Warn,  "[Dev   ][GameLoad] let-else failed"),
            (Info,  "[Player][Build] guard failed"),
            (Info,  "[Player][Forge] action finished 7"),
            (Warn,  "[Dev   ][Wave] unknown arm mystery"),
            (Warn,  "[Dev   ][Wave] skipped bad"),
            (Debug, "[Dev   ][Ui] rows 3"),
            (Info,  "[Dev   ][Ui] tail"),
            (Warn,  "[Dev   ][Ui] returning early"),
            (Warn,  "[Dev   ][Ui] returning through question mark"),
            (Debug, "[Dev   ][Ui] returning coerced ada"),
            (Info,  "[Dev   ][Wave, Ui] macro statement"),
            (Warn,  "[Dev   ][Wave, Ui] macro arm"),
            (Warn,  "[Dev   ][Build] method let-else"),
        ].map(|(level, line)| (level, line.to_string()));
        assert_eq!(logged(&mut app)[3..], expected);

        let buffer = app.world().resource::<LogBuffer>();
        let stored = |text: &str| buffer.entries().map(|entry| &entry.message).find(|message| message == &text).unwrap();
        assert!(matches!(stored("guard failed"), Cow::Borrowed(_)));
        assert!(matches!(stored("action finished 7"), Cow::Owned(_)));
    }
}
