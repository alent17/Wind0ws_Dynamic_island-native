//! Platform-independent media state. Times are monotonic seconds from app start.
pub mod spectrum;
pub mod weather;
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioSnapshot {
    pub device: AudioDevice,
    pub devices: Vec<AudioDevice>,
    pub volume: u8,
    pub muted: bool,
    pub failed: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaSnapshot {
    pub cover: Option<std::sync::Arc<Cover>>,
    pub session: u64,
    pub source: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub previous: bool,
    pub play_pause: bool,
    pub next: bool,
    pub timeline: Timeline,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cover {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // Premultiplied BGRA, bounded to 128 x 128.
}

pub fn matching_song(title: &str, artist: &str, found_title: &str, found_artist: &str) -> bool {
    fn normalize(text: &str) -> String {
        text.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect()
    }
    let title = normalize(title);
    let artist = normalize(artist);
    // Require both fields: uncertain search matches must not become album art.
    !title.is_empty()
        && !artist.is_empty()
        && title == normalize(found_title)
        && artist == normalize(found_artist)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timeline {
    pub position_ms: u64,
    pub duration_ms: u64,
    pub received_at: f64,
}
impl Timeline {
    pub fn position(&self, now: f64, playing: bool) -> u64 {
        let elapsed = if playing && now.is_finite() {
            ((now - self.received_at).max(0.) * 1000.) as u64
        } else {
            0
        };
        let position = self.position_ms.saturating_add(elapsed);
        if self.duration_ms > 0 {
            position.min(self.duration_ms)
        } else {
            position
        }
    }

    /// Preserve a known duration only within the same track. A repeated poll is
    /// not a new sample: otherwise players reporting sparse positions stutter.
    /// Provider wall-clock LastUpdatedTime is deliberately not extrapolated.
    pub fn accept(
        &mut self,
        reported: Option<(u64, u64)>,
        now: f64,
        was_playing: bool,
        playing: bool,
        new_track: bool,
    ) {
        if new_track {
            *self = Self {
                received_at: now,
                ..Self::default()
            };
        }
        let projected = self.position(now, was_playing);
        if let Some((position, duration)) = reported {
            if duration > 360_000_000 {
                *self = Self {
                    received_at: now,
                    ..Self::default()
                };
                return;
            }
            let effective = if duration > 0 {
                duration
            } else {
                self.duration_ms
            };
            let position = if effective > 0 {
                position.min(effective)
            } else {
                position
            };
            if new_track || position != self.position_ms || was_playing != playing {
                self.position_ms = position;
                self.received_at = now;
            }
            self.duration_ms = effective;
        } else if was_playing != playing {
            self.position_ms = projected;
            self.received_at = now;
        }
    }
}

#[derive(Default)]
pub struct Selection {
    pub allowed: Option<Vec<String>>,
    pub order: Vec<String>,
}
pub struct Candidate<'a> {
    pub id: &'a str,
    pub playing: bool,
    pub updated: i64,
}
impl Selection {
    pub fn choose(&self, candidates: &[Candidate<'_>], previous: Option<&str>) -> Option<usize> {
        if let Some(allowed) = &self.allowed {
            for id in self
                .order
                .iter()
                .filter(|id| allowed.contains(id))
                .chain(allowed.iter().filter(|id| !self.order.contains(id)))
            {
                if let Some(index) = candidates
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.id == id)
                    .max_by_key(|(_, c)| (c.playing, c.updated))
                    .map(|(i, _)| i)
                {
                    return Some(index);
                }
            }
            return None;
        }
        for id in &self.order {
            if let Some(index) = candidates
                .iter()
                .enumerate()
                .filter(|(_, c)| c.id == id)
                .max_by_key(|(_, c)| (c.playing, c.updated))
                .map(|(i, _)| i)
            {
                return Some(index);
            }
        }
        candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.playing)
            .max_by_key(|(_, c)| c.updated)
            .map(|(i, _)| i)
            .or_else(|| candidates.iter().position(|c| Some(c.id) == previous))
            .or_else(|| {
                candidates
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, c)| c.updated)
                    .map(|(i, _)| i)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_match_rejects_covers_and_missing_artist() {
        assert!(matching_song(
            "Song (Live)",
            "Artist",
            "song live",
            "ARTIST"
        ));
        assert!(!matching_song("Song", "Artist", "Song (Live)", "Artist"));
        assert!(!matching_song("Song", "Artist", "Song", "Cover Artist"));
        assert!(!matching_song("Song", "", "Song", "Artist"));
    }
    #[test]
    fn live_stream_does_not_inherit_a_finite_duration() {
        let mut t = Timeline {
            position_ms: 5000,
            duration_ms: 10000,
            received_at: 0.,
        };
        t.accept(Some((9000, u64::MAX)), 1., true, true, false);
        assert_eq!(t.duration_ms, 0);
        assert_eq!(t.position_ms, 0);
    }
    #[test]
    fn sparse_timeline_does_not_restart_on_repeated_poll() {
        let mut t = Timeline::default();
        t.accept(Some((2000, 10000)), 1., false, true, true);
        t.accept(Some((2000, 0)), 2., true, true, false);
        assert_eq!(t.position(3., true), 4000);
        assert_eq!(t.duration_ms, 10000);
        assert_eq!(t.position(100., true), 10000);
    }
    #[test]
    fn seek_and_track_change_are_authoritative() {
        let mut t = Timeline::default();
        t.accept(Some((8000, 10000)), 0., false, true, true);
        t.accept(Some((0, 0)), 2., true, true, false);
        assert_eq!(t.position(2., true), 0);
        t.accept(None, 3., true, true, true);
        assert_eq!(t.duration_ms, 0);
    }
    #[test]
    fn missing_snapshot_freezes_on_pause_and_resumes() {
        let mut t = Timeline {
            position_ms: 2000,
            duration_ms: 10000,
            received_at: 0.,
        };
        t.accept(None, 2., true, false, false);
        assert_eq!(t.position(20., false), 4000);
        t.accept(None, 20., false, true, false);
        assert_eq!(t.position(21., true), 5000);
    }
    #[test]
    fn selection_preserves_allowlist_order_and_paused_session() {
        let c = [
            Candidate {
                id: "a",
                playing: false,
                updated: 4,
            },
            Candidate {
                id: "b",
                playing: true,
                updated: 9,
            },
        ];
        assert_eq!(Selection::default().choose(&c, Some("a")), Some(1));
        assert_eq!(
            Selection {
                allowed: Some(vec![]),
                ..Selection::default()
            }
            .choose(&c, None),
            None
        );
        assert_eq!(
            Selection {
                allowed: Some(vec!["a".into(), "b".into()]),
                order: vec!["a".into()]
            }
            .choose(&c, None),
            Some(0)
        );
        let c = [
            Candidate {
                id: "a",
                playing: false,
                updated: 4,
            },
            Candidate {
                id: "b",
                playing: false,
                updated: 9,
            },
        ];
        assert_eq!(Selection::default().choose(&c, Some("a")), Some(0));
    }
}
