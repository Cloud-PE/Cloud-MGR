//! Slider（滑动条）：Track / 已填充段(Decrease) / Thumb 三段 + 拖动命中。
//!
//! 真值来源：`controls/dev/CommonStyles/Slider_themeresources.xaml`
//! * Track 高 4、CornerRadius 2，填充 `ControlStrongFillColorDefault`
//! * 已填充段填充 `AccentFillColorDefault`
//! * Thumb：外圈 18(+Margin -2 → 视觉 22) 圆，背景 `ControlSolidFillColorDefault`(#454545)；
//!   内圆 12，填充 `AccentFillColorDefault`，按状态缩放：
//!   Normal 0.86 / PointerOver 1.167 / Pressed 0.71 / Disabled 1.167（FastOutSlowIn）
//! * 边框 SliderBorderThemeThickness=0（rest 无描边）

use crate::anim::{ease_out, lerp};
use crate::widget::*;

const TRACK_H: f32 = 4.0;
const TRACK_R: f32 = 2.0;
const THUMB_OUTER_D: f32 = 22.0; // 18 + Margin(-2)*2
const THUMB_INNER_D: f32 = 12.0;
const THUMB_RADIUS: f32 = THUMB_OUTER_D / 2.0;
const SCALE_NORMAL: f32 = 0.86;
const SCALE_HOVER: f32 = 1.167;
const SCALE_PRESSED: f32 = 0.71;
const SCALE_DISABLED: f32 = 1.167;
// 源码真值：回到 Normal 用 ControlFastAnimationDuration=167ms，进入 hover/pressed 用
// ControlNormalAnimationDuration=250ms；均为 FastOutSlowIn 缓动。
const SCALE_DUR_NORMAL: f64 = 0.167;
const SCALE_DUR_ACTIVE: f64 = 0.25;

pub struct Slider {
    pub value: f32, // 0..1
    pub interaction: Interaction,
    rect: Rect,
    scale_from: f32,
    scale_to: f32,
    scale_start: f64,
    scale_dur: f64,
}

impl Slider {
    pub fn new(value: f32) -> Slider {
        Slider {
            value: value.clamp(0.0, 1.0),
            interaction: Interaction::default(),
            rect: Rect::default(),
            scale_from: SCALE_NORMAL,
            scale_to: SCALE_NORMAL,
            scale_start: 0.0,
            scale_dur: SCALE_DUR_NORMAL,
        }
    }

    fn target_scale(&self) -> f32 {
        if !self.interaction.enabled {
            SCALE_DISABLED
        } else if self.interaction.pressed {
            SCALE_PRESSED
        } else if self.interaction.hovered {
            SCALE_HOVER
        } else {
            SCALE_NORMAL
        }
    }

    fn retarget_scale(&mut self, now: f64) {
        let target = self.target_scale();
        if (self.scale_to - target).abs() < f32::EPSILON {
            return;
        }
        self.scale_from = self.current_scale(now);
        self.scale_to = target;
        self.scale_start = now;
        // 回到 Normal 用较快时长，进入 hover/pressed 用正常时长（源码两档）。
        self.scale_dur = if (target - SCALE_NORMAL).abs() < f32::EPSILON { SCALE_DUR_NORMAL } else { SCALE_DUR_ACTIVE };
    }

    fn current_scale(&self, now: f64) -> f32 {
        if (now - self.scale_start) >= self.scale_dur {
            return self.scale_to;
        }
        let t = ((now - self.scale_start) / self.scale_dur).clamp(0.0, 1.0) as f32;
        lerp(self.scale_from, self.scale_to, ease_out(t))
    }

    /// thumb 中心可移动范围（x），两端留出 thumb 半径避免溢出。
    fn travel(&self) -> (f32, f32) {
        (self.rect.x + THUMB_RADIUS, self.rect.right() - THUMB_RADIUS)
    }

    fn thumb_cx(&self) -> f32 {
        let (x0, x1) = self.travel();
        lerp(x0, x1, self.value)
    }

    fn set_value_from_x(&mut self, x: f32) {
        let (x0, x1) = self.travel();
        let v = ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
        self.value = v;
    }
}

impl Widget for Slider {
    fn measure(&mut self, available: Size) -> Size {
        // 高度取 thumb 直径以容纳；宽度自适应并夹在合理区间。
        Size { w: available.w.clamp(160.0, 280.0), h: THUMB_OUTER_D }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn hit_test(&self, p: Point) -> bool {
        self.rect.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        let t = ctx.tokens;
        let cy = self.rect.center_y();
        let (x0, x1) = self.travel();
        let cx = self.thumb_cx();

        // —— 整条 track ——
        let track = Rect { x: x0, y: cy - TRACK_H / 2.0, w: x1 - x0, h: TRACK_H };
        let track_fill = if self.interaction.enabled { t.control_strong_fill_default } else { t.control_strong_fill_disabled };
        ctx.painter.fill_rounded_rect(track, TRACK_R, track_fill);

        // —— 已填充段（左侧到 thumb 中心）——
        let fill = Rect { x: x0, y: cy - TRACK_H / 2.0, w: (cx - x0).max(0.0), h: TRACK_H };
        let value_fill = if self.interaction.enabled { t.accent_fill_default() } else { t.accent_fill_disabled };
        ctx.painter.fill_rounded_rect(fill, TRACK_R, value_fill);

        // —— Thumb 外圈 ——
        let outer = Rect { x: cx - THUMB_RADIUS, y: cy - THUMB_RADIUS, w: THUMB_OUTER_D, h: THUMB_OUTER_D };
        ctx.painter.fill_rounded_rect(outer, THUMB_RADIUS, t.control_solid_fill_default);

        // —— Thumb 内圆（按状态缩放）——
        let s = self.current_scale(ctx.now);
        let d = THUMB_INNER_D * s;
        let inner = Rect { x: cx - d / 2.0, y: cy - d / 2.0, w: d, h: d };
        let inner_fill = if self.interaction.enabled { t.accent_fill_default() } else { t.accent_fill_disabled };
        ctx.painter.fill_rounded_rect(inner, d / 2.0, inner_fill);
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if !self.interaction.enabled {
            return EventResult::NONE;
        }
        let before = self.interaction;
        let mut value_changed = false;
        match ev {
            InputEvent::PointerMove(p) => {
                self.interaction.hovered = self.rect.contains(p);
                if self.interaction.pressed {
                    self.set_value_from_x(p.x);
                    value_changed = true;
                }
            }
            InputEvent::PointerLeave => {
                self.interaction.hovered = false;
            }
            InputEvent::PointerDown(p) => {
                if self.rect.contains(p) {
                    self.interaction.pressed = true;
                    self.interaction.focused = true;
                    self.set_value_from_x(p.x);
                    value_changed = true;
                }
            }
            InputEvent::PointerUp(_) => self.interaction.pressed = false,
            InputEvent::KeyDown(vk) => match vk {
                0x25 => { self.value = (self.value - 0.01).clamp(0.0, 1.0); value_changed = true; } // Left
                0x27 => { self.value = (self.value + 0.01).clamp(0.0, 1.0); value_changed = true; } // Right
                _ => {}
            },
            _ => {}
        }
        let state_changed = before.visual_state() != self.interaction.visual_state();
        if state_changed {
            self.retarget_scale(now);
        }
        EventResult {
            redraw: state_changed || value_changed,
            animating: state_changed,
        }
    }

    fn is_animating(&self, now: f64) -> bool {
        (now - self.scale_start) < self.scale_dur
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Slider
    }
}
