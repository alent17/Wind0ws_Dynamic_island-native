//! Bounded live activity arbitration in monotonic application time.
use std::collections::VecDeque;

pub type ActivityId = String;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityKind {
    Volume,
    Timer,
    Media,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LiveActivity {
    pub id: ActivityId,
    pub kind: ActivityKind,
    pub title: String,
    pub value: String,
    pub progress: Option<f32>,
    pub priority: u8,
    pub expires_at: Option<f64>,
    pub completed: bool,
}
impl LiveActivity {
    pub fn valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= 128
            && self.title.chars().count() <= 128
            && self.value.chars().count() <= 128
            && self
                .progress
                .is_none_or(|p| p.is_finite() && (0.0..=1.0).contains(&p))
            && self
                .expires_at
                .is_none_or(|time| time.is_finite() && time >= 0.0)
    }
}
#[derive(Default)]
pub struct ActivityManager {
    entries: VecDeque<LiveActivity>,
}
impl ActivityManager {
    pub const CAPACITY: usize = 32;
    /// Same identity updates in place. Lower priority overflow is discarded.
    pub fn update(&mut self, activity: LiveActivity) -> bool {
        if !activity.valid() {
            return false;
        }
        if let Some(old) = self.entries.iter_mut().find(|old| old.id == activity.id) {
            if *old == activity {
                return false;
            }
            *old = activity;
            return true;
        }
        if self.entries.len() == Self::CAPACITY {
            let (index, lowest) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, item)| item.priority)
                .unwrap();
            if lowest.priority > activity.priority {
                return false;
            }
            self.entries.remove(index);
        }
        self.entries.push_back(activity);
        true
    }
    pub fn dismiss(&mut self, id: &str) -> bool {
        let old = self.entries.len();
        self.entries.retain(|activity| activity.id != id);
        old != self.entries.len()
    }
    pub fn complete(&mut self, id: &str, now: f64) -> bool {
        if !now.is_finite() || now < 0. {
            return false;
        }
        let Some(activity) = self.entries.iter_mut().find(|activity| activity.id == id) else {
            return false;
        };
        activity.completed = true;
        activity.progress = Some(1.0);
        activity.expires_at = Some(now + 3.0);
        true
    }
    pub fn expire(&mut self, now: f64) -> bool {
        if !now.is_finite() || now < 0. {
            return false;
        }
        let old = self.entries.len();
        self.entries
            .retain(|activity| activity.expires_at.is_none_or(|deadline| now < deadline));
        old != self.entries.len()
    }
    /// Exactly two external slots; the rest stay in the bounded queue.
    pub fn slots(&self) -> Vec<LiveActivity> {
        let mut items: Vec<_> = self.entries.iter().enumerate().collect();
        items.sort_by_key(|(index, activity)| (std::cmp::Reverse(activity.priority), *index));
        items
            .into_iter()
            .take(2)
            .map(|(_, activity)| activity.clone())
            .collect()
    }
    pub fn queued(&self) -> usize {
        self.entries.len().saturating_sub(2)
    }
    pub fn next_expiry(&self) -> Option<f64> {
        self.entries
            .iter()
            .filter_map(|activity| activity.expires_at)
            .min_by(f64::total_cmp)
    }
    pub fn contains(&self, id: &str) -> bool {
        self.entries.iter().any(|activity| activity.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn activity(id: &str, priority: u8) -> LiveActivity {
        LiveActivity {
            id: id.into(),
            kind: ActivityKind::Timer,
            title: "Timer".into(),
            value: "1:00".into(),
            progress: Some(0.5),
            priority,
            expires_at: Some(10.0),
            completed: false,
        }
    }
    #[test]
    fn updates_deduplicate_and_priority_limits_visible_slots() {
        let mut manager = ActivityManager::default();
        manager.update(activity("low", 1));
        manager.update(activity("high", 100));
        manager.update(activity("medium", 50));
        assert_eq!(
            manager
                .slots()
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            ["high", "medium"]
        );
        let mut update = activity("low", 110);
        update.value = "0:59".into();
        assert!(manager.update(update.clone()));
        assert!(!manager.update(update));
        assert_eq!(manager.queued(), 1);
        assert_eq!(manager.slots()[0].value, "0:59");
    }
    #[test]
    fn completion_expiration_and_dismiss_are_independent() {
        let mut manager = ActivityManager::default();
        manager.update(activity("timer", 100));
        manager.update(activity("volume", 10));
        assert!(manager.complete("timer", 2.));
        assert!(manager.slots()[0].completed);
        assert!(!manager.expire(4.9));
        assert!(manager.expire(5.));
        assert!(manager.contains("volume"));
        assert!(manager.dismiss("volume"));
        assert!(manager.slots().is_empty());
    }
    #[test]
    fn malformed_and_overflow_requests_cannot_grow_storage() {
        let mut manager = ActivityManager::default();
        for index in 0..64 {
            manager.update(activity(&format!("{index}"), 20));
        }
        assert_eq!(manager.queued(), 30);
        assert!(!manager.update(activity("discard", 1)));
        let mut invalid = activity("invalid", 100);
        invalid.progress = Some(f32::NAN);
        assert!(!manager.update(invalid));
        assert_eq!(manager.next_expiry(), Some(10.));
    }
}
