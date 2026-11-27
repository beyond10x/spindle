//! What `run` writes on stderr, in the format `--log-format` names, set once at start
//! (`story:structured-json-logs`).
//!
//! In text, the default, a line is written as it always was: `b10x-spindle: <message>`, or the
//! message alone where it never carried the prefix. In JSON each line is one object, `{"time",
//! "level", "message", ...}`: `time` the instant written, RFC 3339 in UTC; `level` `info`, `warn`
//! or `error`; `message` the text line without the prefix; and the line's named fields at the top
//! level (`component`, `command`, `binding`, `attempts`, `error`, `request_id`).

use std::io::Write;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use spindle_cli::cli::LogFormat;

/// The format of every line, set once at start; text until then.
static FORMAT: OnceLock<LogFormat> = OnceLock::new();

/// Sets the format of every line from here on, and in JSON a panic hook that writes the panic as
/// one line too. A second call changes nothing.
pub(crate) fn set(format: LogFormat) {
    if FORMAT.set(format).is_ok() && format == LogFormat::Json {
        std::panic::set_hook(Box::new(|panic| {
            let payload = panic.payload();
            let said = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("a panic without a message");
            let mut line = error(format!("panicked: {said}"));
            if let Some(location) = panic.location() {
                line = line.field("location", location.to_string());
            }
            line.write();
        }));
    }
}

/// The format set at start, text where none was.
fn format() -> LogFormat {
    FORMAT.get().copied().unwrap_or_default()
}

/// How much a line matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// One line on stderr, written by [`Line::write`].
#[must_use = "a line is written only by Line::write"]
pub(crate) struct Line {
    level: Level,
    message: String,
    fields: Map<String, Value>,
    prefixed: bool,
}

/// A line of `level` saying `message`, written in text after `b10x-spindle: `.
pub(crate) fn line(level: Level, message: impl Into<String>) -> Line {
    Line {
        level,
        message: message.into(),
        fields: Map::new(),
        prefixed: true,
    }
}

/// An `info` line.
pub(crate) fn info(message: impl Into<String>) -> Line {
    line(Level::Info, message)
}

/// A `warn` line.
pub(crate) fn warn(message: impl Into<String>) -> Line {
    line(Level::Warn, message)
}

/// An `error` line.
pub(crate) fn error(message: impl Into<String>) -> Line {
    line(Level::Error, message)
}

impl Line {
    /// The line with the field `name`, written in JSON only.
    pub(crate) fn field(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.fields.insert(name.to_owned(), value.into());
        self
    }

    /// The line written in text without the `b10x-spindle: ` prefix, as it always was.
    pub(crate) fn bare(mut self) -> Self {
        self.prefixed = false;
        self
    }

    /// Writes the line on stderr in the format set at start. A failed write is dropped: nothing
    /// is left to tell.
    pub(crate) fn write(self) {
        let text = self.render(format(), SystemTime::now());
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "{text}");
    }

    /// The line as `format` spells it, written at `now`.
    fn render(self, format: LogFormat, now: SystemTime) -> String {
        match format {
            LogFormat::Text if self.prefixed => format!("b10x-spindle: {}", self.message),
            LogFormat::Text => self.message,
            LogFormat::Json => {
                let mut object = Map::new();
                object.insert("time".to_owned(), Value::String(rfc3339(now)));
                object.insert(
                    "level".to_owned(),
                    Value::String(self.level.name().to_owned()),
                );
                object.insert("message".to_owned(), Value::String(self.message));
                for (name, value) in self.fields {
                    object.entry(name).or_insert(value);
                }
                Value::Object(object).to_string()
            }
        }
    }
}

/// `at` in RFC 3339, in UTC, to the millisecond.
fn rfc3339(at: SystemTime) -> String {
    let millis = at
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .unwrap_or(0);
    ess_primitives::Rfc3339Instant::from_epoch_millis(millis)
        .or_else(|| ess_primitives::Rfc3339Instant::from_epoch_millis(0))
        .map(ess_primitives::Rfc3339Instant::to_rfc3339)
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_keeps_the_prefix_and_json_drops_it() {
        let at = UNIX_EPOCH + std::time::Duration::from_millis(1_500);
        assert_eq!(
            warn("dropped")
                .field("command", "c")
                .render(LogFormat::Text, at),
            "b10x-spindle: dropped"
        );
        assert_eq!(
            warn("refused:\nwhy").bare().render(LogFormat::Text, at),
            "refused:\nwhy"
        );
        let json: Value = serde_json::from_str(
            &warn("dropped")
                .field("attempts", 5)
                .render(LogFormat::Json, at),
        )
        .expect("one JSON object");
        assert_eq!(
            json,
            serde_json::json!({
                "time": "1970-01-01T00:00:01.5Z",
                "level": "warn",
                "message": "dropped",
                "attempts": 5
            })
        );
    }
}
