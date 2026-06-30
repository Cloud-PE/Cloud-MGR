//! RadioButton（单选按钮，含圆点缩放动画）。
//!
//! 真值来源：`controls/dev/CommonStyles/RadioButton_themeresources.xaml`
//! * OuterEllipse 20×20，边框 1；Unchecked 描边 `ControlStrongStrokeColorDefault`
//! * Checked：accent 外环 + accent 圆点（CheckGlyph，缩放淡入），按下时圆点更小(press hint)
//! * 圆点尺寸 rest ~10 / pressed ~14（press 时变大），缩放过渡 ~150ms FastOutSlowIn
//!
//! 注：组内互斥需由容器管理；此处控件自身仅维护 checked + 点击置位，演示圆点动画。

use crate::anim::{ease_out, lerp};
use crate::typography::TextStyle;
use crate::widget::*;

const RING: f32 = 20.0;
const LABEL_GAP: f32 = 8.0;
const DOT_REST: f32 = 10.0;
const DOT_PRESSED: f32 = 14.0;
const DUR: f64 = 0.15;

pub struct RadioButton {
    pub checked: bool,
    pub label: String,
    pub interaction: Interaction,
    rect: Rect,
    dot_from: f32,
    dot_to: f32,
    anim_start: f64,
}

impl RadioButton {
    pub fn new(label: impl Into<String>, checked: bool) -> RadioButton {
        RadioButton {
            checked,
            label: label.into(),
            interaction: Interaction::default(),
            rect: Rect::default(),
            dot_from: if checked { DOT_REST } else { 0.0 },
            dot_to: if checked { DOT_REST } else { 0.0 },
            anim_start: -1.0,
        }
    }

    fn target_dot(&self) -> f32 {
        if !self.checked {
            0.0
        } else if self.interaction.pressed {
            DOT_PRESSED
        } else {
            DOT_REST
        }
    }

    fn retarget(&mut self, now: f64) {
        let target = self.target_dot();
        if (self.dot_to - target).abs() < f32::EPSILON {
            return;
        }
        self.dot_from = self.cur_dot(now);
        self.dot_to = target;
        self.anim_start = now;
    }

    fn cur_dot(&self, now: f64) -> f32 {
        if self.anim_start < 0.0 || (now - self.anim_start) >= DUR {
            return self.dot_to;
        }
        let t = ease_out(((now - self.anim_start) / DUR).clamp(0.0, 1.0) as f32);
        lerp(self.dot_from, self.dot_to, t)
    }

    fn ring_rect(&self) -> Rect {
        Rect { x: self.rect.x, y: self.rect.center_y() - RING / 2.0, w: RING, h: RING }
    }
}

impl Widget for RadioButton {
    fn measure(&mut self, _available: Size) -> Size {
        let w = if self.label.is_empty() { RING } else { RING + LABEL_GAP + self.label.chars().count() as f32 * 8.0 };
        Size { w, h: 32.0 }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn hit_test(&self, p: Point) -> bool {
        self.rect.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        let t = ctx.tokens;
        let r = self.ring_rect();
        let cx = r.center_x();
        let cy = r.center_y();
        let vs = self.interaction.visual_state();

        // 外环
        let ring_color = if self.checked {
            if !self.interaction.enabled {
                t.accent_fill_disabled
            } else {
                match vs {
                    VisualState::PointerOver => t.accent_fill_secondary(),
                    VisualState::Pressed => t.accent_fill_tertiary(),
                    _ => t.accent_fill_default(),
                }
            }
        } else if self.interaction.enabled {
            t.strong_stroke_default
        } else {
            t.strong_stroke_disabled
        };
        // 未选时弱填充
        if !self.checked {
            let fill = match vs {
                VisualState::PointerOver => t.control_alt_fill_tertiary,
                VisualState::Pressed => t.control_alt_fill_quarternary,
                _ => t.control_alt_fill_secondary,
            };
            ctx.painter.fill_circle(cx, cy, RING / 2.0 - 0.5, fill);
        }
        ctx.painter.stroke_circle(cx, cy, RING / 2.0 - 0.5, ring_color, 1.0);

        // 圆点（缩放）
        let d = self.cur_dot(ctx.now);
        if d > 0.5 {
            let dot_color = if self.interaction.enabled { t.accent_fill_default() } else { t.accent_fill_disabled };
            ctx.painter.fill_circle(cx, cy, d / 2.0, dot_color);
        }

        if !self.label.is_empty() {
            let fg = if self.interaction.enabled { t.text_primary } else { t.text_disabled };
            let lr = Rect { x: r.right() + LABEL_GAP, y: self.rect.y, w: self.rect.w - RING - LABEL_GAP, h: self.rect.h };
            let _ = ctx.painter.draw_text_leading(&self.label, TextStyle::BODY, lr, fg);
        }
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if !self.interaction.enabled {
            return EventResult::NONE;
        }
        let before = self.interaction;
        let mut acted = false;
        match ev {
            InputEvent::PointerMove(p) => self.interaction.hovered = self.rect.contains(p),
            InputEvent::PointerLeave => {
                self.interaction.hovered = false;
                self.interaction.pressed = false;
            }
            InputEvent::PointerDown(p) => {
                if self.rect.contains(p) {
                    self.interaction.pressed = true;
                    self.interaction.focused = true;
                }
            }
            InputEvent::PointerUp(p) => {
                if self.interaction.pressed && self.rect.contains(p) {
                    self.checked = !self.checked; // 演示：可切换以观察圆点动画
                    acted = true;
                }
                self.interaction.pressed = false;
            }
            _ => {}
        }
        let state_changed = before.visual_state() != self.interaction.visual_state();
        if state_changed || acted {
            self.retarget(now);
        }
        EventResult { redraw: state_changed || acted, animating: state_changed || acted }
    }

    fn is_animating(&self, now: f64) -> bool {
        self.anim_start >= 0.0 && (now - self.anim_start) < DUR
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::CheckBox
    }
    fn accessible_name(&self) -> String {
        self.label.clone()
    }
}
