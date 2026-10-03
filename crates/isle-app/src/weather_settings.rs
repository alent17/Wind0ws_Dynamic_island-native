//! Settings V2 is implemented in the `settings` module. This compatibility
//! facade keeps the existing message-loop call sites stable while main.rs is
//! integrated with runtime settings.

pub use crate::settings::window::{
    APPLY, APPLY_APPEARANCE, APPLY_CONTROLS, CLOSE, COMMAND, INSPECTION_LOCK, PLACEMENT_CHANGED,
    PLAYERS, RETRY_SAVE, REVERT_SAVED, SEARCH,
};
pub use crate::settings::{SaveState, Settings};
