use isle_ui::geometry::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    General,
    Appearance,
    Modules,
    Media,
    Weather,
    Advanced,
}

impl Page {
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Appearance,
        Self::Modules,
        Self::Media,
        Self::Weather,
        Self::Advanced,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::General => 0,
            Self::Appearance => 1,
            Self::Modules => 2,
            Self::Media => 3,
            Self::Weather => 4,
            Self::Advanced => 5,
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::General => "常规",
            Self::Appearance => "外观",
            Self::Modules => "模块",
            Self::Media => "媒体",
            Self::Weather => "天气",
            Self::Advanced => "高级",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::General => "控制 Isle 的基本行为和系统集成。",
            Self::Appearance => "调整真实灵动岛的形状、位置与颜色。",
            Self::Modules => "选择工具栏中可用的 Isle 功能。",
            Self::Media => "管理播放器相关选项和播放来源。",
            Self::Weather => "为天气模块选择一个城市。",
            Self::Advanced => "查看保存状态，并处理尚未写入的更改。",
        }
    }
}

pub const WINDOW_WIDTH: f32 = 820.0;
pub const WINDOW_HEIGHT: f32 = 760.0;
pub const MIN_WIDTH: f32 = 720.0;
pub const MIN_HEIGHT: f32 = 640.0;
pub const NAV_WIDTH: f32 = 184.0;
pub const CONTENT_LEFT: f32 = 208.0;
pub const PAGE_TOP: f32 = 144.0;
const NAV_NORMAL_HEIGHT: f32 = 426.0;
const NAV_COMPACT_HEIGHT: f32 = 270.0;

/// Returns the six navigation button bounds in DIPs, in `Page::ALL` order.
/// The normal-height layout intentionally matches the original 118 DIP top,
/// 50 DIP pitch, and 42 DIP buttons.
pub fn navigation_layout(client_height: f32) -> [Rect; 6] {
    let height = client_height.max(0.0);
    if height >= NAV_NORMAL_HEIGHT {
        return std::array::from_fn(|index| Rect {
            x: 12.0,
            y: 118.0 + index as f32 * 50.0,
            w: 160.0,
            h: 42.0,
        });
    }

    if height >= NAV_COMPACT_HEIGHT {
        const GAP: f32 = 2.0;
        const TOP_MIN: f32 = 84.0;
        const TOP_MAX: f32 = 118.0;
        const BOTTOM_MARGIN: f32 = 8.0;
        let row_height = ((height - TOP_MIN - BOTTOM_MARGIN - GAP * 5.0) / 6.0).clamp(28.0, 42.0);
        let group_height = row_height * 6.0 + GAP * 5.0;
        let top = (height - group_height - BOTTOM_MARGIN).clamp(TOP_MIN, TOP_MAX);
        return std::array::from_fn(|index| Rect {
            x: 12.0,
            y: top + index as f32 * (row_height + GAP),
            w: 160.0,
            h: row_height,
        });
    }

    const SIDE_MARGIN: f32 = 8.0;
    const COLUMN_GAP: f32 = 8.0;
    const ROW_GAP: f32 = 6.0;
    let button_width = (NAV_WIDTH - SIDE_MARGIN * 2.0 - COLUMN_GAP) / 2.0;
    let button_height = ((height - SIDE_MARGIN * 2.0 - ROW_GAP * 2.0) / 3.0).clamp(0.0, 42.0);
    let group_height = button_height * 3.0 + ROW_GAP * 2.0;
    let top = ((height - group_height) / 2.0).max(0.0);
    std::array::from_fn(|index| {
        let column = index % 2;
        let row = index / 2;
        Rect {
            x: SIDE_MARGIN + column as f32 * (button_width + COLUMN_GAP),
            y: top + row as f32 * (button_height + ROW_GAP),
            w: button_width,
            h: button_height,
        }
    })
}

pub fn navigation_is_compact(client_height: f32) -> bool {
    client_height < NAV_COMPACT_HEIGHT
}

pub const WINDOW_BG: (u8, u8, u8) = (12, 15, 19);
pub const SURFACE: (u8, u8, u8) = (18, 22, 28);
pub const CARD: (u8, u8, u8) = (23, 28, 35);
pub const CARD_HOVER: (u8, u8, u8) = (29, 36, 45);
pub const PRIMARY_TEXT: (u8, u8, u8) = (245, 247, 250);
pub const SECONDARY_TEXT: (u8, u8, u8) = (166, 175, 187);
pub const MUTED_TEXT: (u8, u8, u8) = (113, 123, 136);

#[cfg(test)]
mod navigation_tests {
    use super::*;

    fn assert_navigation_fits_without_overlap(height: f32) {
        let nav = navigation_layout(height);
        for item in &nav {
            assert!(item.x >= 0.0 && item.y >= 0.0);
            assert!(item.x + item.w <= NAV_WIDTH);
            assert!(item.y + item.h <= height);
            assert!(item.w > 0.0 && item.h > 0.0);
        }
        for left in 0..nav.len() {
            for right in left + 1..nav.len() {
                let a = nav[left];
                let b = nav[right];
                assert!(
                    a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y,
                    "navigation controls overlap at {height} DIP: {a:?} / {b:?}"
                );
            }
        }
    }

    #[test]
    fn normal_height_preserves_original_six_row_layout() {
        let nav = navigation_layout(426.0);
        assert_eq!(
            nav[0],
            Rect {
                x: 12.0,
                y: 118.0,
                w: 160.0,
                h: 42.0
            }
        );
        assert_eq!(
            nav[5],
            Rect {
                x: 12.0,
                y: 368.0,
                w: 160.0,
                h: 42.0
            }
        );
        assert!(!navigation_is_compact(426.0));
        assert_navigation_fits_without_overlap(426.0);
    }

    #[test]
    fn medium_height_compresses_six_rows_and_keeps_all_pages_visible() {
        for height in [270.0, 320.0] {
            let nav = navigation_layout(height);
            assert!(nav.iter().all(|item| item.h >= 28.0));
            assert_eq!(
                nav.iter().map(|item| item.x).collect::<Vec<_>>(),
                vec![12.0; 6]
            );
            assert!(!navigation_is_compact(height));
            assert_navigation_fits_without_overlap(height);
        }
    }

    #[test]
    fn short_two_hundred_percent_dpi_client_uses_compact_two_by_three_layout() {
        let client_height_dip = 440.0 / 2.0;
        let nav = navigation_layout(client_height_dip);
        assert!(navigation_is_compact(client_height_dip));
        assert_eq!(nav[0].y, nav[1].y);
        assert_eq!(nav[2].y, nav[3].y);
        assert_eq!(nav[4].y, nav[5].y);
        assert!(nav[1].x > nav[0].x);
        assert!(nav[2].y > nav[0].y);
        assert!(nav.iter().all(|item| item.h >= 14.0));
        assert_navigation_fits_without_overlap(client_height_dip);
    }
}
