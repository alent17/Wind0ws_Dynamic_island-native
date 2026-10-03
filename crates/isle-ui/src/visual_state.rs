use crate::{
    layout::{ElementLayout, LayoutSnapshot},
    motion::{AnimatedRect, MotionPolicy, MotionProfile, VisualElement},
    spring::Spring,
};

#[derive(Clone, Copy, Debug)]
pub struct VisualState {
    pub album: VisualElement,
    pub title: VisualElement,
    pub artist: VisualElement,
    pub progress: VisualElement,
    /// Previous, play/pause, and next button bounds. The fixed slots keep the
    /// small Full Player control set bounded while allowing each button to be
    /// retargeted without throwing away its current position or velocity.
    pub control_rects: [AnimatedRect; 3],
    pub content_opacity: Spring,
    pub controls_opacity: Spring,
    initialized: bool,
}

impl Default for VisualState {
    fn default() -> Self {
        Self {
            album: VisualElement::new(Default::default(), 10., MotionProfile::Content),
            title: VisualElement::new(Default::default(), 0., MotionProfile::Content),
            artist: VisualElement::new(Default::default(), 0., MotionProfile::Content),
            progress: VisualElement::new(Default::default(), 2., MotionProfile::Content),
            control_rects: std::array::from_fn(|_| AnimatedRect::default()),
            content_opacity: Spring::new(0.),
            controls_opacity: Spring::new(0.),
            initialized: false,
        }
    }
}

impl VisualState {
    pub fn retarget(
        &mut self,
        layout: &LayoutSnapshot,
        policy: MotionPolicy,
        expanded: bool,
        control_targets: [Option<crate::geometry::Rect>; 3],
        collapsed_control_anchor: crate::geometry::Rect,
        controls_visible: bool,
    ) {
        let first = !self.initialized;
        if let Some(element) = layout.album {
            self.album.set_layout(element, policy, first);
        }
        if let Some(element) = layout.title {
            self.title.set_layout(element, policy, first);
        } else {
            self.title.opacity.set(0., first || policy.reduced);
        }
        if let Some(element) = layout.artist {
            self.artist.set_layout(element, policy, first);
        } else {
            self.artist.opacity.set(0., first || policy.reduced);
        }
        if let Some(element) = layout.progress {
            self.progress.set_layout(element, policy, first);
        } else {
            self.progress.opacity.set(0., first || policy.reduced);
        }
        self.content_opacity
            .set(if expanded { 1. } else { 0. }, first || policy.reduced);
        self.controls_opacity.set(
            if controls_visible { 1. } else { 0. },
            first || policy.reduced || control_targets.iter().all(Option::is_none),
        );
        for (animated, target) in self.control_rects.iter_mut().zip(control_targets) {
            if let Some(target) = target {
                animated.set(target, first || policy.reduced);
            } else {
                // Unsupported controls have no visual presence, so settle
                // their dormant slot without scheduling invisible frames.
                animated.set(collapsed_control_anchor, true);
            }
        }
        self.initialized = true;
    }

    pub fn advance(&mut self, dt: f32, policy: MotionPolicy) {
        self.album.advance(dt, policy);
        self.title.advance(dt, policy);
        self.artist.advance(dt, policy);
        self.progress.advance(dt, policy);
        for rect in &mut self.control_rects {
            rect.advance(dt, MotionProfile::Micro, policy);
        }
        let content_dt = policy.dt(dt, MotionProfile::Content);
        self.content_opacity.advance(content_dt);
        self.controls_opacity
            .advance(policy.dt(dt, MotionProfile::Micro));
    }

    pub fn active(&self) -> bool {
        self.album.active()
            || self.title.active()
            || self.artist.active()
            || self.progress.active()
            || self.control_rects.iter().any(AnimatedRect::active)
            || self.content_opacity.active()
            || self.controls_opacity.active()
    }
}

impl VisualElement {
    fn set_layout(&mut self, layout: ElementLayout, policy: MotionPolicy, first: bool) {
        self.set(
            layout.rect,
            layout.radius,
            layout.opacity,
            MotionPolicy {
                reduced: policy.reduced || first,
                ..policy
            },
        );
    }
}
