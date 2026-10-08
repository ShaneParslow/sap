//! Leveled kernel log over the console. Messages above `MAX_LEVEL` compile away.

#[allow(dead_code)] // not every level is used yet
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

pub const MAX_LEVEL: Level = if cfg!(debug_assertions) { Level::Debug } else { Level::Info };

impl Level {
    pub const fn tag(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn ",
            Level::Info => "info ",
            Level::Debug => "debug",
            Level::Trace => "trace",
        }
    }
}

#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        if $level <= $crate::log::MAX_LEVEL {
            $crate::println!("[{}] {}", $level.tag(), format_args!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! error { ($($arg:tt)*) => { $crate::log!($crate::log::Level::Error, $($arg)*) }; }
#[macro_export]
macro_rules! warn { ($($arg:tt)*) => { $crate::log!($crate::log::Level::Warn, $($arg)*) }; }
#[macro_export]
macro_rules! info { ($($arg:tt)*) => { $crate::log!($crate::log::Level::Info, $($arg)*) }; }
#[macro_export]
macro_rules! debug { ($($arg:tt)*) => { $crate::log!($crate::log::Level::Debug, $($arg)*) }; }
#[macro_export]
macro_rules! trace { ($($arg:tt)*) => { $crate::log!($crate::log::Level::Trace, $($arg)*) }; }
