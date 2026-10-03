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
        let height = match input.page {
            Page::Weather => 300.,
            Page::Music => 248.,
            _ => 228.,
        };
        (396., height, input.corner_radius.clamp(8., 80.), shoulder)
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
            y: body_y + 12.,
            w: 112.,
            h: 112.,
        };
        let title = Rect {
            x: body_x + 132.,
            y: body_y + 25.,
            w: (body_w - 142.).max(40.),
            h: 22.,
        };
        let artist = Rect {
            x: title.x,
            y: title.y + 26.,
            w: title.w,
            h: 18.,
        };
        let progress = Rect {
            x: title.x,
            y: body_y + 100.,
            w: title.w,
            h: 24.,
        };
        let visible_controls: Vec<Hit> = [
            input.previous.then_some(Hit::Previous),
            input.play_pause.then_some(Hit::Play),
            input.next.then_some(Hit::Next),
        ]
        .into_iter()
        .flatten()
        .collect();
        let control_width = visible_controls.len() as f32 * 42.;
        let controls_left = title.x + (title.w - control_width) * 0.5;
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
                (42., 38.)
            } else {
                (36., 36.)
            };
            let rect = Rect {
                x: controls_left + i as f32 * 42. + (42. - button_size.0) * 0.5,
                y: body_y + 148.,
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
