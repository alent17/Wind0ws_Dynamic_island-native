#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    Top,
    Right,
    Bottom,
    Left,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn center(self) -> Point {
        Point {
            x: self.x + self.w * 0.5,
            y: self.y + self.h * 0.5,
        }
    }
}
impl Rect {
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.w && p.y < self.y + self.h
    }
}
pub fn vertical(edge: Edge) -> bool {
    matches!(edge, Edge::Left | Edge::Right)
}
/// Fit the render host to the physical work area. The same effective scale is
/// used for pixels, text, hit testing and accessibility bounds.
pub fn window_placement(work: Rect, dpi: u32, host: f32, edge: Edge) -> (Rect, f32) {
    let side = (host * dpi.max(1) as f32 / 96.)
        .round()
        .min(work.w.floor())
        .min(work.h.floor())
        .max(1.);
    let (x, y) = match edge {
        Edge::Top => (work.x + ((work.w - side) / 2.).floor(), work.y),
        Edge::Bottom => (
            work.x + ((work.w - side) / 2.).floor(),
            work.y + work.h - side,
        ),
        Edge::Left => (work.x, work.y + ((work.h - side) / 2.).floor()),
        Edge::Right => (
            work.x + work.w - side,
            work.y + ((work.h - side) / 2.).floor(),
        ),
    };
    (
        Rect {
            x,
            y,
            w: side,
            h: side,
        },
        side / host,
    )
}
pub fn offset(w: f32, h: f32, host: f32, edge: Edge, attached: bool) -> Point {
    let gap = if attached { 0. } else { 22. };
    match edge {
        Edge::Top => Point {
            x: (host - w) / 2.,
            y: gap,
        },
        Edge::Bottom => Point {
            x: (host - w) / 2.,
            y: host - gap - h,
        },
        Edge::Left => Point {
            x: gap,
            y: (host - h) / 2.,
        },
        Edge::Right => Point {
            x: host - gap - w,
            y: (host - h) / 2.,
        },
    }
}
fn curve(out: &mut Vec<Point>, a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) {
    for i in 0..9 {
        let t = i as f32 / 8.;
        let q = 1. - t;
        out.push(Point {
            x: q * q * q * a.0 + 3. * q * q * t * b.0 + 3. * q * t * t * c.0 + t * t * t * d.0,
            y: q * q * q * a.1 + 3. * q * q * t * b.1 + 3. * q * t * t * c.1 + t * t * t * d.1,
        });
    }
}
pub fn polygon(
    width: f32,
    height: f32,
    radius: f32,
    shoulder: f32,
    edge: Edge,
    attached: bool,
) -> Vec<Point> {
    let (w, h) = if vertical(edge) {
        (height, width)
    } else {
        (width, height)
    };
    let r = radius.clamp(0., w.min(h) / 2.);
    let mut p = Vec::with_capacity(72);
    if attached {
        let s = shoulder.clamp(0., 64.);
        let inset = s.min(w / 4.).min(h / 2.);
        let depth = s.min(h / 2.).min(h - r);
        let c = inset.min(depth);
        curve(
            &mut p,
            (0., 0.),
            (inset - c * 0.45, 0.),
            (inset, depth - c * 0.45),
            (inset, depth),
        );
        curve(
            &mut p,
            (inset, depth),
            (inset, h * 0.35),
            (inset, h * 0.65),
            (inset, h - r),
        );
        curve(
            &mut p,
            (inset, h - r),
            (inset, h - r * 0.45),
            (inset + r * 0.45, h),
            (inset + r, h),
        );
        curve(
            &mut p,
            (inset + r, h),
            (w * 0.35, h),
            (w * 0.65, h),
            (w - inset - r, h),
        );
        curve(
            &mut p,
            (w - inset - r, h),
            (w - inset - r * 0.45, h),
            (w - inset, h - r * 0.45),
            (w - inset, h - r),
        );
        curve(
            &mut p,
            (w - inset, h - r),
            (w - inset, h * 0.65),
            (w - inset, h * 0.35),
            (w - inset, depth),
        );
        curve(
            &mut p,
            (w - inset, depth),
            (w - inset, depth - c * 0.45),
            (w - inset + c * 0.45, 0.),
            (w, 0.),
        );
        curve(&mut p, (w, 0.), (w * 0.65, 0.), (w * 0.35, 0.), (0., 0.));
    } else {
        curve(&mut p, (r, 0.), (r * 0.45, 0.), (0., r * 0.45), (0., r));
        curve(&mut p, (0., r), (0., h * 0.35), (0., h * 0.65), (0., h - r));
        curve(
            &mut p,
            (0., h - r),
            (0., h - r * 0.45),
            (r * 0.45, h),
            (r, h),
        );
        curve(&mut p, (r, h), (w * 0.35, h), (w * 0.65, h), (w - r, h));
        curve(
            &mut p,
            (w - r, h),
            (w - r * 0.45, h),
            (w, h - r * 0.45),
            (w, h - r),
        );
        curve(&mut p, (w, h - r), (w, h * 0.65), (w, h * 0.35), (w, r));
        curve(
            &mut p,
            (w, r),
            (w, r * 0.45),
            (w - r * 0.45, 0.),
            (w - r, 0.),
        );
        curve(&mut p, (w - r, 0.), (w * 0.65, 0.), (w * 0.35, 0.), (r, 0.));
    }
    p.into_iter()
        .map(|p| match edge {
            Edge::Top => p,
            Edge::Bottom => Point {
                x: width - p.x,
                y: height - p.y,
            },
            Edge::Left => Point { x: p.y, y: p.x },
            Edge::Right => Point {
                x: width - p.y,
                y: height - p.x,
            },
        })
        .collect()
}
pub fn inside(p: Point, poly: &[Point]) -> bool {
    let mut yes = false;
    let mut j = poly.len().saturating_sub(1);
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[j];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            yes = !yes;
        }
        j = i;
    }
    yes
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_fits_small_work_areas_at_all_dpis_and_keeps_its_edge() {
        for dpi in [96, 120, 144, 192] {
            for (w, h) in [(1920., 1080.), (600., 400.), (240., 180.)] {
                let work = Rect {
                    x: -1920.,
                    y: 100.,
                    w,
                    h,
                };
                for edge in [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left] {
                    let (r, scale) = window_placement(work, dpi, 480., edge);
                    assert!(r.x >= work.x && r.y >= work.y);
                    assert!(r.x + r.w <= work.x + work.w && r.y + r.h <= work.y + work.h);
                    assert!((r.w / 480. - scale).abs() < 0.0001);
                    match edge {
                        Edge::Top => assert_eq!(r.y, work.y),
                        Edge::Right => assert_eq!(r.x + r.w, work.x + work.w),
                        Edge::Bottom => assert_eq!(r.y + r.h, work.y + work.h),
                        Edge::Left => assert_eq!(r.x, work.x),
                    }
                }
            }
        }
    }
    #[test]
    fn all_edges_have_bounded_geometry() {
        for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            for attached in [true, false] {
                let p = polygon(364., 280., 45., 32., e, attached);
                assert_eq!(p.len(), 72);
                assert!(p
                    .iter()
                    .all(|p| p.x >= 0. && p.x <= 364. && p.y >= 0. && p.y <= 280.));
                assert!(inside(Point { x: 182., y: 140. }, &p));
            }
        }
    }
    #[test]
    fn shoulder_cutout_is_transparent() {
        let p = polygon(364., 200., 45., 32., Edge::Top, true);
        assert!(!inside(Point { x: 2., y: 25. }, &p));
        assert!(inside(Point { x: 70., y: 25. }, &p));
    }
    #[test]
    fn anchors_remain_fixed() {
        for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            let a = offset(80., 28., 480., e, false);
            let b = offset(300., 200., 480., e, false);
            match e {
                Edge::Top => assert_eq!(a.y, b.y),
                Edge::Bottom => assert_eq!(a.y + 28., b.y + 200.),
                Edge::Left => assert_eq!(a.x, b.x),
                Edge::Right => assert_eq!(a.x + 80., b.x + 300.),
            }
        }
    }
}
