use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use async_io::Timer;
use fixed_map::{Key, Map};
use flume::{Sender};
use futures_lite::{FutureExt, Stream, StreamExt};
use tracing::instrument;

use crate::util::PeekExt;

#[derive(Debug, Clone, Copy, Key, Eq, PartialEq)]
pub enum Action {
    SaveConfig,

    UpdateSideTone,
    UpdateEqualizer,
    UpdateMicrophoneVolume,
    UpdateVolumeLimit,

    UpdateInactiveTime,
    UpdateMicrophoneLight,
    UpdateBluetoothCall,
    UpdateAutoBluetooth,

    UpdateSystemAudio,
    UpdateTray,
    UpdateTrayTooltip,
    UpdateDeviceStatus,
    RefreshDeviceList,
    SwitchDevice
}

impl Action {
    fn timeout(self) -> Duration {
        match self {
            Action::SaveConfig => Duration::from_secs(10),
            Action::SwitchDevice | Action::RefreshDeviceList => Duration::from_millis(10),
            //Action::UpdateDeviceStatus => Duration::from_millis(250),
            _ => Duration::from_millis(500)
        }
    }
}

#[derive(Debug)]
enum ActionOp {
    Submit(Action, Instant),
    Force(Action)
}

pub trait ActionSender {
    fn submit(&mut self, action: Action);
    fn force(&mut self, action: Action);

    fn submit_all(&mut self, actions: impl IntoIterator<Item=Action>) {
        for action in actions {
            self.submit(action);
        }
    }

    fn force_all(&mut self, actions: impl IntoIterator<Item=Action>) {
        for action in actions {
            self.force(action);
        }
    }

    #[instrument(skip_all)]
    fn submit_profile_change(&mut self) {
        let actions = [
            Action::UpdateSideTone,
            Action::UpdateEqualizer,
            Action::UpdateMicrophoneVolume,
            Action::UpdateVolumeLimit
        ];
        self.submit_all(actions);
        self.force_all(actions);
    }

    #[instrument(skip_all)]
    fn submit_full_change(&mut self) {
        self.submit_profile_change();
        let actions = [
            Action::UpdateMicrophoneLight,
            Action::UpdateInactiveTime,
            Action::UpdateBluetoothCall,
            Action::UpdateAutoBluetooth,
            Action::UpdateSystemAudio
        ];
        self.submit_all(actions);
        self.force_all(actions);
    }
}

#[derive(Debug, Clone)]
pub struct ActionProxy {
    sender: Sender<ActionOp>
}

impl ActionSender for ActionProxy {

    #[instrument(skip(self))]
    fn submit(&mut self, action: Action) {
        self
            .sender
            .try_send(ActionOp::Submit(action, Instant::now()))
            .unwrap_or_else(|_| tracing::warn!("Failed to send"));
        tracing::trace!("Submitted new action");
    }

    #[instrument(skip(self))]
    fn force(&mut self, action: Action) {
        self
            .sender
            .try_send(ActionOp::Force(action))
            .unwrap_or_else(|_| tracing::warn!("Failed to send"));
        tracing::trace!("Skipped timeout");
    }
}

pub struct ActionReceiver {
    receiver: flume::r#async::RecvStream<'static, ActionOp>,
    actions: Map<Action, Instant>,
    timer: Timer
}

impl ActionSender for ActionReceiver {

    #[instrument(skip(self))]
    fn submit(&mut self, action: Action) {
        let now = Instant::now();
        let old = self.actions.insert(action, now);
        debug_assert!(old.map_or(true, |old| old <= now));
        tracing::trace!("Submitted new action");
    }

    #[instrument(skip(self))]
    fn force(&mut self, action: Action) {
        if let Some(time) = self.actions.get_mut(action) {
            *time -= action.timeout();
        }
        tracing::trace!("Skipped timeout");
    }
}

impl Stream for ActionReceiver {
    type Item = Action;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.receiver.is_disconnected() {

        }
        let empty_remaining = self.receiver.is_disconnected() || loop {
            match self.receiver.poll_next(cx) {
                Poll::Ready(Some(op)) => match op {
                    ActionOp::Submit(action, now) => {
                        let old = self.actions.insert(action, now);
                        debug_assert!(old.map_or(true, |old| old <= now));
                    }
                    ActionOp::Force(action) => {
                        if let Some(time) = self.actions.get_mut(action) {
                            *time -= action.timeout();
                        }
                    }
                },
                Poll::Ready(None) => break true,
                Poll::Pending => break false,
            }
        };
        if empty_remaining {
            return Poll::Ready(self.actions
                .keys()
                .next()
                .peek(|k| self.actions.remove(*k)));
        }

        let now = Instant::now();
        let elapsed = self
            .actions
            .iter()
            .find_map(|(k, b)| {
                now.checked_duration_since(*b)
                    .map_or(true, |dur| dur >= k.timeout())
                    .then_some(k)
            });
        if let Some(action) = elapsed {
            self.actions.remove(action);
            return Poll::Ready(Some(action));
        }

        let deadline = self
            .actions
            .iter()
            .map(|(k, v)| *v + k.timeout())
            .min();

        self.timer = deadline
            .map(Timer::at)
            .unwrap_or_else(Timer::never);// .set(deadline.map(sleep_until));
        debug_assert!(self.timer.poll(cx).is_pending());

        //if let Some(timer) = self.timer.as_mut().as_pin_mut() {
        //    debug_assert!(timer.poll(cx).is_pending());
        //}

        Poll::Pending
    }
}

pub fn debouncer() -> (ActionProxy, ActionReceiver) {
    let (sender, receiver) = flume::bounded(512);
    let sender = ActionProxy {
        sender,
    };
    let receiver = ActionReceiver {
        receiver: receiver.into_stream(),
        actions: Map::new(),
        timer: Timer::never(),
    };
    (sender, receiver)
}

#[derive(Debug, Clone)]
pub struct Debouncer(Map<Action, std::time::Instant>);

impl Debouncer {
    pub fn new() -> Self {
        Self(Map::new())
    }

    #[instrument(skip(self))]
    pub fn submit(&mut self, action: Action) {
        let now = std::time::Instant::now();
        let old = self.0.insert(action, now);
        debug_assert!(old.map_or(true, |old| old <= now));
        tracing::trace!("Received new action");
    }

    pub fn submit_all(&mut self, actions: impl IntoIterator<Item = Action>) {
        for action in actions {
            self.submit(action);
        }
    }

    pub fn next_action(&self) -> Option<std::time::Instant> {
        self.0.iter().map(|(k, v)| *v + k.timeout()).min()
    }

    #[instrument(skip(self))]
    pub fn force(&mut self, action: Action) {
        if let Some(time) = self.0.get_mut(action) {
            *time -= action.timeout();
            tracing::trace!("Skipped timeout");
        }
    }

    pub fn force_all(&mut self, actions: impl IntoIterator<Item = Action>) {
        for action in actions {
            self.force(action);
        }
    }
}

impl Iterator for Debouncer {
    type Item = Action;

    fn next(&mut self) -> Option<Self::Item> {
        let now = std::time::Instant::now();
        let elapsed = self
            .0
            .iter()
            .find(|(k, b)| {
                now.checked_duration_since(**b)
                    .map_or(true, |dur| dur >= k.timeout())
            })
            .map(|(k, _)| k);
        elapsed.peek(|k| self.0.remove(*k))
    }
}
