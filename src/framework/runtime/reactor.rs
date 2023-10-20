use std::cell::{RefCell, Cell};
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use async_oneshot::Sender;
use winit::error::OsError;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Window, WindowBuilder};
use crate::framework::runtime::{EventLoopWaker, Wakeup};

pub struct Reactor {
    waker: Arc<EventLoopWaker>,

    event_loop_ops: RefCell<VecDeque<EventLoopOp>>
}

impl Reactor {

    pub fn new(waker: Arc<EventLoopWaker>) -> Self {
        Self {
            waker,
            event_loop_ops: RefCell::new(VecDeque::new()),
        }
    }

    pub(crate) fn current() -> Rc<Self> {
        REACTOR
            .with(|tls| {
                let current = tls.take();
                tls.set(current.clone());
                current
            })
            .take()
            .expect("No Reactor installed for the current thread")
    }

    pub(crate) fn install(self: &Rc<Self>) -> ReactorGuard {
        REACTOR
            .with(|tls| tls
                .replace(Some(self.clone())))
            .is_some()
            .then(|| panic!("Another reactor is already installed"));
        ReactorGuard::default()
    }

    pub fn insert_event_loop_op(&self, op: EventLoopOp) {
        self.event_loop_ops.borrow_mut().push_back(op);
        self.waker.notify();
    }

    pub fn process_loop_ops(&self, target: &EventLoopWindowTarget<Wakeup>) {
        self.event_loop_ops
            .borrow_mut()
            .drain(..)
            .for_each(|op| op.run(target));
    }

}

thread_local! {
    static REACTOR: Cell<Option<Rc<Reactor>>> = Cell::new(None);
}
#[derive(Default)]
pub(crate) struct ReactorGuard {
    _marker: PhantomData<*mut ()>
}

impl Drop for ReactorGuard {
    fn drop(&mut self) {
        REACTOR.with(|tls| tls.set(None))
    }
}

pub enum EventLoopOp {
    BuildWindow {
        builder: Box<WindowBuilder>,
        sender: Sender<Result<Window, OsError>>
    }
}

impl EventLoopOp {
    fn run(self, target: &EventLoopWindowTarget<Wakeup>) {
        match self {
            EventLoopOp::BuildWindow { builder, mut sender } => {
                if !sender.is_closed() {
                    let window = builder.build(target);
                    let _ = sender.send(window);
                }
            }
        }
    }
}