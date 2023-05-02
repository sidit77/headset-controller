use std::fmt::Display;

pub trait CopySlice<T> {
    fn cloned(self) -> Box<[T]>;
}

impl<T: Clone> CopySlice<T> for &[T] {
    fn cloned(self) -> Box<[T]> {
        self.to_vec()
            .into_boxed_slice()
    }
}

pub trait LogResultExt<T> {
    fn log_ok(self, msg: &str) -> Option<T>;
}

impl<T, E: Display> LogResultExt<T> for std::result::Result<T, E> {
    fn log_ok(self, msg: &str) -> Option<T> {
        match self {
            Ok(val) => Some(val),
            Err(err) => {
                tracing::warn!("{}: {}", msg, err);
                None
            }
        }
    }
}

pub trait PeekExt<T, R> {
    fn peek(self, func: impl FnOnce(&T) -> R) -> Self;
}

impl<T, R> PeekExt<T, R> for Option<T> {
    fn peek(self, func: impl FnOnce(&T) -> R) -> Self {
        if let Some(inner) = self.as_ref() {
            func(inner);
        }
        self
    }
}