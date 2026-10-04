//! Asynchronous UI bridge to the allowlisted local NetEase CDP adapter.
use crate::cdp::LocalCdpClient;
use isle_core::player_extension::PlaybackMode;
use std::{
    sync::{mpsc, Arc, Mutex},
    thread,
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::PostMessageW,
};

pub const UPDATED: u32 = 0x8000 + 83;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Mode(Option<PlaybackMode>),
    Updated(PlaybackMode),
    Failed(String),
}

enum Command {
    Read(u16),
    Set(u16, PlaybackMode),
    Stop,
}

pub struct Service {
    sender: mpsc::SyncSender<Command>,
    outcome: Arc<Mutex<Option<Outcome>>>,
}

impl Service {
    pub fn new(hwnd: HWND) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let outcome = Arc::new(Mutex::new(None));
        let result = outcome.clone();
        let window = hwnd.0;
        thread::Builder::new()
            .name("isle-netease-cdp".into())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    let next = match command {
                        Command::Read(port) => LocalCdpClient::new(port)
                            .and_then(LocalCdpClient::cloud_music_playback_mode)
                            .map(Outcome::Mode)
                            .map_err(|error| error.to_string()),
                        Command::Set(port, mode) => LocalCdpClient::new(port)
                            .and_then(|client| client.set_cloud_music_playback_mode(mode))
                            .map(|action| Outcome::Updated(action.observed))
                            .map_err(|error| error.to_string()),
                        Command::Stop => break,
                    }
                    .unwrap_or_else(Outcome::Failed);
                    *result.lock().unwrap_or_else(|error| error.into_inner()) = Some(next);
                    unsafe {
                        let _ = PostMessageW(HWND(window), UPDATED, WPARAM(0), LPARAM(0));
                    }
                }
            })?;
        Ok(Self { sender, outcome })
    }

    pub fn read(&self, port: u16) -> bool {
        self.sender.try_send(Command::Read(port)).is_ok()
    }

    pub fn set(&self, port: u16, mode: PlaybackMode) -> bool {
        self.sender.try_send(Command::Set(port, mode)).is_ok()
    }

    pub fn take(&self) -> Option<Outcome> {
        self.outcome
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.sender.try_send(Command::Stop);
    }
}

pub const fn next_mode(mode: PlaybackMode) -> PlaybackMode {
    match mode {
        PlaybackMode::Sequential => PlaybackMode::RepeatList,
        PlaybackMode::RepeatList => PlaybackMode::RepeatOne,
        PlaybackMode::RepeatOne => PlaybackMode::Shuffle,
        PlaybackMode::Shuffle => PlaybackMode::Sequential,
    }
}

pub const fn mode_label(mode: PlaybackMode) -> &'static str {
    match mode {
        PlaybackMode::Sequential => "顺序播放",
        PlaybackMode::RepeatList => "列表循环",
        PlaybackMode::RepeatOne => "单曲循环",
        PlaybackMode::Shuffle => "随机播放",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_modes_cycle_only_through_provider_supported_values() {
        let sequence = [
            PlaybackMode::Sequential,
            PlaybackMode::RepeatList,
            PlaybackMode::RepeatOne,
            PlaybackMode::Shuffle,
            PlaybackMode::Sequential,
        ];
        for pair in sequence.windows(2) {
            assert_eq!(next_mode(pair[0]), pair[1]);
        }
    }
}
