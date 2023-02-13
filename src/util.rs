use std::fmt::Display;

pub trait LogResultExt<T> {
    fn log_ok(self, msg: &str) -> Option<T>;
}

impl<T, E: Display> LogResultExt<T> for std::result::Result<T, E> {
    fn log_ok(self, msg: &str) -> Option<T> {
        match self {
            Ok(val) => Some(val),
            Err(err) => {
                log::warn!("{}: {}", msg, err);
                None
            }
        }
    }
}

pub trait PeekExt<T> {
    fn peek(self, func: impl FnOnce(&T)) -> Self;
}

impl<T> PeekExt<T> for Option<T> {
    fn peek(self, func: impl FnOnce(&T)) -> Self {
        if let Some(inner) = self.as_ref() {
            func(inner);
        }
        self
    }
}