//! Optional, player-specific capabilities layered over generic GSMTC.

use crate::player::PlayerKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackMode {
    Sequential,
    RepeatList,
    RepeatOne,
    Shuffle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerExtensionCapabilities {
    pub playback_mode: bool,
    pub like_state: bool,
    pub set_like: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedAction<T> {
    pub requested: T,
    pub observed: T,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerExtensionError {
    Unavailable,
    Unsupported,
    StaleTrack,
    Failed(String),
    VerificationFailed,
}

/// Optional adapter contract. Generic players have no extension and continue
/// to use the existing GSMTC controls and `MediaCapabilities` unchanged.
pub trait PlayerExtension: Send + Sync {
    fn player_kind(&self) -> PlayerKind;
    fn capabilities(&self) -> PlayerExtensionCapabilities;
    fn playback_mode(&self) -> Result<Option<PlaybackMode>, PlayerExtensionError>;
    fn set_playback_mode(
        &self,
        mode: PlaybackMode,
    ) -> Result<VerifiedAction<PlaybackMode>, PlayerExtensionError>;
    fn like_state(&self) -> Result<Option<bool>, PlayerExtensionError>;
    fn set_like(
        &self,
        track_id: &str,
        liked: bool,
    ) -> Result<VerifiedAction<bool>, PlayerExtensionError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn extension_contract_is_thread_safe_and_capabilities_default_off() {
        assert_send_sync::<dyn PlayerExtension>();
        assert_eq!(
            PlayerExtensionCapabilities::default(),
            PlayerExtensionCapabilities {
                playback_mode: false,
                like_state: false,
                set_like: false,
            }
        );
        assert_eq!(
            VerifiedAction {
                requested: PlaybackMode::RepeatOne,
                observed: PlaybackMode::RepeatOne,
            }
            .observed,
            PlaybackMode::RepeatOne
        );
    }
}
