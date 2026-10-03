//! Built-in shelf configuration. Future identities and nested options survive edits.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct WidgetConfig {
    pub id: String,
    pub order: u16,
    pub span: u8,
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
impl Default for WidgetConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            order: 0,
            span: 1,
            enabled: true,
            extra: HashMap::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    Music,
    Volume,
    Timer,
    Clock,
    Weather,
    SystemStats,
}
pub struct WidgetDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: WidgetKind,
}
pub const BUILT_INS: [WidgetDefinition; 6] = [
    WidgetDefinition {
        id: "music",
        name: "Music",
        kind: WidgetKind::Music,
    },
    WidgetDefinition {
        id: "volume",
        name: "Volume",
        kind: WidgetKind::Volume,
    },
    WidgetDefinition {
        id: "timer",
        name: "Timer",
        kind: WidgetKind::Timer,
    },
    WidgetDefinition {
        id: "clock",
        name: "Clock",
        kind: WidgetKind::Clock,
    },
    WidgetDefinition {
        id: "weather",
        name: "Weather",
        kind: WidgetKind::Weather,
    },
    WidgetDefinition {
        id: "system-stats",
        name: "System Stats",
        kind: WidgetKind::SystemStats,
    },
];
pub fn defaults() -> Vec<WidgetConfig> {
    vec![WidgetConfig {
        id: "music".into(),
        ..Default::default()
    }]
}
pub fn definition(id: &str) -> Option<&'static WidgetDefinition> {
    BUILT_INS.iter().find(|widget| widget.id == id)
}
pub fn validate(widgets: &[WidgetConfig]) -> Result<(), String> {
    if widgets.len() > 128 {
        return Err("Shelf exceeds 128 widgets".into());
    }
    let mut ids = HashSet::new();
    for widget in widgets {
        if widget.id.is_empty()
            || widget.id.len() > 128
            || widget.id.chars().any(char::is_control)
            || (definition(&widget.id).is_some() && !(1..=2).contains(&widget.span))
            || !ids.insert(&widget.id)
        {
            return Err("Invalid widget identity, span or duplicate".into());
        }
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShelfMode {
    Empty,
    SoloMusic,
    Solo,
    Dual,
    Compact,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub id: String,
    pub row: u8,
    pub column: u8,
    pub span: u8,
}
pub fn layout(widgets: &[WidgetConfig]) -> (ShelfMode, Vec<Placement>) {
    let mut visible: Vec<_> = widgets
        .iter()
        .filter(|widget| widget.enabled && definition(&widget.id).is_some())
        .collect();
    visible.sort_by_key(|widget| widget.order);
    visible.truncate(6);
    let mode = match visible.len() {
        0 => ShelfMode::Empty,
        1 if visible[0].id == "music" => ShelfMode::SoloMusic,
        1 => ShelfMode::Solo,
        2 => ShelfMode::Dual,
        _ => ShelfMode::Compact,
    };
    let (mut row, mut column) = (0, 0);
    let placements = visible
        .into_iter()
        .map(|widget| {
            let span = if matches!(mode, ShelfMode::Solo | ShelfMode::SoloMusic) {
                2
            } else {
                widget.span.clamp(1, 2)
            };
            if column + span > 2 {
                row += 1;
                column = 0;
            }
            let placement = Placement {
                id: widget.id.clone(),
                row,
                column,
                span,
            };
            column += span;
            if column == 2 {
                row += 1;
                column = 0;
            }
            placement
        })
        .collect();
    (mode, placements)
}
/// Reordering swaps semantic order; unknown plugin entries are untouched.
pub fn move_widget(widgets: &mut [WidgetConfig], id: &str, direction: i8) -> bool {
    let mut indices: Vec<_> = (0..widgets.len())
        .filter(|index| definition(&widgets[*index].id).is_some())
        .collect();
    indices.sort_by_key(|index| widgets[*index].order);
    let Some(index) = indices.iter().position(|index| widgets[*index].id == id) else {
        return false;
    };
    let next = index as isize + direction.signum() as isize;
    if next < 0 || next >= indices.len() as isize || direction == 0 {
        return false;
    }
    indices.swap(index, next as usize);
    for (rank, index) in indices.into_iter().enumerate() {
        widgets[index].order = rank as u16;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_ids_and_nested_options_roundtrip_without_rendering() {
        let widgets: Vec<WidgetConfig> = serde_json::from_str(r#"[{"id":"future-widget","order":3,"span":2,"enabled":true,"future":{"nested":[1,null,3]}},{"id":"music","order":1}]"#).unwrap();
        let (mode, placements) = layout(&widgets);
        assert_eq!(mode, ShelfMode::SoloMusic);
        assert_eq!(placements.len(), 1);
        assert_eq!(
            serde_json::from_value::<Vec<WidgetConfig>>(serde_json::to_value(&widgets).unwrap())
                .unwrap(),
            widgets
        );
    }
    #[test]
    fn regular_grid_respects_order_span_and_keyboard_reorder() {
        let mut widgets = vec![
            WidgetConfig {
                id: "clock".into(),
                order: 2,
                ..Default::default()
            },
            WidgetConfig {
                id: "music".into(),
                order: 1,
                span: 2,
                ..Default::default()
            },
            WidgetConfig {
                id: "volume".into(),
                order: 3,
                ..Default::default()
            },
        ];
        let (mode, placements) = layout(&widgets);
        assert_eq!(mode, ShelfMode::Compact);
        assert_eq!(placements[0].span, 2);
        assert_eq!(placements[1].row, 1);
        assert!(move_widget(&mut widgets, "volume", -1));
        assert_eq!(layout(&widgets).1[1].id, "volume");
        assert!(!move_widget(&mut widgets, "music", -1));
        assert!(validate(&widgets).is_ok());
    }
}
