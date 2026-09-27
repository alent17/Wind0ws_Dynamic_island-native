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
        Ok(prefs)
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
