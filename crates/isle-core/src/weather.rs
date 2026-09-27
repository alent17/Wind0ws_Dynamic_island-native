use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct City {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}
impl City {
    pub fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.chars().count() <= 150
            && !self.name.chars().any(char::is_control)
            && self.latitude.is_finite()
            && self.longitude.is_finite()
            && (-90. ..=90.).contains(&self.latitude)
            && (-180. ..=180.).contains(&self.longitude)
    }
    pub fn same(&self, other: &Self) -> bool {
        self.latitude == other.latitude && self.longitude == other.longitude
    }
}
#[derive(Clone, Debug)]
pub struct Day {
    pub date: String,
    pub code: u32,
    pub high: f64,
    pub low: f64,
}
#[derive(Clone, Debug)]
pub struct Forecast {
    pub temperature: f64,
    pub code: u32,
    pub observed: String,
    pub days: Vec<Day>,
}
#[derive(Default)]
pub struct View {
    pub city: Option<City>,
    pub data: Option<Forecast>,
    pub failed: bool,
}
fn number(v: &Value) -> Option<f64> {
    v.as_f64()
        .filter(|n| n.is_finite() && (-100. ..=70.).contains(n))
}
fn code(v: &Value) -> Option<u32> {
    v.as_u64().filter(|v| *v <= 99).map(|v| v as u32)
}
fn date(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
}
pub fn parse_forecast(bytes: &[u8]) -> Option<Forecast> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    let current = &v["current"];
    let daily = &v["daily"];
    let observed = current["time"].as_str()?.to_string();
    if observed.len() != 16 || !observed.is_ascii() || !date(&observed[..10]) {
        return None;
    }
    let mut days = Vec::new();
    for (i, value) in daily["time"].as_array()?.iter().take(4).enumerate() {
        let day = value.as_str()?;
        if !date(day) {
            return None;
        }
        if day <= &observed[..10] {
            continue;
        }
        let high = number(&daily["temperature_2m_max"][i])?;
        let low = number(&daily["temperature_2m_min"][i])?;
        if low > high || days.last().is_some_and(|d: &Day| d.date.as_str() >= day) {
            return None;
        }
        days.push(Day {
            date: day.into(),
            code: code(&daily["weather_code"][i])?,
            high,
            low,
        });
    }
    if days.len() != 3 {
        return None;
    }
    Some(Forecast {
        temperature: number(&current["temperature_2m"])?,
        code: code(&current["weather_code"])?,
        observed,
        days,
    })
}
pub struct Candidate {
    pub city: City,
    pub rank: (bool, u64),
}
pub fn parse_cities(bytes: &[u8]) -> Option<Vec<Candidate>> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    if v.get("error").is_some() {
        return None;
    }
    let Some(rows) = v.get("results") else {
        return Some(vec![]);
    };
    let mut rows = rows.as_array()?.iter().take(16).collect::<Vec<_>>();
    rows.sort_by_key(|v| {
        std::cmp::Reverse((
            matches!(
                v["feature_code"].as_str().unwrap_or(""),
                "PPLC" | "PPLA" | "PPLA2" | "PPLA3" | "PPLA4"
            ),
            v["population"].as_u64().unwrap_or(0),
        ))
    });
    let mut result = vec![];
    for v in rows {
        let city = City {
            name: ["name", "admin1", "country"]
                .iter()
                .filter_map(|key| v[*key].as_str())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
            latitude: v["latitude"].as_f64()?,
            longitude: v["longitude"].as_f64()?,
        };
        if city.valid() && !result.iter().any(|c: &Candidate| c.city.same(&city)) {
            let rank = (
                matches!(
                    v["feature_code"].as_str().unwrap_or(""),
                    "PPLC" | "PPLA" | "PPLA2" | "PPLA3" | "PPLA4"
                ),
                v["population"].as_u64().unwrap_or(0),
            );
            result.push(Candidate { city, rank });
        }
    }
    Some(result)
}
pub fn settings(bytes: &[u8]) -> Option<City> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    serde_json::from_value::<City>(v.get("weatherLocation")?.clone())
        .ok()
        .filter(City::valid)
}
pub fn settings_bytes(city: &City) -> Vec<u8> {
    serde_json::to_vec_pretty(&serde_json::json!({"version":1,"weatherLocation":city})).unwrap()
}
pub fn description(code: u32) -> &'static str {
    match code {
        0 => "晴",
        1 => "晴间多云",
        2 => "多云",
        3 => "阴",
        45 | 48 => "雾",
        51..=57 => "毛毛雨",
        61..=67 => "雨",
        71..=77 => "雪",
        80..=82 => "阵雨",
        85 | 86 => "阵雪",
        95..=99 => "雷雨",
        _ => "未知天气",
    }
}
struct Entry {
    city: City,
    data: Option<Forecast>,
    at: u64,
    failed: bool,
}
#[derive(Default)]
pub struct Cache {
    entries: VecDeque<Entry>,
}
impl Cache {
    pub fn lookup(&self, city: &City, now: u64) -> (Option<Forecast>, bool, bool) {
        match self.entries.iter().find(|e| e.city.same(city)) {
            Some(e) => (
                e.data.clone(),
                now.saturating_sub(e.at) >= if e.failed { 60 } else { 1800 },
                e.failed,
            ),
            None => (None, true, false),
        }
    }
    pub fn record(&mut self, city: City, data: Option<Forecast>, now: u64) {
        let prior = self
            .entries
            .iter()
            .position(|e| e.city.same(&city))
            .and_then(|i| self.entries.remove(i));
        let failed = data.is_none();
        let data = data.or_else(|| prior.and_then(|e| e.data));
        self.entries.push_back(Entry {
            city,
            data,
            at: now,
            failed,
        });
        while self.entries.len() > 4 {
            self.entries.pop_front();
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn city(i: u32) -> City {
        City {
            name: format!("city{i}"),
            latitude: i as f64,
            longitude: 0.,
        }
    }
    fn data() -> Forecast {
        Forecast {
            temperature: 20.,
            code: 0,
            observed: "2026-09-27T12:00".into(),
            days: vec![],
        }
    }
    #[test]
    fn cache_is_bounded_keeps_only_same_city_on_failure_and_backs_off() {
        let mut cache = Cache::default();
        cache.record(city(0), Some(data()), 0);
        cache.record(city(0), None, 1800);
        let (data, needed, failed) = cache.lookup(&city(0), 1801);
        assert!(data.is_some() && !needed && failed);
        assert!(cache.lookup(&city(0), 1860).1);
        assert!(cache.lookup(&city(1), 1801).0.is_none());
        for i in 1..7 {
            cache.record(city(i), None, 1900);
        }
        assert_eq!(cache.len(), 4);
        assert!(cache.lookup(&city(0), 2000).0.is_none());
    }
    #[test]
    fn city_ranking_survives_merging_queries_and_deduplicates_coordinates() {
        let rural = parse_cities(br#"{"results":[{"name":"Shanghai","latitude":41.05,"longitude":-90.5,"feature_code":"PPL","population":100}]}"#).unwrap();
        let metro = parse_cities(br#"{"results":[{"name":"Shanghai","latitude":31.22,"longitude":121.46,"feature_code":"PPLA","population":24874500},{"name":"duplicate","latitude":31.22,"longitude":121.46}]}"#).unwrap();
        assert_eq!(metro.len(), 1);
        let mut merged = rural;
        merged.extend(metro);
        merged.sort_by_key(|item| std::cmp::Reverse(item.rank));
        assert_eq!(merged[0].city.latitude, 31.22);
        assert!(parse_cities(br#"{"error":true}"#).is_none());
    }
    #[test]
    fn settings_validate_coordinates_and_roundtrip_chinese_names() {
        let c = City {
            name: "上海市 · 中国".into(),
            latitude: 31.2,
            longitude: 121.5,
        };
        assert_eq!(settings(&settings_bytes(&c)), Some(c));
        assert!(
            settings(br#"{"weatherLocation":{"name":"bad","latitude":91,"longitude":1}}"#)
                .is_none()
        );
    }
    #[test]
    fn forecast_rejects_null_and_inconsistent_days() {
        let bytes=br#"{"current":{"time":"2026-09-27T12:00","temperature_2m":20,"weather_code":0},"daily":{"time":["2026-09-27","2026-09-28","2026-09-29","2026-09-30"],"weather_code":[0,1,2,3],"temperature_2m_max":[20,21,22,23],"temperature_2m_min":[10,11,12,13]}}"#;
        let good = parse_forecast(bytes).unwrap();
        assert_eq!(good.days[0].date, "2026-09-28");
        assert_eq!(good.days.len(), 3);
        let bad = String::from_utf8_lossy(bytes).replace("20,21,22,23", "20,null,22,23");
        assert!(parse_forecast(bad.as_bytes()).is_none());
    }
}
