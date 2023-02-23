use std::time::{Duration, Instant};
use fixed_map::{Key, Map};
use crate::util::PeekExt;

#[derive(Debug, Clone, Copy, Key, Eq, PartialEq)]
pub enum Action {
    SaveConfig,
    UpdateSideTone,
    UpdateEqualizer,
    UpdateMicrophoneVolume,
    UpdateVolumeLimit,
}

impl Action {
    fn timeout(self) -> Duration {
        match self {
            Action::SaveConfig => Duration::from_secs(10),
            _ => Duration::from_millis(500)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Debouncer(Map<Action, Instant>);

impl Debouncer {

    pub fn new() -> Self {
        Self(Map::new())
    }

    pub fn submit(&mut self, action: Action) {
        let now = Instant::now();
        let old = self.0.insert(action, now);
        debug_assert!(old.map_or(true, |old| old <= now));
    }

    pub fn submit_all(&mut self, actions: impl IntoIterator<Item=Action>){
        for action in actions {
            self.submit(action);
        }
    }

    pub fn next_action(&self) -> Option<Instant> {
        self.0
            .iter()
            .map(|(k, v)| *v + k.timeout())
            .min()
    }

    pub fn force(&mut self, action: Action) {
        if let Some(time) = self.0.get_mut(action) {
            *time -= action.timeout();
        }
    }

    pub fn force_all(&mut self, actions: impl IntoIterator<Item=Action>) {
        for action in actions {
            self.force(action);
        }
    }

}

impl Iterator for Debouncer {
    type Item = Action;

    fn next(&mut self) -> Option<Self::Item> {
        let now = Instant::now();
        let elapsed = self.0
            .iter()
            .find(|(k, b)| now
                .checked_duration_since(**b)
                .map_or(true, |dur| dur >= k.timeout()))
            .map(|(k,_)| k);
        elapsed.peek(|k| self.0.remove(*k))
    }
}