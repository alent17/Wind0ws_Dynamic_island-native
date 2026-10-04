use crate::{
    geometry::Rect,
    layout::{ElementLayout, LayoutSnapshot},
    motion::{AnimatedRect, MotionPolicy, MotionProfile, VisualElement},
    spring::Spring,
};

#[derive(Clone, Debug)]
pub struct AnimatedActivitySlot {
    pub activity: Option<crate::state::LiveActivity>,
    pub rect: AnimatedRect,
    pub opacity: Spring,
    pub radius: f32,
}

impl Default for AnimatedActivitySlot {
    fn default() -> Self {
        Self {
            activity: None,
            rect: AnimatedRect::default(),
            opacity: Spring::new(0.),
            radius: 14.,
        }
    }
}

impl AnimatedActivitySlot {
    pub fn layout(&self) -> Option<ElementLayout> {
        let activity = self.activity.as_ref()?;
        let rect = self.rect.rect();
        (self.opacity.value > 0.01 && rect.w > 0.5 && rect.h > 0.5)
            .then_some(ElementLayout {
                rect,
                radius: self.radius.min(rect.w.min(rect.h) * 0.5),
                opacity: self.opacity.value.clamp(0., 1.),
            })
            .filter(|_| activity.valid())
    }

    fn advance(&mut self, dt: f32, policy: MotionPolicy) {
        self.rect.advance(dt, MotionProfile::Content, policy);
        self.opacity.advance(policy.dt(dt, MotionProfile::Content));
        if self.opacity.target <= 0.01 && !self.opacity.active() && !self.rect.active() {
            self.activity = None;
        }
    }

    pub fn active(&self) -> bool {
        self.rect.active() || self.opacity.active()
    }
}

#[derive(Clone, Debug)]
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
    pub activity_slots: [AnimatedActivitySlot; 2],
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
            activity_slots: std::array::from_fn(|_| AnimatedActivitySlot::default()),
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

    pub fn retarget_activity_slots(
        &mut self,
        targets: &[ElementLayout],
        activities: &[crate::state::LiveActivity],
        anchor: Rect,
        policy: MotionPolicy,
    ) {
        for (index, slot) in self.activity_slots.iter_mut().enumerate() {
            if let (Some(activity), Some(target)) = (activities.get(index), targets.get(index)) {
                if slot.activity.is_none() {
                    slot.activity = Some(activity.clone());
                    slot.rect.set(anchor, true);
                    slot.opacity.set(0., true);
                } else {
                    slot.activity = Some(activity.clone());
                }
                slot.rect.set(target.rect, policy.reduced);
                slot.opacity.set(target.opacity, policy.reduced);
                slot.radius = target.radius;
            } else if slot.activity.is_some() {
                slot.rect.set(anchor, policy.reduced);
                slot.opacity.set(0., policy.reduced);
            }
        }
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
        for slot in &mut self.activity_slots {
            slot.advance(dt, policy);
        }
    }

    pub fn active(&self) -> bool {
        self.album.active()
            || self.title.active()
            || self.artist.active()
            || self.progress.active()
            || self.control_rects.iter().any(AnimatedRect::active)
            || self.content_opacity.active()
            || self.controls_opacity.active()
            || self.activity_slots.iter().any(AnimatedActivitySlot::active)
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
