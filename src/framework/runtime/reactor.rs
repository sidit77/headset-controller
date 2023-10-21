use std::cell::{RefCell, Cell};
use std::collections::{BTreeMap, VecDeque};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use async_oneshot::Sender;
use winit::event::Event;
use winit::event_loop::EventLoopWindowTarget;
use crate::framework::runtime::{EventLoopWaker, Wakeup};
use crate::framework::window::{DefaultGuiWindow, Gui, GuiWindow};

pub struct Reactor {
    waker: Arc<EventLoopWaker>,

    next_window_id: Cell<usize>,
    active_windows: RefCell<BTreeMap<usize, DefaultGuiWindow>>,

    event_loop_ops: RefCell<VecDeque<EventLoopOp>>
}

impl Reactor {

    pub fn new(waker: Arc<EventLoopWaker>) -> Self {
        Self {
            waker,
            next_window_id: Cell::new(0),
            active_windows: RefCell::new(Default::default()),
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
            .for_each(|op| op.run(self, target));
    }

    pub fn process_event(&self, event: &Event<Wakeup>) {
        self.active_windows
            .borrow_mut()
            .values_mut()
            .for_each(|window| window.handle_events(event));
    }

    pub fn calculate_deadline(&self) -> Option<Instant> {
        self.active_windows
            .borrow()
            .values()
            .filter_map(GuiWindow::next_repaint)
            .min()
    }

    pub fn remove_window(&self, id: usize) {
        tracing::trace!("Destroying Window {}", id);
        self
            .active_windows
            .borrow_mut()
            .remove(&id)
            .is_none()
            .then_some(|| tracing::warn!("Window {} does not exist", id));
    }

    pub fn with_window<T>(&self, id: usize, func: impl FnOnce(&DefaultGuiWindow) -> T) -> T {
        self
            .active_windows
            .borrow()
            .get(&id)
            .map(func)
            .expect("No such window")
    }

    pub fn close(&self) {
        let mut windows = self.active_windows.borrow_mut();
        if !windows.is_empty() {
            tracing::warn!("{} Windows are still open", windows.len());
            windows.clear();
        }
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
        gui: Gui,
        sender: Sender<usize>
    }
}

impl EventLoopOp {
    fn run(self, reactor: &Reactor, target: &EventLoopWindowTarget<Wakeup>) {
        match self {
            EventLoopOp::BuildWindow { gui, mut sender } => {
                if !sender.is_closed() {
                    let window = GuiWindow::new(target, gui);
                    let id = reactor.next_window_id.replace(reactor.next_window_id.get() + 1);
                    reactor.active_windows.borrow_mut().insert(id, window);
                    tracing::trace!("Registered new gui window with id {}", id);
                    let _ = sender.send(id);
                }
            }
        }
    }
}