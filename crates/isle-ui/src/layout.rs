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
    pub shelf_button: Option<ElementLayout>,
    pub netease_button: Option<ElementLayout>,
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
    pub show_transport_fallback: bool,
    pub seek: bool,
    pub favorite_state: Option<bool>,
    pub has_media: bool,
    pub shuffle_enabled: bool,
    pub repeat_enabled: bool,
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
            Page::Shelf => (396., 350.),
            Page::NetEase => (396., 228.),
            Page::Music => (
                430.,
                if input.tool_ids.iter().any(|visible| *visible) {
                    202.
                } else {
                    164.
                },
            ),
            _ => (396., 228.),
        };
        let height = if input.page == Page::Music
            && input.attached
            && matches!(input.edge, Edge::Left | Edge::Right)
        {
            height + shoulder * 2.
        } else {
            height
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
            (length + 20.).min(300.)
        } else {
            length
        };
        let thickness = if input.hovered { 40. } else { 36. };
        let (w, h) = match input.edge {
            Edge::Left | Edge::Right => (thickness, length),
            Edge::Top | Edge::Bottom => (length, thickness),
        };
        (w, h, 18., input.collapsed_shoulder.clamp(0., 16.))
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
        let (body_x, body_y, body_w, body_h) =
            music_body(surface, input.edge, input.attached, shoulder, false);
        let album = Rect {
            x: body_x,
            y: body_y,
            w: 72.,
            h: 72.,
        };
        let title = Rect {
            x: body_x + 88.,
            y: body_y + 16.,
            w: (body_w - 122.).max(40.),
            h: 24.,
        };
        let artist = Rect {
            x: title.x,
            y: title.y + 17.,
            w: title.w,
            h: 20.,
        };
        let progress = Rect {
            x: body_x + 36.,
            y: body_y + 77.,
            w: (body_w - 78.).max(40.),
            h: 20.,
        };
        let visible_controls: Vec<Hit> = [
            input.has_media.then_some(Hit::Shuffle),
            input.has_media.then_some(Hit::Favorite),
            (input.previous || input.show_transport_fallback).then_some(Hit::Previous),
            (input.play_pause || input.show_transport_fallback).then_some(Hit::Play),
            (input.next || input.show_transport_fallback).then_some(Hit::Next),
            input.has_media.then_some(Hit::Mode),
            input.has_media.then_some(Hit::Output),
        ]
        .into_iter()
        .flatten()
        .collect();
        let control_center = |hit: Hit| {
            let x = match hit {
                Hit::Shuffle => 40.,
                Hit::Favorite => 73.,
                Hit::Previous => 147.,
                Hit::Play => 202.,
                Hit::Next => 257.,
                Hit::Mode => 332.,
                Hit::Output => 367.,
                _ => surface.w * 0.5,
            };
            surface.x + x * surface.w / 407.
        };
        let mut controls = Vec::new();
        let has_transport =
            input.previous || input.play_pause || input.next || input.show_transport_fallback;
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
        for hit in visible_controls {
            let button_size = match hit {
                Hit::Play => (48., 46.),
                Hit::Previous | Hit::Next => (40., 40.),
                _ => (34., 34.),
            };
            let rect = Rect {
                x: control_center(hit) - button_size.0 * 0.5,
                y: body_y + 100. + (46. - button_size.1) * 0.5,
                w: button_size.0,
                h: button_size.1,
            };
            if matches!(hit, Hit::Previous | Hit::Play | Hit::Next) && (!has_transport
                || !input.show_transport_fallback && matches!(hit, Hit::Previous) && !input.previous
                || !input.show_transport_fallback && matches!(hit, Hit::Play) && !input.play_pause
                || !input.show_transport_fallback && matches!(hit, Hit::Next) && !input.next) {
                continue;
            }
            controls.push(ControlLayout {
                hit,
                rect,
                enabled: match hit {
                    Hit::Shuffle => input.shuffle_enabled,
                    Hit::Previous => input.previous,
                    Hit::Play => input.play_pause,
                    Hit::Next => input.next,
                    Hit::Mode => input.repeat_enabled,
                    _ => true,
                },
            });
            snapshot.hit_regions.push(HitRegion { hit, rect });
        }
        if let Some(liked) = input.favorite_state.filter(|_| !input.has_media) {
            snapshot.favorite = Some(ElementLayout {
                rect: Rect {
                    x: body_x + 8.,
                    y: body_y + 107.,
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
            radius: 17.,
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
        snapshot.shelf_button = Some(ElementLayout {
            rect: Rect {
                x: surface.x + surface.w - 74.,
                y: surface.y + 6.,
                w: 30.,
                h: 30.,
            },
            radius: 12.,
            opacity: 1.,
        });
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
    let toolbar = if has_toolbar { 38. } else { 0. };
    (
        surface.x + 24. + side_inset,
        surface.y + top_inset + toolbar + 16.,
        (surface.w - 48. - side_inset * 2.).max(40.),
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
    fn music_content_and_time_labels_fit_every_edge_with_toolbar_and_shoulders() {
        for edge in [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left] {
            for attached in [false, true] {
                for toolbar in [false, true] {
                    let layout = compute(LayoutInput {
                        edge,
                        attached,
                        mode: PrimarySurfaceMode::Expanded(ExpandedView::Music),
                        page: Page::Music,
                        hovered: false,
                        timer_active: false,
                        timer_finished: false,
                        compact_length: 156.,
                        collapsed_shoulder: 8.,
                        expanded_shoulder: 64.,
                        corner_radius: 32.,
                        tool_ids: [toolbar; 7],
                        previous: true,
                        play_pause: true,
                        next: true,
                        seek: true,
                        favorite_state: Some(false),
                        has_media: false,
                        shuffle_enabled: false,
                        repeat_enabled: false,
                    });
                    for rect in layout.controls.iter().map(|control| control.rect).chain([
                        layout.album.unwrap().rect,
                        layout.title.unwrap().rect,
                        layout.artist.unwrap().rect,
                        layout.favorite.unwrap().rect,
                    ]) {
                        assert!(rect.x >= layout.surface.x && rect.y >= layout.surface.y);
                        assert!(rect.x + rect.w <= layout.surface.x + layout.surface.w);
                        assert!(rect.y + rect.h <= layout.surface.y + layout.surface.h);
                    }
                    let timeline = layout.progress.unwrap().rect;
                    assert!(timeline.x - 31. >= layout.surface.x);
                    assert!(timeline.x + timeline.w + 38. <= layout.surface.x + layout.surface.w);
                    let previous = layout
                        .controls
                        .iter()
                        .find(|control| control.hit == Hit::Previous)
                        .unwrap()
                        .rect
                        .center();
                    let play = layout
                        .controls
                        .iter()
                        .find(|control| control.hit == Hit::Play)
                        .unwrap()
                        .rect
                        .center();
                    let next = layout
                        .controls
                        .iter()
                        .find(|control| control.hit == Hit::Next)
                        .unwrap()
                        .rect
                        .center();
                    assert!((play.x - previous.x - (next.x - play.x)).abs() < 0.01);
                }
            }
        }
    }

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
            has_media: false,
            shuffle_enabled: false,
            repeat_enabled: false,
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
