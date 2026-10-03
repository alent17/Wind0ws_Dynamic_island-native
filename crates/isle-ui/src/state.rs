use crate::model::Page;
pub use isle_core::activity::{ActivityKind, LiveActivity};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrimarySurfaceMode {
    #[default]
    Compact,
    Expanded(ExpandedView),
    Hidden,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExpandedView {
    #[default]
    Music,
    Timer,
    Volume,
    Clock,
    Weather,
    Shelf,
    PlayingNext,
}

impl From<Page> for ExpandedView {
    fn from(page: Page) -> Self {
        match page {
            Page::Music => Self::Music,
            Page::Timer => Self::Timer,
            Page::Volume => Self::Volume,
            Page::Clock => Self::Clock,
            Page::Weather => Self::Weather,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InteractionState {
    pub hovered: bool,
    pub pressed: bool,
    pub dragging: bool,
    pub inspection_lock: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiState {
    pub primary: PrimarySurfaceMode,
    pub interaction: InteractionState,
    pub activities: Vec<LiveActivity>,
}
