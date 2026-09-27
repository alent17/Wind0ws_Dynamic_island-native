//! Preserve the complete JSON document, including fields not yet used by native UI.
use crate::{preferences::AppPreferences, weather::City};
use serde_json::{Map, Value};

pub const MAX_BYTES: usize = 1024 * 1024;
pub const TOOL_KEYS: [&str; 7] = [
    "showTimerTool",
    "showVolumeTool",
    "showFloatingTool",
    "showSettingsTool",
    "showHideTool",
    "showClockTool",
    "showWeatherTool",
];
#[derive(Clone, Debug, PartialEq)]
pub struct Controls {
    pub panel: bool,
    pub tools: [bool; 7],
    pub animations: bool,
    pub reduced: bool,
    pub time_zone: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub style: String,
    pub edge: String,
    pub edge_position: u8,
    pub compact_length: u16,
    pub collapsed_shoulder_radius: u8,
    pub expanded_shoulder_radius: u8,
    pub expanded_corner_radius: u32,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            style: "floating".into(),
            edge: "top".into(),
            edge_position: 50,
            compact_length: 80,
            collapsed_shoulder_radius: 8,
            expanded_shoulder_radius: 32,
            expanded_corner_radius: 45,
        }
    }
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            panel: true,
            tools: [true; 7],
            animations: true,
            reduced: false,
            time_zone: "system".into(),
        }
    }
}
impl Controls {
    pub fn mask(&self) -> [bool; 7] {
        self.tools.map(|enabled| enabled && self.panel)
    }
}
#[derive(Clone, Debug)]
pub struct Document(Map<String, Value>);
impl Default for Document {
    fn default() -> Self {
        Self(
            serde_json::to_value(AppPreferences::default())
                .unwrap()
                .as_object()
                .unwrap()
                .clone(),
        )
    }
}
impl Document {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("配置超过 1 MiB".into());
        }
        let value: Value = serde_json::from_slice(bytes).map_err(|_| "配置不是有效 JSON")?;
        let mut fields = value.as_object().ok_or("配置根节点必须为对象")?.clone();
        for (alias, canonical) in [
            ("edgeShoulderRadius", "collapsedEdgeShoulderRadius"),
            ("autoHide", "captureHideOnFullscreen"),
        ] {
            if !fields.contains_key(canonical) {
                if let Some(v) = fields.get(alias).cloned() {
                    fields.insert(canonical.into(), v);
                }
            }
        }
        if !fields.contains_key("floatingWindowAlwaysOnTop") {
            if let Some(v) = fields.get("alwaysOnTop").cloned() {
                fields.insert("floatingWindowAlwaysOnTop".into(), v);
            }
        }
        for (key, value) in Self::default().0 {
            fields.entry(key).or_insert(value);
        }
        let doc = Self(fields);
        doc.preferences()?;
        Ok(doc)
    }
    pub fn preferences(&self) -> Result<AppPreferences, String> {
        let mut known = self.0.clone();
        // Keep aliases in the saved document, but canonical values win during typed decoding.
        known.remove("edgeShoulderRadius");
        known.remove("autoHide");
        let prefs: AppPreferences = serde_json::from_value(Value::Object(known))
            .map_err(|_| "配置字段类型或数值范围无效".to_string())?;
        if let Some(city) = &prefs.weather_location {
            if !(City {
                name: city.name.clone(),
                latitude: city.latitude,
                longitude: city.longitude,
            })
            .valid()
            {
                return Err("天气城市配置无效".into());
            }
        }
        if prefs.island_edge_position > 100 {
            return Err("贴边位置必须在 0–100 之间".into());
        }
        if !(80..=300).contains(&prefs.compact_length)
            || prefs.collapsed_edge_shoulder_radius > 16
            || prefs.expanded_edge_shoulder_radius > 64
            || prefs.expanded_corner_radius > 80
        {
            return Err("灵动岛形状参数超出允许范围".into());
        }
        Ok(prefs)
    }
    pub fn selection(&self) -> crate::Selection {
        crate::Selection {
            allowed: serde_json::from_value(self.0["selectedPlayerIds"].clone()).unwrap_or(None),
            order: serde_json::from_value(self.0["playerOrderIds"].clone()).unwrap_or_default(),
        }
    }
    pub fn set_selection(&mut self, selection: &crate::Selection) -> Result<(), String> {
        for ids in std::iter::once(&selection.order).chain(selection.allowed.iter()) {
            if ids.len() > 256
                || ids.iter().any(|id| {
                    id.is_empty() || id.chars().count() > 512 || id.chars().any(char::is_control)
                })
            {
                return Err("播放器列表无效或超过 256 项".into());
            }
        }
        self.0.insert(
            "selectedPlayerIds".into(),
            serde_json::to_value(&selection.allowed).unwrap(),
        );
        self.0.insert(
            "playerOrderIds".into(),
            serde_json::to_value(&selection.order).unwrap(),
        );
        Ok(())
    }
    pub fn city(&self) -> Option<City> {
        serde_json::from_value(self.0.get("weatherLocation")?.clone())
            .ok()
            .filter(City::valid)
    }
    pub fn controls(&self) -> Controls {
        Controls {
            panel: self.0["showCustomFunctionPanel"].as_bool().unwrap_or(true),
            tools: TOOL_KEYS.map(|key| self.0[key].as_bool().unwrap_or(true)),
            animations: self.0["enableAnimations"].as_bool().unwrap_or(true),
            reduced: self.0["reduceAnimations"].as_bool().unwrap_or(false),
            time_zone: self.0["clockTimeZone"].as_str().unwrap_or("system").into(),
        }
    }
    pub fn appearance(&self) -> Appearance {
        Appearance {
            style: self.0["islandStyle"].as_str().unwrap_or("floating").into(),
            edge: self.0["islandEdge"].as_str().unwrap_or("top").into(),
            edge_position: self.0["islandEdgePosition"]
                .as_u64()
                .and_then(|value| u8::try_from(value).ok())
                .filter(|value| *value <= 100)
                .unwrap_or(50),
            compact_length: self.0["compactLength"].as_u64().unwrap_or(80) as u16,
            collapsed_shoulder_radius: self.0["collapsedEdgeShoulderRadius"].as_u64().unwrap_or(8)
                as u8,
            expanded_shoulder_radius: self.0["expandedEdgeShoulderRadius"].as_u64().unwrap_or(32)
                as u8,
            expanded_corner_radius: self.0["expandedCornerRadius"].as_u64().unwrap_or(45) as u32,
        }
    }
    pub fn set_appearance(&mut self, appearance: &Appearance) -> Result<(), String> {
        if appearance.edge_position > 100
            || !(80..=300).contains(&appearance.compact_length)
            || appearance.collapsed_shoulder_radius > 16
            || appearance.expanded_shoulder_radius > 64
            || appearance.expanded_corner_radius > 80
        {
            return Err("灵动岛形状参数超出允许范围".into());
        }
        self.0.insert(
            "islandStyle".into(),
            Value::String(appearance.style.clone()),
        );
        self.0
            .insert("islandEdge".into(), Value::String(appearance.edge.clone()));
        self.0.insert(
            "islandEdgePosition".into(),
            Value::from(appearance.edge_position),
        );
        self.0.insert(
            "compactLength".into(),
            Value::from(appearance.compact_length),
        );
        self.0.insert(
            "collapsedEdgeShoulderRadius".into(),
            Value::from(appearance.collapsed_shoulder_radius),
        );
        self.0.insert(
            "expandedEdgeShoulderRadius".into(),
            Value::from(appearance.expanded_shoulder_radius),
        );
        self.0.insert(
            "expandedCornerRadius".into(),
            Value::from(appearance.expanded_corner_radius),
        );
        Ok(())
    }
    pub fn set_controls(&mut self, controls: &Controls) {
        self.0.insert(
            "clockTimeZone".into(),
            Value::String(controls.time_zone.clone()),
        );
        for (key, value) in TOOL_KEYS.into_iter().zip(controls.tools).chain([
            ("showCustomFunctionPanel", controls.panel),
            ("enableAnimations", controls.animations),
            ("reduceAnimations", controls.reduced),
        ]) {
            self.0.insert(key.into(), Value::Bool(value));
        }
    }
    pub fn set_city(&mut self, city: &City) -> Result<(), String> {
        if !city.valid() {
            return Err("天气城市配置无效".into());
        }
        let mut value = self
            .0
            .get("weatherLocation")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (key, value_new) in serde_json::to_value(city).unwrap().as_object().unwrap() {
            value.insert(key.clone(), value_new.clone());
        }
        self.0
            .insert("weatherLocation".into(), Value::Object(value));
        Ok(())
    }
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        let bytes = serde_json::to_vec_pretty(&self.0).map_err(|_| "配置序列化失败")?;
        if bytes.len() > MAX_BYTES {
            return Err("保存的配置超过 1 MiB".into());
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(dead_code)]
    mod legacy {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../src-tauri/src/models/settings.rs"
        ));
    }
    #[test]
    fn defaults_match_every_legacy_field() {
        assert_eq!(
            serde_json::to_value(AppPreferences::default()).unwrap(),
            serde_json::to_value(legacy::AppPreferences::default()).unwrap()
        );
    }
    #[test]
    fn controls_roundtrip_without_changing_city_or_unknown_fields() {
        let mut doc = Document::parse(
            br#"{"future":[1,2],"weatherLocation":{"name":"Test","latitude":1,"longitude":2}}"#,
        )
        .unwrap();
        let before = doc.city();
        let controls = Controls {
            panel: false,
            tools: [true, false, true, false, true, false, true],
            animations: false,
            reduced: true,
            time_zone: "America/New_York".into(),
        };
        doc.set_controls(&controls);
        let again = Document::parse(&doc.bytes().unwrap()).unwrap();
        assert_eq!(again.controls(), controls);
        assert_eq!(again.city(), before);
        assert_eq!(again.0["future"], serde_json::json!([1, 2]));
        assert_eq!(controls.mask(), [false; 7]);
    }
    #[test]
    fn editing_other_controls_keeps_unrecognized_legacy_timezone() {
        let mut doc = Document::parse(br#"{"clockTimeZone":"Unknown/Legacy"}"#).unwrap();
        let mut controls = doc.controls();
        controls.animations = false;
        doc.set_controls(&controls);
        assert_eq!(
            Document::parse(&doc.bytes().unwrap())
                .unwrap()
                .controls()
                .time_zone,
            "Unknown/Legacy"
        );
    }
    #[test]
    fn appearance_roundtrip_preserves_unrelated_values_and_checks_position() {
        let mut doc = Document::parse(br#"{"future":{"keep":true},"islandStyle":"edge","islandEdge":"right","islandEdgePosition":17}"#).unwrap();
        let mut appearance = doc.appearance();
        assert_eq!(
            appearance,
            Appearance {
                style: "edge".into(),
                edge: "right".into(),
                edge_position: 17,
                ..Appearance::default()
            }
        );
        appearance.edge_position = 83;
        appearance.compact_length = 164;
        appearance.collapsed_shoulder_radius = 12;
        appearance.expanded_shoulder_radius = 54;
        appearance.expanded_corner_radius = 70;
        doc.set_appearance(&appearance).unwrap();
        let loaded = Document::parse(&doc.bytes().unwrap()).unwrap();
        assert_eq!(loaded.appearance(), appearance);
        assert_eq!(loaded.0["future"]["keep"], true);
        assert!(doc
            .set_appearance(&Appearance {
                edge_position: 101,
                ..appearance
            })
            .is_err());
        assert!(Document::parse(br#"{"islandEdgePosition":255}"#).is_err());
        for invalid in [
            br#"{"compactLength":79}"#.as_slice(),
            br#"{"collapsedEdgeShoulderRadius":17}"#,
            br#"{"expandedEdgeShoulderRadius":65}"#,
            br#"{"expandedCornerRadius":81}"#,
        ] {
            assert!(Document::parse(invalid).is_err());
        }
    }
    #[test]
    fn player_selection_distinguishes_all_from_none_and_preserves_unrelated_values() {
        let mut doc = Document::parse(
            br#"{"playerWeights":{"custom":73},"future":{"keep":true},"clockTimeZone":"UTC"}"#,
        )
        .unwrap();
        let none = crate::Selection {
            allowed: Some(vec![]),
            order: vec!["offline".into()],
        };
        doc.set_selection(&none).unwrap();
        let loaded = Document::parse(&doc.bytes().unwrap()).unwrap();
        assert_eq!(loaded.selection(), none);
        assert_eq!(loaded.0["playerWeights"]["custom"], 73);
        assert_eq!(loaded.0["future"]["keep"], true);
        assert_eq!(loaded.controls().time_zone, "UTC");
        doc.set_selection(&crate::Selection::default()).unwrap();
        assert!(Document::parse(&doc.bytes().unwrap())
            .unwrap()
            .selection()
            .allowed
            .is_none());
        assert!(doc
            .set_selection(&crate::Selection {
                allowed: Some(vec!["\n".into()]),
                order: vec![]
            })
            .is_err());
    }
    #[test]
    fn edits_preserve_unknown_root_nested_and_unused_fields() {
        let mut doc = Document::parse(br#"{"playerOrderIds":["b","a"],"autoStart":true,"future":{"list":[null,{"x":2}]},"weatherLocation":{"name":"A","latitude":1,"longitude":2,"provider":"custom"}}"#).unwrap();
        doc.set_city(&City {
            name: "B".into(),
            latitude: 3.,
            longitude: 4.,
        })
        .unwrap();
        let saved: Value = serde_json::from_slice(&doc.bytes().unwrap()).unwrap();
        assert_eq!(saved["future"]["list"][1]["x"], 2);
        assert_eq!(saved["playerOrderIds"], serde_json::json!(["b", "a"]));
        assert_eq!(saved["autoStart"], true);
        assert_eq!(saved["weatherLocation"]["provider"], "custom");
        assert_eq!(
            Document::parse(&doc.bytes().unwrap())
                .unwrap()
                .city()
                .unwrap()
                .name,
            "B"
        );
    }
    #[test]
    fn canonical_fields_win_aliases_and_independent_topmost_is_inherited() {
        let doc =
            Document::parse(br#"{"autoHide":false,"edgeShoulderRadius":9,"alwaysOnTop":false}"#)
                .unwrap();
        let p = doc.preferences().unwrap();
        assert!(!p.capture_hide_on_fullscreen && !p.floating_window_always_on_top);
        assert_eq!(p.collapsed_edge_shoulder_radius, 9);
        let doc = Document::parse(br#"{"autoHide":false,"captureHideOnFullscreen":true,"edgeShoulderRadius":9,"collapsedEdgeShoulderRadius":12,"alwaysOnTop":false,"floatingWindowAlwaysOnTop":true}"#).unwrap();
        let p = doc.preferences().unwrap();
        assert!(p.capture_hide_on_fullscreen && p.floating_window_always_on_top);
        assert_eq!(p.collapsed_edge_shoulder_radius, 12);
    }
    #[test]
    fn invalid_documents_cannot_be_silently_defaulted_and_saved() {
        for bytes in [
            b"null".as_slice(),
            b"[]",
            b"{",
            br#"{"windowOpacity":999}"#,
            br#"{"showClockTool":"yes"}"#,
            br#"{"weatherLocation":{"name":"bad","latitude":91,"longitude":0}}"#,
        ] {
            assert!(Document::parse(bytes).is_err());
        }
        assert!(Document::parse(&vec![b' '; MAX_BYTES + 1]).is_err());
    }
}
