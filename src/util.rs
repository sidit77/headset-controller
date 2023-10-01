use std::io::{Error, ErrorKind, Write};

use crossbeam_utils::atomic::AtomicCell;
use winit::event_loop::EventLoopProxy;

pub trait CopySlice<T> {
    fn cloned(self) -> Box<[T]>;
}

impl<T: Clone> CopySlice<T> for &[T] {
    fn cloned(self) -> Box<[T]> {
        self.to_vec().into_boxed_slice()
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

pub struct EscapeStripper<T> {
    inner: T,
    escape_sequence: bool,
    buffer: String
}

impl<T> EscapeStripper<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            escape_sequence: false,
            buffer: String::new()
        }
    }
}

impl<T: Write> Write for EscapeStripper<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let str = std::str::from_utf8(buf).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        self.buffer.clear();
        for c in str.chars() {
            match self.escape_sequence {
                true if c == 'm' => self.escape_sequence = false,
                true => {}
                false if c == '\u{001b}' => self.escape_sequence = true,
                false => self.buffer.push(c)
            }
        }
        self.inner.write_all(self.buffer.as_bytes())?;
        Ok(str.as_bytes().len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub trait AtomicCellExt<T> {
    fn update<F: Fn(&mut T)>(&self, func: F);
}

impl<T: Copy + Eq> AtomicCellExt<T> for AtomicCell<T> {
    fn update<F: Fn(&mut T)>(&self, func: F) {
        let mut previous_state = self.load();
        loop {
            let mut current_state = previous_state;
            func(&mut current_state);

            match self.compare_exchange(previous_state, current_state) {
                Ok(_) => break,
                Err(current) => {
                    previous_state = current;
                    tracing::trace!("compare exchange failed!")
                }
            }
        }
    }
}

pub trait SenderExt<T> {
    fn send_log<I: Into<T>>(&self, update: I);
}

impl<T> SenderExt<T> for EventLoopProxy<T> {
    fn send_log<I: Into<T>>(&self, update: I) {
        self.send_event(update.into())
            .unwrap_or_else(|_| tracing::warn!("Could not send message because the receiver is closed"))
    }
}

pub trait VecExt<T> {
    fn prepend<I: IntoIterator<Item = T>>(&mut self, iter: I);
}

impl<T> VecExt<T> for Vec<T> {
    fn prepend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let prev = self.len();
        self.extend(iter);
        let offset = self.len() - prev;
        self.rotate_right(offset);
    }
}
