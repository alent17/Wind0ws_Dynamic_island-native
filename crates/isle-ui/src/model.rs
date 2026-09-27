use crate::{geometry::*, spring::Spring};
pub const HOST: f32 = 480.;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Music,
    Timer,
    Volume,
    Clock,
    Weather,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Blank,
    Tool(usize),
    Back,
    Play,
    Previous,
    Next,
    Volume,
    Timer,
    TimerRuler,
    Dismiss,
    Reset,
    Devices,
    DeviceMenu,
    Device(usize),
    DevicePrev,
    DeviceNext,
    Mute,
    WeatherSettings,
}
#[derive(Debug)]
pub struct PageInstance {
    pub page: Page,
    pub generation: u64,
}
pub struct Model {
    pub weather: isle_core::weather::View,
    pub time_zone: String,
    pub spectrum: Option<SpectrumVisual>,
    pub audio: Option<isle_core::AudioSnapshot>,
    pub device_menu: bool,
    pub device_scroll: f32,
    pub media: Option<isle_core::MediaSnapshot>,
    pub media_failed: bool,
    pub edge: Edge,
    pub attached: bool,
    pub compact_length: u16,
    pub collapsed_shoulder_radius: u8,
    pub expanded_shoulder_radius: u8,
    pub expanded_corner_radius: u32,
    pub background_color: [f32; 3],
    pub use_album_color: bool,
    pub expanded: bool,
    pub reduced: bool,
    pub hovered: bool,
    pub width: Spring,
    pub height: Spring,
    pub radius: Spring,
    pub shoulder: Spring,
    pub current: Option<PageInstance>,
    pub generation: u64,
    pub scroll: f32,
    pub focus: Option<Hit>,
    pub playing: bool,
    pub volume: f32,
    pub timer_deadline: Option<f64>,
    pub timer_left: f64,
    pub timer_duration: f64,
    pub timer_minutes: u16,
    pub timer_active: bool,
    pub timer_finished: bool,
    pub now: f64,
    pub track: usize,
    pub title_started: f64,
    pub title_overflow: bool,
    pub disc_angle: f32,
    pub tool_count: usize,
    pub tool_mask: [bool; 7],
}
impl Default for Model {
    fn default() -> Self {
        Self {
            weather: isle_core::weather::View::default(),
            time_zone: "system".into(),
            spectrum: None,
            audio: None,
            device_menu: false,
            device_scroll: 0.,
            media: None,
            media_failed: false,
            edge: Edge::Top,
            attached: false,
            compact_length: 80,
            collapsed_shoulder_radius: 8,
            expanded_shoulder_radius: 32,
            expanded_corner_radius: 45,
            background_color: [40. / 255., 50. / 255., 60. / 255.],
            use_album_color: true,
            expanded: false,
            reduced: false,
            hovered: false,
            width: Spring::new(80.),
            height: Spring::new(28.),
            radius: Spring::new(14.),
            shoulder: Spring::new(8.),
            current: None,
            generation: 0,
            scroll: 0.,
            focus: None,
            playing: true,
            volume: 42.,
            timer_deadline: None,
            timer_left: 1200.,
            timer_duration: 1200.,
            timer_minutes: 20,
            timer_active: false,
            timer_finished: false,
            now: 0.,
            track: 0,
            title_started: 0.,
            title_overflow: false,
            disc_angle: 0.,
            tool_count: 7,
            tool_mask: [true; 7],
        }
    }
}
impl Model {
    pub fn visible_tools(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.tool_count.min(7)).filter(|i| self.tool_mask[*i])
    }
    pub fn set_tool_mask(&mut self, mask: [bool; 7]) {
        self.tool_mask = mask;
        self.scroll_by(0.);
        if matches!(self.focus,Some(Hit::Tool(i)) if !self.visible_tools().any(|id|id==i)) {
            self.focus = None;
        }
        self.retarget();
    }
    pub fn progress_tick(&self) -> bool {
        self.expanded
            && self.page() == Page::Music
            && self.media.as_ref().is_some_and(|m| {
                m.playing && m.timeline.position_known && m.timeline.duration_ms > 0
            })
    }
    pub fn enabled(&self, hit: Hit) -> bool {
        if matches!(hit, Hit::Volume | Hit::Mute)
            && self.audio.as_ref().is_some_and(|a| a.device.id.is_empty())
        {
            return false;
        }
        self.media.as_ref().is_none_or(|m| match hit {
            Hit::Play => m.play_pause,
            Hit::Previous => m.previous,
            Hit::Next => m.next,
            _ => true,
        })
    }
    pub fn page(&self) -> Page {
        self.current.as_ref().map(|p| p.page).unwrap_or(Page::Music)
    }
    pub fn switch(&mut self, page: Page) {
        self.device_menu = false;
        self.device_scroll = 0.;
        self.title_started = self.now;
        self.expanded = true;
        self.generation += 1;
        self.current = Some(PageInstance {
            page,
            generation: self.generation,
        });
        self.focus = None;
        self.retarget();
    }
    pub fn toggle(&mut self) {
        if self.expanded {
            self.device_menu = false;
            self.expanded = false;
            self.current = None;
            self.scroll = 0.;
            self.focus = None;
            self.retarget();
        } else {
            self.switch(Page::Music)
        }
    }
    pub fn back(&mut self) {
        if self.device_menu {
            self.device_menu = false;
            self.focus = None;
            self.retarget();
            return;
        }
        if self.page() != Page::Music {
            self.switch(Page::Music)
        } else {
            self.toggle()
        }
    }
    pub fn change_track(&mut self) {
        self.track = 1 - self.track;
        self.title_started = self.now;
    }
    pub fn retarget(&mut self) {
        self.title_started = self.now;
        let (w, h, r, s) = if self.expanded {
            let bar = if self.visible_tools().next().is_some() {
                40.
            } else {
                0.
            };
            let h = match self.page() {
                Page::Music => 160.,
                Page::Weather => 240.,
                _ => 188.,
            } + bar;
            let inset = if self.attached {
                self.expanded_shoulder_radius.min(64) as f32
            } else {
                0.
            };
            (
                if self.page() == Page::Music || vertical(self.edge) {
                    300.
                } else {
                    300. + inset * 2.
                },
                h + if vertical(self.edge) { inset * 2. } else { 0. },
                self.expanded_corner_radius.min(80) as f32,
                self.expanded_shoulder_radius.min(64) as f32,
            )
        } else {
            let compact_length = self.compact_length.clamp(80, 300).max(
                if self.timer_active || self.timer_finished {
                    240
                } else {
                    80
                },
            ) as f32;
            let len = if self.hovered {
                (compact_length + 10.).min(300.)
            } else {
                compact_length
            };
            let thick = if self.hovered { 30. } else { 28. };
            if vertical(self.edge) {
                (
                    thick,
                    len,
                    14.,
                    self.collapsed_shoulder_radius.min(16) as f32,
                )
            } else {
                (
                    len,
                    thick,
                    14.,
                    self.collapsed_shoulder_radius.min(16) as f32,
                )
            }
        };
        self.width.set(w, self.reduced);
        self.height.set(h, self.reduced);
        self.radius.set(r, self.reduced);
        self.shoulder.set(s, self.reduced);
    }
    pub fn step(&mut self, dt: f32, now: f64) {
        self.now = now;
        if self.disc_spinning() {
            self.disc_angle =
                (self.disc_angle + dt * std::f32::consts::TAU / 8.) % std::f32::consts::TAU;
        }
        if let Some(spectrum) = &mut self.spectrum {
            spectrum.step(dt, self.reduced);
        }
        self.width.advance(dt);
        self.height.advance(dt);
        self.radius.advance(dt);
        self.shoulder.advance(dt);
        if let Some(deadline) = self.timer_deadline {
            self.timer_left = (deadline - now).max(0.);
            if self.timer_left == 0. {
                self.timer_deadline = None;
                self.timer_active = false;
                self.timer_finished = true;
                self.switch(Page::Timer);
            }
        }
    }
    pub fn moving(&self) -> bool {
        self.width.active()
            || self.height.active()
            || self.radius.active()
            || self.shoulder.active()
    }
    pub fn continuous(&self) -> bool {
        self.moving()
            || self.disc_spinning()
            || (self.media.is_none()
                && self.spectrum.is_none()
                && !self.reduced
                && self.playing
                && (!self.expanded || self.page() == Page::Music))
            || (!self.reduced && self.expanded && self.page() == Page::Music && self.title_overflow)
            || (!self.reduced
                && (!self.expanded || self.page() == Page::Music)
                && self.spectrum.as_ref().is_some_and(SpectrumVisual::moving))
    }
    pub fn cover_visible(&self) -> bool {
        if self.expanded {
            self.page() == Page::Music
        } else {
            !self.timer_active && !self.timer_finished
        }
    }
    pub fn disc_spinning(&self) -> bool {
        !self.expanded
            && !self.hovered
            && !self.reduced
            && self.playing
            && self.cover_visible()
            && self.media.as_ref().is_some_and(|m| m.cover.is_some())
    }
    pub fn compact_cover(&self) -> Rect {
        let o = self.origin();
        let inset = if self.attached {
            self.shoulder.value.max(0.)
        } else {
            0.
        };
        if vertical(self.edge) {
            Rect {
                x: o.x + (self.width.value - 20.) / 2.,
                y: o.y + inset + 4.,
                w: 20.,
                h: 20.,
            }
        } else {
            Rect {
                x: o.x + inset + 4.,
                y: o.y + (self.height.value - 20.) / 2.,
                w: 20.,
                h: 20.,
            }
        }
    }
    pub fn origin(&self) -> Point {
        offset(
            self.width.value,
            self.height.value,
            HOST,
            self.edge,
            self.attached,
        )
    }
    pub fn outline(&self) -> Vec<Point> {
        let o = self.origin();
        polygon(
            self.width.value,
            self.height.value,
            self.radius.value,
            self.shoulder.value,
            self.edge,
            self.attached,
        )
        .into_iter()
        .map(|p| Point {
            x: p.x + o.x,
            y: p.y + o.y,
        })
        .collect()
    }
    pub fn content(&self) -> Rect {
        let o = self.origin();
        let inset = if self.attached && !vertical(self.edge) {
            self.shoulder.value
        } else {
            0.
        };
        let top = if self.attached && vertical(self.edge) {
            self.shoulder.value
        } else {
            0.
        };
        Rect {
            x: o.x + 28. + inset,
            y: o.y + top,
            w: (self.width.value - 56. - inset * 2.).max(40.),
            h: self.height.value - top,
        }
    }
    pub fn bar(&self) -> Rect {
        let c = self.content();
        let width = (self.visible_tools().count().min(5) as f32 * 32. - 4.).max(0.);
        Rect {
            x: c.x + (c.w - width) / 2. + 1.,
            y: c.y + 7.,
            w: width,
            h: 28.,
        }
    }
    pub fn tool(&self, i: usize) -> Rect {
        let b = self.bar();
        Rect {
            x: b.x + self.visible_tools().position(|id| id == i).unwrap_or(0) as f32 * 32.
                - self.scroll,
            y: b.y,
            w: 28.,
            h: 28.,
        }
    }
    pub fn body(&self) -> Rect {
        let mut c = self.content();
        let bar = if self.visible_tools().next().is_some() {
            40.
        } else {
            0.
        };
        if self.page() == Page::Music {
            c.y += bar + 12.;
            c.h -= bar + 12.;
        } else {
            c.x -= 3.;
            c.w += 8.;
            c.y += bar + 17.;
            c.h -= bar
                + 40.
                + if self.attached && vertical(self.edge) {
                    self.shoulder.value
                } else {
                    0.
                };
        }
        c
    }
    pub fn detail_content(&self) -> Rect {
        let c = self.body();
        Rect {
            x: c.x,
            y: c.y + 40.,
            w: c.w,
            h: (c.h - 40.).max(0.),
        }
    }
    pub fn ruler(&self) -> Rect {
        let c = self.detail_content();
        Rect {
            x: c.x - 6.,
            y: c.y,
            w: c.w + 12.,
            h: 44.,
        }
    }
    pub fn device_trigger(&self) -> Rect {
        let c = self.detail_content();
        Rect {
            x: c.x,
            y: c.y + 54.,
            w: (c.w - 66.).max(40.),
            h: 26.,
        }
    }
    pub fn device_popup(&self) -> Rect {
        let trigger = self.device_trigger();
        let count = self.audio.as_ref().map_or(0, |a| a.devices.len());
        let h = (count as f32 * 28. + 10.).min(78.);
        Rect {
            x: trigger.x,
            y: trigger.y - 4. - h,
            w: trigger.w,
            h,
        }
    }
    pub fn set_timer_minutes(&mut self, value: f32) {
        if !self.timer_active && !self.timer_finished {
            self.timer_minutes = value.round().clamp(1., 1440.) as u16;
            self.timer_left = f64::from(self.timer_minutes) * 60.;
        }
    }
    pub fn device_viewport(&self) -> Rect {
        let p = self.device_popup();
        Rect {
            x: p.x + 5.,
            y: p.y + 5.,
            w: p.w - 10.,
            h: (p.h - 10.).max(0.),
        }
    }
    pub fn device_row(&self, index: usize) -> Rect {
        let p = self.device_viewport();
        Rect {
            x: p.x,
            y: p.y + index as f32 * 28. - self.device_scroll,
            w: p.w,
            h: 28.,
        }
    }
    pub fn scroll_devices(&mut self, delta: f32) {
        let content = self.audio.as_ref().map_or(0, |a| a.devices.len()) as f32 * 28.;
        self.device_scroll =
            (self.device_scroll + delta).clamp(0., (content - self.device_viewport().h).max(0.));
        self.focus = None;
    }
    pub fn controls(&self) -> Vec<(Hit, Rect)> {
        if !self.expanded {
            return vec![];
        }
        let c = self.body();
        let mut v = vec![];
        for i in self.visible_tools() {
            let r = self.tool(i);
            if self.bar().contains(Point {
                x: r.x + 14.,
                y: r.y + 14.,
            }) {
                v.push((Hit::Tool(i), r));
            }
        }
        if self.page() != Page::Music {
            v.push((
                Hit::Back,
                Rect {
                    x: c.x,
                    y: c.y,
                    w: 28.,
                    h: 28.,
                },
            ));
        }
        let p = self.detail_content();
        match self.page() {
            Page::Music => {
                for (i, hit) in [Hit::Previous, Hit::Play, Hit::Next]
                    .into_iter()
                    .enumerate()
                {
                    v.push((
                        hit,
                        Rect {
                            x: c.x + c.w / 2. - 80. + i as f32 * 56.,
                            y: c.y + 82.,
                            w: 48.,
                            h: 48.,
                        },
                    ));
                }
            }
            Page::Volume => {
                if self.device_menu {
                    if let Some(audio) = &self.audio {
                        let viewport = self.device_viewport();
                        for index in 0..audio.devices.len() {
                            let row = self.device_row(index);
                            let top = row.y.max(viewport.y);
                            let bottom = (row.y + row.h).min(viewport.y + viewport.h);
                            if bottom > top {
                                v.push((
                                    Hit::Device(index),
                                    Rect {
                                        y: top,
                                        h: bottom - top,
                                        ..row
                                    },
                                ));
                            }
                        }
                    }
                }
                v.push((Hit::Volume, self.ruler()));
                if self.audio.is_some() {
                    v.push((Hit::Devices, self.device_trigger()));
                }
            }
            Page::Timer => {
                if self.timer_finished {
                    v.push((
                        Hit::Dismiss,
                        Rect {
                            x: p.x + p.w - 38.,
                            y: p.y + (p.h - 27.) / 2.,
                            w: 27.,
                            h: 27.,
                        },
                    ));
                    return v;
                }
                if !self.timer_active {
                    v.push((Hit::TimerRuler, self.ruler()));
                }
                let y = p.y
                    + if self.timer_active {
                        (p.h - 48.) / 2.
                    } else {
                        53.8
                    };
                v.push((
                    Hit::Timer,
                    Rect {
                        x: p.x,
                        y,
                        w: if self.timer_active { 48. } else { 70.5 },
                        h: if self.timer_active { 48. } else { 28. },
                    },
                ));
                if self.timer_active {
                    v.push((
                        Hit::Reset,
                        Rect {
                            x: p.x + 60.,
                            y,
                            w: 48.,
                            h: 48.,
                        },
                    ));
                }
            }
            Page::Weather if self.weather.city.is_none() => v.push((
                Hit::WeatherSettings,
                Rect {
                    x: p.x,
                    y: p.y + p.h / 2. - 4.,
                    w: 76.,
                    h: 40.,
                },
            )),
            _ => {}
        }
        v
    }
    pub fn hit(&self, p: Point) -> Option<Hit> {
        if !inside(p, &self.outline()) {
            return None;
        }
        if self.device_menu && self.device_popup().contains(p) {
            return Some(
                self.controls()
                    .into_iter()
                    .find(|(hit, r)| matches!(hit, Hit::Device(_)) && r.contains(p))
                    .map(|(hit, _)| hit)
                    .unwrap_or(Hit::DeviceMenu),
            );
        }
        Some(
            self.controls()
                .into_iter()
                .find(|(hit, r)| {
                    r.contains(p) && (!matches!(hit, Hit::Tool(_)) || self.bar().contains(p))
                })
                .map(|(h, _)| h)
                .unwrap_or(Hit::Blank),
        )
    }
    pub fn move_focus(&mut self, backwards: bool, tools_only: bool) {
        if !self.expanded {
            return;
        }
        let mut targets: Vec<Hit> = self.visible_tools().map(Hit::Tool).collect();
        if !tools_only {
            targets.extend(
                self.controls()
                    .into_iter()
                    .map(|(h, _)| h)
                    .filter(|h| !matches!(h, Hit::Tool(_)) && self.enabled(*h)),
            );
        }
        if targets.is_empty() {
            self.focus = None;
            return;
        }
        let index = self
            .focus
            .and_then(|focus| targets.iter().position(|h| *h == focus));
        let next = match index {
            Some(i) if backwards => (i + targets.len() - 1) % targets.len(),
            Some(i) => (i + 1) % targets.len(),
            None if backwards => targets.len() - 1,
            None => 0,
        };
        self.focus = Some(targets[next]);
        if let Hit::Tool(i) = targets[next] {
            let x = self.visible_tools().position(|id| id == i).unwrap_or(0) as f32 * 32.;
            self.scroll = self.scroll.clamp((x + 28. - 156.).max(0.), x);
        }
    }
    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll = (self.scroll + delta).clamp(
            0.,
            self.visible_tools().count().saturating_sub(5) as f32 * 32.,
        );
    }
    pub fn focus_tool_boundary(&mut self, last: bool) {
        let tool = if last {
            self.visible_tools().last()
        } else {
            self.visible_tools().next()
        };
        self.focus = tool.map(Hit::Tool);
        self.scroll_by(if last { f32::MAX } else { -f32::MAX });
    }
    pub fn activate(&mut self, hit: Hit) {
        match hit {
            Hit::Blank if !self.expanded && (self.timer_active || self.timer_finished) => {
                self.switch(Page::Timer)
            }
            Hit::Blank => self.toggle(),
            Hit::Back => self.back(),
            Hit::Play => self.playing = !self.playing,
            Hit::Previous | Hit::Next => self.change_track(),
            Hit::Tool(i) => match i {
                0 => self.switch(Page::Timer),
                1 => self.switch(Page::Volume),
                5 => self.switch(Page::Clock),
                6 => self.switch(Page::Weather),
                _ => {}
            },
            Hit::Timer => {
                if self.timer_deadline.is_some() {
                    self.timer_deadline = None;
                } else {
                    if !self.timer_active {
                        self.timer_left = f64::from(self.timer_minutes) * 60.;
                        self.timer_duration = self.timer_left;
                    }
                    self.timer_active = true;
                    self.timer_finished = false;
                    self.timer_deadline = Some(self.now + self.timer_left);
                }
            }
            Hit::Reset => {
                self.timer_deadline = None;
                self.timer_active = false;
                self.timer_finished = false;
                self.timer_left = f64::from(self.timer_minutes) * 60.;
            }
            Hit::Dismiss => {
                self.timer_finished = false;
                self.switch(Page::Music);
            }
            Hit::Devices => {
                self.device_menu = !self.device_menu;
                self.device_scroll = 0.;
                self.focus = None;
                self.retarget();
            }
            Hit::DevicePrev => {
                self.scroll_devices(-28.);
            }
            Hit::DeviceNext => {
                self.scroll_devices(28.);
            }
            Hit::Device(_)
            | Hit::DeviceMenu
            | Hit::Mute
            | Hit::Volume
            | Hit::TimerRuler
            | Hit::WeatherSettings => {}
        }
    }
}
#[derive(Default)]
pub struct SpectrumVisual {
    pub values: [f32; 6],
    pub target: [f32; 6],
    pub failed: bool,
}
impl SpectrumVisual {
    pub fn moving(&self) -> bool {
        self.values
            .iter()
            .zip(self.target)
            .any(|(v, t)| (v - t).abs() > 0.002)
    }
    pub fn step(&mut self, dt: f32, reduced: bool) {
        for (value, target) in self.values.iter_mut().zip(self.target) {
            if reduced || (*value - target).abs() <= 0.002 {
                *value = target;
            } else {
                *value += (target - *value) * (1. - (-dt.clamp(0., 0.1) / 0.055).exp());
            }
        }
    }
}
pub fn marquee(distance: f32, seconds: f64) -> f32 {
    if distance <= 0. {
        return 0.;
    }
    let travel = distance as f64 / 24.;
    let t = seconds % (travel * 2. + 2.4);
    if t < 1.2 {
        0.
    } else if t < 1.2 + travel {
        ((t - 1.2) * 24.) as f32
    } else if t < 2.4 + travel {
        distance
    } else {
        distance - ((t - travel - 2.4) * 24.) as f32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_cover_is_visible_and_rotation_obeys_lifecycle() {
        let mut m = Model {
            media: Some(isle_core::MediaSnapshot {
                cover: Some(std::sync::Arc::new(isle_core::Cover {
                    width: 1,
                    height: 1,
                    pixels: vec![255; 4],
                })),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(m.cover_visible() && m.disc_spinning());
        m.step(1., 1.);
        let angle = m.disc_angle;
        assert!((angle - std::f32::consts::FRAC_PI_4).abs() < 0.001);
        m.playing = false;
        m.step(1., 2.);
        assert_eq!(m.disc_angle, angle);
        m.playing = true;
        m.reduced = true;
        assert!(!m.disc_spinning());
        m.switch(Page::Volume);
        assert!(!m.cover_visible());
        m.toggle();
        assert!(m.cover_visible());
        m.timer_active = true;
        assert!(!m.cover_visible());
    }
    #[test]
    fn compact_cover_fits_every_edge_and_floating_mode() {
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            for attached in [false, true] {
                let mut m = Model {
                    edge,
                    attached,
                    reduced: true,
                    ..Default::default()
                };
                m.retarget();
                let r = m.compact_cover();
                assert_eq!((r.w, r.h), (20., 20.));
                assert!(inside(
                    Point {
                        x: r.x + 10.,
                        y: r.y + 10.
                    },
                    &m.outline()
                ));
                assert!(inside(
                    Point {
                        x: r.x + 1.,
                        y: r.y + 10.
                    },
                    &m.outline()
                ));
            }
        }
    }
    #[test]
    fn appearance_geometry_settings_retarget_compact_and_expanded_shapes() {
        let mut m = Model {
            reduced: true,
            attached: true,
            compact_length: 140,
            collapsed_shoulder_radius: 12,
            expanded_shoulder_radius: 48,
            expanded_corner_radius: 64,
            ..Default::default()
        };
        m.retarget();
        assert_eq!(m.width.target, 140.);
        assert_eq!(m.height.target, 28.);
        assert_eq!(m.shoulder.target, 12.);
        m.switch(Page::Music);
        assert_eq!(m.width.target, 300.);
        assert_eq!(m.height.target, 200.);
        assert_eq!(m.radius.target, 64.);
        assert_eq!(m.shoulder.target, 48.);
    }
    #[test]
    fn sparse_tools_keep_identity_layout_and_navigation_without_empty_slots() {
        let mut m = Model {
            reduced: true,
            ..Default::default()
        };
        m.switch(Page::Music);
        m.set_tool_mask([true, false, false, false, false, true, true]);
        assert_eq!(m.visible_tools().collect::<Vec<_>>(), vec![0, 5, 6]);
        assert_eq!(m.tool(5).x - m.tool(0).x, 32.);
        assert_eq!(m.bar().w, 92.);
        for id in [0, 5, 6, 0] {
            m.move_focus(false, true);
            assert_eq!(m.focus, Some(Hit::Tool(id)));
        }
        m.activate(Hit::Tool(5));
        assert_eq!(m.page(), Page::Clock);
        m.scroll = 300.;
        m.set_tool_mask([false; 7]);
        assert_eq!(m.scroll, 0.);
        assert_eq!(m.focus, None);
        assert_eq!(m.bar().w, 0.);
        assert!(m
            .controls()
            .iter()
            .all(|(hit, _)| !matches!(hit, Hit::Tool(_))));
        m.set_tool_mask([false, false, false, true, false, false, false]);
        assert_eq!(m.visible_tools().collect::<Vec<_>>(), vec![3]);
        assert_eq!(m.bar().w, 28.);
    }
    #[test]
    fn live_spectrum_settles_and_hidden_pages_do_not_request_animation_frames() {
        let mut m = Model {
            playing: false,
            spectrum: Some(SpectrumVisual::default()),
            ..Default::default()
        };
        m.spectrum.as_mut().unwrap().target = [0.8; 6];
        assert!(m.continuous());
        for i in 0..120 {
            m.step(1. / 60., i as f64 / 60.);
        }
        assert!(!m.continuous());
        m.spectrum.as_mut().unwrap().target = [0.; 6];
        m.reduced = true;
        m.step(0., 3.);
        assert_eq!(m.spectrum.as_ref().unwrap().values, [0.; 6]);
        m.switch(Page::Volume);
        m.reduced = false;
        m.spectrum.as_mut().unwrap().target = [1.; 6];
        assert!(!m.continuous());
    }
    #[test]
    fn live_progress_only_ticks_when_it_can_change_visible_pixels() {
        let mut m = Model {
            reduced: true,
            media: Some(isle_core::MediaSnapshot::default()),
            ..Model::default()
        };
        m.switch(Page::Music);
        m.media.as_mut().unwrap().playing = true;
        assert!(!m.progress_tick());
        m.media.as_mut().unwrap().timeline.duration_ms = 10000;
        m.media.as_mut().unwrap().timeline.position_known = true;
        assert!(m.progress_tick());
        m.switch(Page::Volume);
        assert!(!m.progress_tick());
        m.toggle();
        assert!(!m.progress_tick());
    }
    #[test]
    fn audio_menu_pages_without_overflow_and_returns_one_level() {
        let mut m = Model {
            audio: Some(isle_core::AudioSnapshot {
                devices: (0..9)
                    .map(|i| isle_core::AudioDevice {
                        id: i.to_string(),
                        name: format!("Device {i}"),
                    })
                    .collect(),
                ..Default::default()
            }),
            reduced: true,
            ..Default::default()
        };
        m.switch(Page::Volume);
        assert!(!m.enabled(Hit::Volume));
        let height = m.height.target;
        m.activate(Hit::Devices);
        assert_eq!(m.height.target, height);
        assert_eq!(
            m.controls()
                .iter()
                .filter(|(h, _)| matches!(h, Hit::Device(_)))
                .count(),
            3
        );
        for _ in 0..12 {
            m.activate(Hit::DeviceNext);
        }
        assert_eq!(
            m.controls()
                .iter()
                .filter(|(h, _)| matches!(h, Hit::Device(_)))
                .count(),
            3
        );
        m.activate(Hit::DeviceNext);
        assert_eq!(m.device_scroll, 184.);
        m.back();
        assert!(!m.device_menu && m.page() == Page::Volume && m.expanded);
        m.back();
        assert_eq!(m.page(), Page::Music);
        m.switch(Page::Volume);
        m.activate(Hit::Devices);
        m.toggle();
        assert!(!m.device_menu && m.current.is_none());
    }
    #[test]
    fn keyboard_reaches_every_tool_and_page_control_then_wraps() {
        let mut m = Model {
            reduced: true,
            ..Model::default()
        };
        m.switch(Page::Volume);
        for i in 0..7 {
            m.move_focus(false, false);
            assert_eq!(m.focus, Some(Hit::Tool(i)));
            let r = m.tool(i);
            assert!(r.x >= m.bar().x && r.x + r.w <= m.bar().x + m.bar().w);
        }
        m.move_focus(false, false);
        assert_eq!(m.focus, Some(Hit::Back));
        m.move_focus(false, false);
        assert_eq!(m.focus, Some(Hit::Volume));
        m.move_focus(false, false);
        assert_eq!(m.focus, Some(Hit::Tool(0)));
        m.move_focus(true, false);
        assert_eq!(m.focus, Some(Hit::Volume));
    }
    #[test]
    fn focus_works_without_tools_and_hidden_toolbar_cannot_receive_clicks() {
        let mut m = Model {
            reduced: true,
            tool_count: 0,
            ..Model::default()
        };
        m.switch(Page::Music);
        m.move_focus(false, false);
        assert_eq!(m.focus, Some(Hit::Previous));
        m.tool_count = 7;
        m.scroll = 14.;
        let r = m.tool(0);
        assert_eq!(
            m.hit(Point {
                x: r.x + 1.,
                y: r.y + 14.
            }),
            Some(Hit::Blank)
        );
        m.toggle();
        m.move_focus(false, false);
        assert_eq!(m.focus, None);
    }
    #[test]
    fn title_restarts_after_track_page_and_shape_changes() {
        let mut m = Model {
            now: 90.,
            ..Model::default()
        };
        m.change_track();
        assert_eq!(marquee(100., m.now - m.title_started), 0.);
        m.now = 100.;
        m.switch(Page::Music);
        assert_eq!(m.title_started, 100.);
        m.now = 110.;
        m.retarget();
        assert_eq!(m.title_started, 110.);
    }
    #[test]
    fn title_frames_follow_measured_overflow_visibility_and_motion_preference() {
        let mut m = Model {
            reduced: true,
            playing: false,
            ..Model::default()
        };
        m.switch(Page::Music);
        m.reduced = false;
        assert!(!m.continuous());
        m.title_overflow = true;
        assert!(m.continuous());
        m.reduced = true;
        assert!(!m.continuous());
        m.switch(Page::Volume);
        m.reduced = false;
        assert!(!m.continuous());
    }
    #[test]
    fn collapsed_pages_are_released() {
        let mut m = Model {
            reduced: true,
            ..Model::default()
        };
        for _ in 0..100 {
            m.switch(Page::Volume);
            m.switch(Page::Timer);
            m.toggle();
            assert!(m.current.is_none());
            m.toggle();
            assert_eq!(m.page(), Page::Music);
            m.toggle();
        }
    }
    #[test]
    fn countdown_survives_page_release() {
        let mut m = Model::default();
        m.switch(Page::Timer);
        m.activate(Hit::Timer);
        m.toggle();
        m.step(0., 1201.);
        assert_eq!(m.page(), Page::Timer);
        assert_eq!(m.timer_left, 0.);
        assert!(m.timer_deadline.is_none());
        assert!(m.timer_finished);
        m.activate(Hit::Dismiss);
        assert_eq!(m.page(), Page::Music);
        assert!(!m.timer_finished);
    }
    #[test]
    fn timer_ruler_bounds_pause_resume_and_reset_survive_page_switches() {
        let mut m = Model {
            reduced: true,
            ..Model::default()
        };
        m.switch(Page::Timer);
        m.set_timer_minutes(2000.);
        assert_eq!(m.timer_minutes, 1440);
        m.set_timer_minutes(-10.);
        assert_eq!(m.timer_minutes, 1);
        m.activate(Hit::Timer);
        m.step(0., 20.);
        m.activate(Hit::Timer);
        m.set_timer_minutes(60.);
        assert_eq!(m.timer_left, 40.);
        m.switch(Page::Music);
        m.step(0., 30.);
        m.switch(Page::Timer);
        assert!(!m.controls().iter().any(|(hit, _)| *hit == Hit::TimerRuler));
        m.activate(Hit::Timer);
        assert_eq!(m.timer_deadline, Some(70.));
        m.activate(Hit::Reset);
        assert_eq!(m.timer_left, 60.);
        assert!(!m.timer_active);
        assert!(m.controls().iter().any(|(hit, _)| *hit == Hit::TimerRuler));
    }
    #[test]
    fn scrolling_is_bounded() {
        let mut m = Model::default();
        m.scroll_by(1000.);
        assert_eq!(m.scroll, 64.);
        m.scroll_by(-1000.);
        assert_eq!(m.scroll, 0.);
    }
    #[test]
    fn title_pauses_and_returns() {
        assert_eq!(marquee(48., 1.), 0.);
        assert_eq!(marquee(48., 3.5), 48.);
        assert!((marquee(48., 6.4)).abs() < 0.001);
    }
}
