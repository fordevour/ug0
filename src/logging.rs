//! Короткие логи в консоль (stderr).
//!
//! Уровень задаётся аргументом `--loglevel=off|error|warn|info|debug`,
//! по умолчанию — `debug`. До вызова [`init`] логгер молчит.

use std::fmt;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub(crate) const USAGE: &str = "usage: ug0 [--loglevel=off|error|warn|info|debug]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Off,
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    pub(crate) const DEFAULT: Self = Self::Debug;

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "error" => Some(Self::Error),
            "warn" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            _ => None,
        }
    }

    /// Пропускает ли выбранный уровень сообщение данного уровня.
    pub(crate) fn allows(self, level: Self) -> bool {
        rank(self) >= rank(level)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }
}

fn rank(level: Level) -> u8 {
    match level {
        Level::Off => 0,
        Level::Error => 1,
        Level::Warn => 2,
        Level::Info => 3,
        Level::Debug => 4,
    }
}

/// Разбирает аргументы командной строки без имени программы.
/// Неизвестные аргументы пока игнорируются.
pub(crate) fn level_from_args<I>(args: I) -> Result<Level, String>
where
    I: IntoIterator<Item = String>,
{
    let mut level = Level::DEFAULT;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        if arg == "--loglevel" {
            let value = args
                .next()
                .ok_or_else(|| "--loglevel requires a value".to_owned())?;
            level = parse_or_error(&value)?;
        } else if let Some(value) = arg.strip_prefix("--loglevel=") {
            level = parse_or_error(value)?;
        }
    }

    Ok(level)
}

fn parse_or_error(value: &str) -> Result<Level, String> {
    Level::parse(value).ok_or_else(|| format!("invalid log level {value:?}"))
}

struct Config {
    level: Level,
    start: Instant,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Включает логи начиная с момента вызова.
pub(crate) fn init(level: Level) {
    let _ = CONFIG.set(Config {
        level,
        start: Instant::now(),
    });
}

pub(crate) fn log(level: Level, args: fmt::Arguments<'_>) {
    let Some(config) = CONFIG.get() else {
        return;
    };
    if !config.level.allows(level) {
        return;
    }

    eprintln!(
        "{}",
        format_line(level, config.start.elapsed(), &args.to_string())
    );
}

/// `[  12.345s INFO ] сообщение`
pub(crate) fn format_line(level: Level, elapsed: Duration, message: &str) -> String {
    format!(
        "[{:8.3}s {:<5}] {message}",
        elapsed.as_secs_f64(),
        level.label()
    )
}

macro_rules! error {
    ($($arg:tt)*) => {
        $crate::logging::log($crate::logging::Level::Error, format_args!($($arg)*))
    };
}

macro_rules! warning {
    ($($arg:tt)*) => {
        $crate::logging::log($crate::logging::Level::Warn, format_args!($($arg)*))
    };
}

macro_rules! info {
    ($($arg:tt)*) => {
        $crate::logging::log($crate::logging::Level::Info, format_args!($($arg)*))
    };
}

macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::logging::log($crate::logging::Level::Debug, format_args!($($arg)*))
    };
}

pub(crate) use {debug, error, info, warning};
