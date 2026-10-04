use crate::{
    geometry::{offset, Edge, Rect},
    model::{Hit, Page, HOST},
    state::PrimarySurfaceMode,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementLayout {
    pub rect: Rect,
    pub radius: f32,
    pub opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlLayout {
    pub hit: Hit,
    pub rect: Rect,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitRegion {
    pub hit: Hit,
    pub rect: Rect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutSnapshot {
    pub surface: Rect,
    pub radius: f32,
    pub shoulder: f32,
    pub album: Option<ElementLayout>,
    pub title: Option<ElementLayout>,
    pub artist: Option<ElementLayout>,
    pub progress: Option<ElementLayout>,
    pub controls: Vec<ControlLayout>,
    pub favorite: Option<ElementLayout>,
    pub favorite_state: Option<bool>,
    pub toolbar: Vec<ElementLayout>,
    pub activities: Vec<ElementLayout>,
    pub hit_regions: Vec<HitRegion>,
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutInput {
    pub edge: Edge,
    pub attached: bool,
    pub mode: PrimarySurfaceMode,
    pub page: Page,
    pub hovered: bool,
    pub timer_active: bool,
    pub timer_finished: bool,
    pub compact_length: f32,
    pub collapsed_shoulder: f32,
    pub expanded_shoulder: f32,
    pub corner_radius: f32,
    pub tool_ids: [bool; 7],
    pub previous: bool,
    pub play_pause: bool,
    pub next: bool,
    pub seek: bool,
    pub favorite_state: Option<bool>,
}

/// Places the highest-priority compact activities beside the single island
/// surface. The capsules share the island HWND and remain inside its 480-DIP
/// host on every edge.
pub fn activity_slots(surface: Rect, edge: Edge, count: usize) -> Vec<ElementLayout> {
    const SLOT_WIDTH: f32 = 76.;
    const SLOT_HEIGHT: f32 = 28.;
    const GAP: f32 = 4.;

    let count = count.min(2);
    (0..count)
        .map(|index| {
            let mut rect = if matches!(edge, Edge::Left | Edge::Right) {
                Rect {
                    x: surface.x + (surface.w - SLOT_WIDTH) * 0.5,
                    y: if index == 0 {
                        surface.y - GAP - SLOT_HEIGHT
                    } else {
                        surface.y + surface.h + GAP
                    },
                    w: SLOT_WIDTH,
                    h: SLOT_HEIGHT,
                }
            } else {
                Rect {
                    x: if index == 0 {
                        surface.x - GAP - SLOT_WIDTH
                    } else {
                        surface.x + surface.w + GAP
                    },
                    y: surface.y + (surface.h - SLOT_HEIGHT) * 0.5,
                    w: SLOT_WIDTH,
                    h: SLOT_HEIGHT,
                }
            };
            rect.x = rect.x.clamp(0., HOST - SLOT_WIDTH);
            rect.y = rect.y.clamp(0., HOST - SLOT_HEIGHT);
            ElementLayout {
                rect,
                radius: SLOT_HEIGHT * 0.5,
                opacity: 1.,
            }
        })
        .collect()
}

/// All returned coordinates are host-local DIPs. Edge changes only the
/// surface anchor and attached shoulder; text and controls stay readable.
pub fn compute(input: LayoutInput) -> LayoutSnapshot {
    let expanded = matches!(input.mode, PrimarySurfaceMode::Expanded(_));
    let (w, h, radius, shoulder) = if expanded {
        let shoulder = if input.attached {
            input.expanded_shoulder.clamp(0., 64.)
        } else {
            0.
        };
        let (width, height) = match input.page {
            Page::Weather => (396., 300.),
            Page::Music => (
                448.,
                if input.tool_ids.iter().any(|visible| *visible) {
                    216.
                } else {
                    188.
                },
            ),
            _ => (396., 228.),
        };
        (width, height, input.corner_radius.clamp(8., 80.), shoulder)
    } else {
        let length = input.compact_length.clamp(80., 300.).max(
            if input.timer_active || input.timer_finished {
                240.
            } else {
                80.
            },
        );
        let length = if input.hovered {
            (length + 8.).min(300.)
        } else {
            length
        };
        let thickness = if input.hovered { 30. } else { 28. };
        let (w, h) = match input.edge {
            Edge::Left | Edge::Right => (thickness, length),
            Edge::Top | Edge::Bottom => (length, thickness),
        };
        (w, h, 14., input.collapsed_shoulder.clamp(0., 16.))
    };

    let origin = offset(w, h, HOST, input.edge, input.attached);
    let surface = Rect {
        x: origin.x,
        y: origin.y,
        w,
        h,
    };
    let mut snapshot = LayoutSnapshot {
        surface,
        radius,
        shoulder,
        ..Default::default()
    };

    let compact_album = compact_album_rect(surface, input.edge, input.attached, shoulder);
    if expanded && input.page == Page::Music {
        let (body_x, body_y, body_w, body_h) = music_body(
            surface,
            input.edge,
            input.attached,
            shoulder,
            input.tool_ids.iter().any(|v| *v),
        );
        let album = Rect {
            x: body_x,
            y: body_y + 5.,
            w: 64.,
            h: 64.,
        };
        let title = Rect {
            x: body_x + 82.,
            y: body_y + 10.,
            w: (body_w - 124.).max(40.),
            h: 24.,
        };
        let artist = Rect {
            x: title.x,
            y: title.y + 27.,
            w: title.w,
            h: 20.,
        };
        let progress = Rect {
            x: body_x + 14.,
            y: body_y + 87.,
            w: (body_w - 28.).max(40.),
            h: 20.,
        };
        let visible_controls: Vec<Hit> = [
            input.previous.then_some(Hit::Previous),
            input.play_pause.then_some(Hit::Play),
            input.next.then_some(Hit::Next),
        ]
        .into_iter()
        .flatten()
        .collect();
        let control_width = visible_controls.len() as f32 * 56.;
        let controls_left = body_x + (body_w - control_width) * 0.5;
        let mut controls = Vec::new();
        if input.seek {
            controls.push(ControlLayout {
                hit: Hit::Seek,
                rect: progress,
                enabled: true,
            });
            snapshot.hit_regions.push(HitRegion {
                hit: Hit::Seek,
                rect: progress,
            });
        }
        for (i, hit) in visible_controls.into_iter().enumerate() {
            let button_size = if hit == Hit::Play {
                (48., 46.)
            } else {
                (40., 40.)
            };
            let rect = Rect {
                x: controls_left + i as f32 * 56. + (56. - button_size.0) * 0.5,
                y: body_y + 110.,
                w: button_size.0,
                h: button_size.1,
            };
            controls.push(ControlLayout {
                hit,
                rect,
                enabled: true,
            });
            snapshot.hit_regions.push(HitRegion { hit, rect });
        }
        if let Some(liked) = input.favorite_state {
            snapshot.favorite = Some(ElementLayout {
                rect: Rect {
                    x: surface.x + surface.w * 0.285 - 16.,
                    y: body_y + 112.,
                    w: 32.,
                    h: 32.,
                },
                radius: 16.,
                opacity: 1.,
            });
            snapshot.favorite_state = Some(liked);
        }
        snapshot.album = Some(ElementLayout {
            rect: album,
            radius: 16.,
            opacity: 1.,
        });
        snapshot.title = Some(ElementLayout {
            rect: title,
            radius: 0.,
            opacity: 1.,
        });
        snapshot.artist = Some(ElementLayout {
            rect: artist,
            radius: 0.,
            opacity: 1.,
        });
        snapshot.progress = Some(ElementLayout {
            rect: progress,
            radius: 2.,
            opacity: 1.,
        });
        snapshot.controls = controls;
        snapshot.toolbar = toolbar_layout(surface, input.tool_ids);
        // Keep a live album target even while collapsing into the compact state.
        let _ = body_h;
    } else {
        snapshot.album = Some(ElementLayout {
            rect: compact_album,
            radius: 10.,
            opacity: 1.,
        });
        snapshot.toolbar = if expanded {
            toolbar_layout(surface, input.tool_ids)
        } else {
            vec![]
        };
    }
    snapshot
}

fn compact_album_rect(surface: Rect, edge: Edge, attached: bool, shoulder: f32) -> Rect {
    let inset = if attached { shoulder.max(0.) } else { 0. };
    let (x, y) = match edge {
        Edge::Left | Edge::Right => (surface.x + (surface.w - 20.) * 0.5, surface.y + inset + 4.),
        Edge::Top | Edge::Bottom => (surface.x + inset + 4., surface.y + (surface.h - 20.) * 0.5),
    };
    Rect {
        x,
        y,
        w: 20.,
        h: 20.,
    }
}

fn music_body(
    surface: Rect,
    edge: Edge,
    attached: bool,
    shoulder: f32,
    has_toolbar: bool,
) -> (f32, f32, f32, f32) {
    let side_inset = if attached && matches!(edge, Edge::Top | Edge::Bottom) {
        shoulder
    } else {
        0.
    };
    let top_inset = if attached && matches!(edge, Edge::Left | Edge::Right) {
        shoulder
    } else {
        0.
    };
    let toolbar = if has_toolbar { 40. } else { 0. };
    (
        surface.x + 28. + side_inset,
        surface.y + top_inset + toolbar + 12.,
        (surface.w - 56. - side_inset * 2.).max(40.),
        surface.h - top_inset - toolbar - 12.,
    )
}

fn toolbar_layout(surface: Rect, tool_ids: [bool; 7]) -> Vec<ElementLayout> {
    let visible: Vec<usize> = tool_ids
        .into_iter()
        .enumerate()
        .filter_map(|(i, show)| show.then_some(i))
        .collect();
    let width = (visible.len().min(5) as f32 * 32. - 4.).max(0.);
    visible
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(slot, _)| ElementLayout {
            rect: Rect {
                x: surface.x + (surface.w - width) * 0.5 + 1. + slot as f32 * 32.,
                y: surface.y + 7.,
                w: 28.,
                h: 28.,
            },
            radius: 12.,
            opacity: 1.,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ExpandedView;

    #[test]
    fn expanded_music_layout_matches_wide_reference_and_stays_inside_surface() {
        let layout = compute(LayoutInput {
            edge: Edge::Top,
            attached: false,
            mode: PrimarySurfaceMode::Expanded(ExpandedView::Music),
            page: Page::Music,
            hovered: false,
            timer_active: false,
            timer_finished: false,
            compact_length: 80.,
            collapsed_shoulder: 8.,
            expanded_shoulder: 32.,
            corner_radius: 32.,
            tool_ids: [false; 7],
            previous: true,
            play_pause: true,
            next: true,
            seek: true,
            favorite_state: Some(false),
        });
        assert!(layout.surface.w / layout.surface.h > 2.3);
        let album = layout.album.unwrap().rect;
        assert_eq!(album.w, album.h);
        assert!(album.w >= 60.);
        for rect in [
            layout.title.unwrap().rect,
            layout.artist.unwrap().rect,
            layout.progress.unwrap().rect,
            layout.controls[0].rect,
            layout.controls[1].rect,
            layout.controls[2].rect,
        ] {
            assert!(rect.x >= layout.surface.x);
            assert!(rect.y >= layout.surface.y);
            assert!(rect.x + rect.w <= layout.surface.x + layout.surface.w);
            assert!(rect.y + rect.h <= layout.surface.y + layout.surface.h);
        }
        assert!(layout.controls[0].rect.x < layout.controls[1].rect.x);
        assert!(layout.controls[1].rect.x < layout.controls[2].rect.x);
        let favorite = layout.favorite.unwrap().rect;
        assert!(!layout.favorite_state.unwrap());
        assert!(favorite.x >= layout.surface.x);
        assert!(favorite.x + favorite.w < layout.controls[1].rect.x);
        assert!(favorite.y + favorite.h <= layout.surface.y + layout.surface.h);
    }
}
