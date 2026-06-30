//! ToggleSwitch（开关），含 knob 位移 + 尺寸形变（hover/press morph）+ 配色交叉淡入。
//!
//! 真值来源：`controls/dev/CommonStyles/ToggleSwitch_themeresources.xaml`
//! * 轨道 OuterBorder：Width 40、Height 20、Radius 10、StrokeThickness 1（off）
//! * Knob：rest 12×12、PointerOver 14×14、**Pressed 17×14（拉长）**，
//!   尺寸过渡 `ControlFasterAnimationDuration=83ms` + FastOutSlowIn
//! * On/Off：knob 用 RepositionThemeAnimation 平移（X off=0→on=20，约 0.2s 缓动）；
//!   轨道(OuterBorder↔SwitchKnobBounds)与 knob 配色用 **83ms 线性** 交叉淡入
//! * Off 填充 `ControlAltFillColorSecondary`(hover Tertiary/pressed Quarternary)，描边
//!   `ControlStrongStrokeColorDefault`，knob `TextFillColorSecondary`；On 填充 `AccentFillColorDefault`

use crate::anim::{ease_out, lerp, lerp_color};
use crate::widget::*;

const TRACK_W: f32 = 40.0;
const TRACK_H: f32 = 20.0;
const TRACK_R: f32 = 10.0;
const TRACK_STROKE: f32 = 1.0;
const KNOB_OFF_CX: f32 = 10.0; // 轨道局部坐标
const KNOB_ON_CX: f32 = 30.0; // = 10 + TranslateX(20)

const POS_DUR: f64 = 0.2; // knob 平移（RepositionThemeAnimation 近似）
const FADE_DUR: f64 = 0.083; // 配色/轨道交叉淡入（线性）
const SIZE_DUR: f64 = 0.083; // knob 尺寸形变

// knob 尺寸（宽,高）：normal / hover / pressed
const KNOB_N: (f32, f32) = (12.0, 12.0);
const KNOB_H: (f32, f32) = (14.0, 14.0);
const KNOB_P: (f32, f32) = (17.0, 14.0);

pub struct ToggleSwitch {
    pub on: bool,
    pub on_text: String,
    pub off_text: String,
    pub interaction: Interaction,
    rect: Rect,
    // on/off 进度：0=off, 1=on
    anim_from: f32,
    anim_to: f32,
    anim_start: f64,
    // knob 尺寸形变
    kw_from: f32,
    kh_from: f32,
    kw_to: f32,
    kh_to: f32,
    size_start: f64,
}

impl ToggleSwitch {
    pub fn new(on: bool) -> ToggleSwitch {
        let v = if on { 1.0 } else { 0.0 };
        ToggleSwitch {
            on,
            on_text: "On".into(),
            off_text: "Off".into(),
            interaction: Interaction::default(),
            rect: Rect::default(),
            anim_from: v,
            anim_to: v,
            anim_start: -1.0,
            kw_from: KNOB_N.0,
            kh_from: KNOB_N.1,
            kw_to: KNOB_N.0,
            kh_to: KNOB_N.1,
            size_start: -1.0,
        }
    }

    /// on/off 位移进度（FastOutSlowIn）。
    fn pos_progress(&self, now: f64) -> f32 {
        if self.anim_start < 0.0 || (now - self.anim_start) >= POS_DUR {
            return self.anim_to;
        }
        let t = ((now - self.anim_start) / POS_DUR).clamp(0.0, 1.0) as f32;
        lerp(self.anim_from, self.anim_to, ease_out(t))
    }

    /// on/off 配色交叉淡入进度（线性 83ms）。
    fn fade_progress(&self, now: f64) -> f32 {
        if self.anim_start < 0.0 || (now - self.anim_start) >= FADE_DUR {
            return self.anim_to;
        }
        let t = ((now - self.anim_start) / FADE_DUR).clamp(0.0, 1.0) as f32;
        lerp(self.anim_from, self.anim_to, t) // 线性
    }

    fn toggle(&mut self, now: f64) {
        self.anim_from = self.pos_progress(now);
        self.on = !self.on;
        self.anim_to = if self.on { 1.0 } else { 0.0 };
        self.anim_start = now;
    }

    fn knob_target(&self) -> (f32, f32) {
        if !self.interaction.enabled {
            KNOB_N
        } else if self.interaction.pressed {
            KNOB_P
        } else if self.interaction.hovered {
            KNOB_H
        } else {
            KNOB_N
        }
    }

    fn retarget_size(&mut self, now: f64) {
        let (tw, th) = self.knob_target();
        if (self.kw_to - tw).abs() < f32::EPSILON && (self.kh_to - th).abs() < f32::EPSILON {
            return;
        }
        let (cw, ch) = self.cur_size(now);
        self.kw_from = cw;
        self.kh_from = ch;
        self.kw_to = tw;
        self.kh_to = th;
        self.size_start = now;
    }

    fn cur_size(&self, now: f64) -> (f32, f32) {
        if self.size_start < 0.0 || (now - self.size_start) >= SIZE_DUR {
            return (self.kw_to, self.kh_to);
        }
        let t = ease_out(((now - self.size_start) / SIZE_DUR).clamp(0.0, 1.0) as f32);
        (lerp(self.kw_from, self.kw_to, t), lerp(self.kh_from, self.kh_to, t))
    }

    fn track(&self) -> Rect {
        Rect { x: self.rect.x, y: self.rect.center_y() - TRACK_H / 2.0, w: TRACK_W, h: TRACK_H }
    }
}

impl Widget for ToggleSwitch {
    fn measure(&mut self, _available: Size) -> Size {
        Size { w: TRACK_W, h: TRACK_H }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn hit_test(&self, p: Point) -> bool {
        self.rect.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        let t = ctx.tokens;
        let now = ctx.now;
        let pos = self.pos_progress(now); // 位移
        let fade = self.fade_progress(now); // 配色交叉淡入
        let track = self.track();
        let vs = self.interaction.visual_state();

        // —— off 轨道（始终绘制）——
        let off_fill = if !self.interaction.enabled {
            t.control_alt_fill_disabled
        } else {
            match vs {
                VisualState::PointerOver => t.control_alt_fill_tertiary,
                VisualState::Pressed => t.control_alt_fill_quarternary,
                _ => t.control_alt_fill_secondary,
            }
        };
        ctx.painter.fill_rounded_rect(track, TRACK_R, off_fill);
        let off_stroke = if self.interaction.enabled { t.strong_stroke_default } else { t.strong_stroke_disabled };
        ctx.painter.stroke_inner(track, TRACK_R, off_stroke, TRACK_STROKE);

        // —— on 轨道（accent）以不透明度 fade 交叉淡入 ——
        if fade > 0.0 {
            let on_fill = if !self.interaction.enabled {
                t.accent_fill_disabled
            } else {
                match vs {
                    VisualState::PointerOver => t.accent_fill_secondary(),
                    VisualState::Pressed => t.accent_fill_tertiary(),
                    _ => t.accent_fill_default(),
                }
            };
            ctx.painter.fill_rounded_rect(track, TRACK_R, on_fill.with_opacity(fade));
        }

        // —— knob：位置随 pos 缓动，配色随 fade 交叉淡入，尺寸随 state 形变 ——
        let cx = track.x + lerp(KNOB_OFF_CX, KNOB_ON_CX, pos);
        let cy = track.center_y();
        let (kw, kh) = self.cur_size(now);
        let off_knob = if self.interaction.enabled { t.text_secondary } else { t.text_disabled };
        let on_knob = if self.interaction.enabled { t.text_on_accent_primary } else { t.text_on_accent_disabled };
        let knob_color = lerp_color(off_knob, on_knob, fade);
        let knob = Rect { x: cx - kw / 2.0, y: cy - kh / 2.0, w: kw, h: kh };
        ctx.painter.fill_rounded_rect(knob, kh / 2.0, knob_color);
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if !self.interaction.enabled {
            return EventResult::NONE;
        }
        let before = self.interaction;
        let mut toggled = false;
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
                    self.toggle(now);
                    toggled = true;
                }
                self.interaction.pressed = false;
            }
            InputEvent::KeyDown(vk) => {
                if vk == 0x20 || vk == 0x0D {
                    self.toggle(now);
                    toggled = true;
                }
            }
            _ => {}
        }
        let state_changed = before.visual_state() != self.interaction.visual_state();
        if state_changed {
            self.retarget_size(now);
        }
        let changed = toggled || state_changed;
        EventResult { redraw: changed, animating: changed }
    }

    fn is_animating(&self, now: f64) -> bool {
        (self.anim_start >= 0.0 && (now - self.anim_start) < POS_DUR)
            || (self.size_start >= 0.0 && (now - self.size_start) < SIZE_DUR)
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::CheckBox
    }
    fn accessible_name(&self) -> String {
        if self.on { self.on_text.clone() } else { self.off_text.clone() }
    }
}
