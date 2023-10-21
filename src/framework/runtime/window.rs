use std::future::Future;
use std::rc::Rc;
use async_oneshot::oneshot;
use crate::framework::runtime::reactor::{EventLoopOp, Reactor};
use crate::framework::window::Gui;

pub struct AsyncGuiWindow {
    reactor: Rc<Reactor>,
    id: usize
}

impl AsyncGuiWindow {

    pub async fn new(gui: Gui) -> Self {
        let reactor = Reactor::current();
        let (tx, rx) = oneshot();
        reactor.insert_event_loop_op(EventLoopOp::BuildWindow {
            gui,
            sender: tx,
        });
        let id = rx.await.unwrap();
        Self {
            reactor,
            id
        }
    }

    pub fn close_requested(&self) -> impl Future<Output=()> {
        self.reactor.with_window(self.id, |w| w.close_requested())
    }

    pub fn focus(&self) {
        self.reactor.with_window(self.id, |w| w.focus())
    }

}

impl Drop for AsyncGuiWindow {
    fn drop(&mut self) {
        self.reactor.remove_window(self.id);
    }
}