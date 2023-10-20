mod reactor;

use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Instant;
use async_oneshot::oneshot;
use futures_lite::pin;
use winit::event::Event;
use winit::event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy};
use winit::platform::run_return::EventLoopExtRunReturn;
use winit::window::WindowBuilder;
use crate::framework::runtime::reactor::{EventLoopOp, Reactor};
use crate::framework::window::{Gui, GuiWindowHandle};

pub struct EventLoopWaker {
    proxy: EventLoopProxy<Wakeup>,
    notified: AtomicBool,
    awake: AtomicBool
}

impl EventLoopWaker {
    fn new(event_loop: &EventLoop<Wakeup>) -> Self {
        Self {
            proxy: event_loop.create_proxy(),
            notified: AtomicBool::new(true),
            awake: AtomicBool::new(false),
        }
    }

    fn notify(&self) {
        if self.notified.swap(true, Ordering::SeqCst) || self.awake.load(Ordering::SeqCst) {
            return;
        }

        let _ = self.proxy.send_event(Wakeup);
    }
}

impl Wake for EventLoopWaker {
    fn wake(self: Arc<Self>) {
        self.notify()
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.notify()
    }
}

pub struct Wakeup;

pub fn block_on<F: Future>(fut: F) -> F::Output {
    pin!(fut);

    let mut event_loop = EventLoopBuilder::with_user_event()
        .build();

    let notifier = Arc::new(EventLoopWaker::new(&event_loop));
    let waker = Waker::from(notifier.clone());

    let mut yielding = false;
    let mut deadline: Option<Instant> = None;

    let reactor = Rc::new(Reactor::new(notifier.clone()));
    let _guard = reactor.install();

    let mut future_result = None;
    let result = &mut future_result;
    event_loop.run_return(move |event, target, flow| {
        let about_to_sleep = match &event {
            Event::NewEvents(_) => {
                yielding = false;
                notifier.awake.store(true, Ordering::SeqCst);
                false
            },
            Event::RedrawEventsCleared => {
                notifier.awake.store(false, Ordering::SeqCst);
                true
            }
            _ => false
        };

        reactor.process_event(&event);

        reactor.process_loop_ops(target);

        while result.is_none() && !yielding && notifier.notified.swap(false, Ordering::SeqCst) {
            let mut cx = Context::from_waker(&waker);
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(r) => *result = Some(r),
                Poll::Pending => {
                    if notifier.notified.load(Ordering::SeqCst) {
                        yielding = true;
                    }
                }
            }
            reactor.process_loop_ops(target);
        }

        if about_to_sleep {
            deadline = reactor.calculate_deadline();
        }

        if result.is_some() {
            flow.set_exit();
        } else if yielding {
            flow.set_poll();
        } else if let Some(deadline) = deadline {
            flow.set_wait_until(deadline);
        } else {
            flow.set_wait()
        }
    });
    future_result.unwrap()
}

pub async fn window(gui: Gui) -> GuiWindowHandle {
    let reactor = Reactor::current();
    let (tx, rx) = oneshot();
    reactor.insert_event_loop_op(EventLoopOp::BuildWindow {
        gui,
        sender: tx,
    });
    rx.await.unwrap()
}
