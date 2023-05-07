use std::io::{Error, ErrorKind, Write};

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
