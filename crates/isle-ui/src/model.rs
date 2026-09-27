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
    Reset,
}
#[derive(Debug)]
pub struct PageInstance {
    pub page: Page,
    pub generation: u64,
}
pub struct Model {
    pub edge: Edge,
    pub attached: bool,
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
    pub now: f64,
    pub track: usize,
    pub title_started: f64,
    pub tool_count: usize,
}
impl Default for Model {
    fn default() -> Self {
        Self {
            edge: Edge::Top,
            attached: false,
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
            timer_left: 300.,
            now: 0.,
            track: 0,
            title_started: 0.,
            tool_count: 7,
        }
    }
}
impl Model {
    pub fn page(&self) -> Page {
        self.current.as_ref().map(|p| p.page).unwrap_or(Page::Music)
    }
    pub fn switch(&mut self, page: Page) {
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
            let bar = if self.tool_count > 0 { 40. } else { 0. };
            let h = match self.page() {
                Page::Music => 160.,
                Page::Weather => 240.,
                _ => 188.,
            } + bar;
            let inset = if self.attached { 32. } else { 0. };
            (
                if self.page() == Page::Music || vertical(self.edge) {
                    300.
                } else {
                    300. + inset * 2.
                },
                h + if vertical(self.edge) { inset * 2. } else { 0. },
                45.,
                32.,
            )
        } else {
            let len = if self.hovered { 90. } else { 80. };
            let thick = if self.hovered { 30. } else { 28. };
            if vertical(self.edge) {
                (thick, len, 14., 8.)
            } else {
                (len, thick, 14., 8.)
            }
        };
        self.width.set(w, self.reduced);
        self.height.set(h, self.reduced);
        self.radius.set(r, self.reduced);
        self.shoulder.set(s, self.reduced);
    }
    pub fn step(&mut self, dt: f32, now: f64) {
        self.now = now;
        self.width.advance(dt);
        self.height.advance(dt);
        self.radius.advance(dt);
        self.shoulder.advance(dt);
        if let Some(deadline) = self.timer_deadline {
            self.timer_left = (deadline - now).max(0.);
            if self.timer_left == 0. {
                self.timer_deadline = None;
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
            || (!self.reduced && self.playing && (!self.expanded || self.page() == Page::Music))
            || (!self.reduced && self.expanded && self.page() == Page::Music && self.track == 1)
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
        Rect {
            x: c.x + (c.w - 156.) / 2.,
            y: c.y + 6.,
            w: 156.,
            h: 28.,
        }
    }
    pub fn tool(&self, i: usize) -> Rect {
        let b = self.bar();
        Rect {
            x: b.x + i as f32 * 32. - self.scroll,
            y: b.y,
            w: 28.,
            h: 28.,
        }
    }
    pub fn body(&self) -> Rect {
        let mut c = self.content();
        let bar = if self.tool_count > 0 { 40. } else { 0. };
        c.y += bar + 12.;
        c.h -= bar + 12.;
        c
    }
    pub fn controls(&self) -> Vec<(Hit, Rect)> {
        if !self.expanded {
            return vec![];
        }
        let c = self.body();
        let mut v = vec![];
        for i in 0..self.tool_count {
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
            Page::Volume => v.push((
                Hit::Volume,
                Rect {
                    x: c.x,
                    y: c.y + 44.,
                    w: c.w,
                    h: 56.,
                },
            )),
            Page::Timer => {
                v.push((
                    Hit::Timer,
                    Rect {
                        x: c.x,
                        y: c.y + 102.,
                        w: c.w - 40.,
                        h: 32.,
                    },
                ));
                v.push((
                    Hit::Reset,
                    Rect {
                        x: c.x + c.w - 32.,
                        y: c.y + 102.,
                        w: 32.,
                        h: 32.,
                    },
                ));
            }
            _ => {}
        }
        v
    }
    pub fn hit(&self, p: Point) -> Option<Hit> {
        if !inside(p, &self.outline()) {
            return None;
        }
        Some(
            self.controls()
                .into_iter()
                .find(|(_, r)| r.contains(p))
                .map(|(h, _)| h)
                .unwrap_or(Hit::Blank),
        )
    }
    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll =
            (self.scroll + delta).clamp(0., self.tool_count.saturating_sub(5) as f32 * 32.);
    }
    pub fn activate(&mut self, hit: Hit) {
        match hit {
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
                    if self.timer_left <= 0. {
                        self.timer_left = 300.
                    }
                    self.timer_deadline = Some(self.now + self.timer_left);
                }
            }
            Hit::Reset => {
                self.timer_deadline = None;
                self.timer_left = 300.;
            }
            Hit::Volume => {}
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
        m.step(0., 301.);
        assert_eq!(m.page(), Page::Timer);
        assert_eq!(m.timer_left, 0.);
        assert!(m.timer_deadline.is_none());
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
