use std::fmt::Display;

pub trait CopySlice<T> {
    fn cloned(self) -> Box<[T]>;
}

impl<T: Clone> CopySlice<T> for &[T] {
    fn cloned(self) -> Box<[T]> {
        self
            .iter()
            .cloned()
            .collect::<Vec<T>>()
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