//! Use Windows timezone rules; never change the machine clock or timezone.
use std::sync::OnceLock;
use windows::Win32::{
    Foundation::SYSTEMTIME,
    System::{
        SystemInformation::GetSystemTime,
        Time::{
            EnumDynamicTimeZoneInformation, SystemTimeToTzSpecificLocalTimeEx,
            DYNAMIC_TIME_ZONE_INFORMATION,
        },
    },
};
pub const ZONES: [(&str, &str); 6] = [
    ("system", "跟随系统"),
    ("Asia/Taipei", "台北"),
    ("Asia/Tokyo", "东京"),
    ("America/New_York", "纽约"),
    ("Europe/London", "伦敦"),
    ("UTC", "UTC"),
];
const WINDOWS_KEYS: [&str; 4] = [
    "Taipei Standard Time",
    "Tokyo Standard Time",
    "Eastern Standard Time",
    "GMT Standard Time",
];
fn rules() -> &'static [Option<DYNAMIC_TIME_ZONE_INFORMATION>; 4] {
    static RULES: OnceLock<[Option<DYNAMIC_TIME_ZONE_INFORMATION>; 4]> = OnceLock::new();
    RULES.get_or_init(|| {
        let mut found = [None; 4];
        for index in 0..512 {
            let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
            if unsafe { EnumDynamicTimeZoneInformation(index, &mut info) } != 0 {
                break;
            }
            let end = info
                .TimeZoneKeyName
                .iter()
                .position(|v| *v == 0)
                .unwrap_or(info.TimeZoneKeyName.len());
            let name = String::from_utf16_lossy(&info.TimeZoneKeyName[..end]);
            if let Some(i) = WINDOWS_KEYS.iter().position(|key| *key == name) {
                found[i] = Some(info);
            }
        }
        found
    })
}
pub fn warm_up() {
    let _ = rules();
}
pub fn convert(zone: &str, utc: &SYSTEMTIME) -> Option<SYSTEMTIME> {
    if zone == "UTC" {
        return Some(*utc);
    }
    let rule = if zone == "system" {
        None
    } else {
        let index = ZONES.iter().position(|(id, _)| *id == zone)?;
        Some(rules().get(index.checked_sub(1)?)?.as_ref()? as *const _)
    };
    let mut local = SYSTEMTIME::default();
    unsafe { SystemTimeToTzSpecificLocalTimeEx(rule, utc, &mut local) }.ok()?;
    Some(local)
}
pub fn now(zone: &str) -> Option<SYSTEMTIME> {
    convert(zone, &unsafe { GetSystemTime() })
}
pub fn label(zone: &str) -> &str {
    ZONES
        .iter()
        .find(|(id, _)| *id == zone)
        .map(|(_, label)| *label)
        .unwrap_or("不支持的时区 · 请按 F8 设置")
}
#[cfg(test)]
mod tests {
    use super::*;
    fn utc(month: u16, day: u16, hour: u16, minute: u16) -> SYSTEMTIME {
        SYSTEMTIME {
            wYear: 2024,
            wMonth: month,
            wDay: day,
            wHour: hour,
            wMinute: minute,
            ..Default::default()
        }
    }
    fn hm(zone: &str, utc: SYSTEMTIME) -> (u16, u16, u16) {
        let time = convert(zone, &utc).expect("Windows timezone installed");
        (time.wDay, time.wHour, time.wMinute)
    }
    #[test]
    fn new_york_dst_skips_and_repeats_without_fixed_offset_assumptions() {
        assert_eq!(hm("America/New_York", utc(3, 10, 6, 59)), (10, 1, 59));
        assert_eq!(hm("America/New_York", utc(3, 10, 7, 0)), (10, 3, 0));
        assert_eq!(hm("America/New_York", utc(11, 3, 5, 59)), (3, 1, 59));
        assert_eq!(hm("America/New_York", utc(11, 3, 6, 0)), (3, 1, 0));
    }
    #[test]
    fn london_dst_and_asian_date_rollovers() {
        assert_eq!(hm("Europe/London", utc(3, 31, 0, 59)), (31, 0, 59));
        assert_eq!(hm("Europe/London", utc(3, 31, 1, 0)), (31, 2, 0));
        let t = convert("Asia/Taipei", &utc(12, 31, 23, 30)).unwrap();
        assert_eq!((t.wYear, t.wMonth, t.wDay, t.wHour), (2025, 1, 1, 7));
        assert_eq!(hm("Asia/Tokyo", utc(6, 1, 23, 30)), (2, 8, 30));
        assert_eq!(hm("UTC", utc(6, 1, 23, 30)), (1, 23, 30));
        assert!(convert("Unknown/Test", &utc(6, 1, 23, 30)).is_none());
    }
}
