use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread::{JoinHandle, spawn};
use crossbeam_utils::atomic::AtomicCell;
use either::Either;
use flume::Sender;
use futures_lite::FutureExt;
use oneshot::{channel, Receiver};

pub trait CopySlice<T> {
    fn cloned(self) -> Box<[T]>;
}

impl<T: Clone> CopySlice<T> for &[T] {
    fn cloned(self) -> Box<[T]> {
        self.to_vec().into_boxed_slice()
    }
}

pub trait PeekExt<T, R> {
    fn peek(self, func: impl FnOnce(&mut T) -> R) -> Self;
}

impl<T, R> PeekExt<T, R> for Option<T> {
    fn peek(mut self, func: impl FnOnce(&mut T) -> R) -> Self {
        if let Some(inner) = self.as_mut() {
            func(inner);
        }
        self
    }
}

pub async fn select<F1, F2, L, R>(future1: F1, future2: F2) -> Either<L, R>
    where
        F1: Future<Output = L>,
        F2: Future<Output = R>
{
    let future1 = async move {
        Either::Left(future1.await)
    };

    let future2 = async move {
        Either::Right(future2.await)
    };

    future1.or(future2).await
}

pub struct WorkerThread<T> {
    thread: Option<JoinHandle<()>>,
    receiver: Receiver<T>
}

impl<T: Send + 'static> WorkerThread<T> {

    pub fn spawn<F: FnOnce() -> T + Send + 'static>(func: F) -> Self {
        let (sender, receiver) = channel();
        let thread = spawn(move || {
            sender
                .send(func())
                .unwrap_or_else(|_| tracing::trace!("Receiver of this workthread is no longer alive"));
        });
        Self {
            thread: Some(thread),
            receiver,
        }
    }

}

impl<T> Future for WorkerThread<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.receiver.poll(cx).map(|r| r.expect("Worker thread disappeared"))
    }
}

impl<T> Drop for WorkerThread<T> {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            if !thread.is_finished() {
                tracing::debug!("Waiting for working thread to finish work");
                thread.join().expect("Worker thread panic");
            }
        }
    }
}

/*
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

 */

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
    fn send_log(&self, update: T);
}

impl<T> SenderExt<T> for Sender<T> {
    fn send_log(&self, update: T) {
        self.try_send(update)
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
