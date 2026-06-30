//! Button（普通）与 AccentButton（蓝）。
//!
//! 真值来源：fluent-svelte `src/lib/Button/Button.scss`（MIT Svelte/CSS WinUI 端口）。
//! * `.button`：padding-block 4px 6px、padding-inline 11px、border-radius var(--control-corner-radius)=4、
//!   border 1px、typography-body（font-size 14 / line-height 20）→ 自然高 20+4+6+1+1=32。
//! * `transition: var(--control-faster-duration) ease background`（83ms，仅 background）。
//! * `&:focus-visible { box-shadow: var(--focus-stroke) }`
//!   = `0 0 0 1px inner, 0 0 0 3px outer` → 贴边 1px 内环(focus-stroke-inner) + 到 3px 外环(focus-stroke-outer)，瞬时。
//! * `.style-standard`：border 1px solid control-border-default、bg control-fill-default、color text-primary、
//!   background-clip padding-box(=InnerBorderEdge)；hover→fill-secondary；
//!   active→border control-stroke-default + fill-tertiary + text-secondary；
//!   disabled→border control-stroke-default + fill-disabled + text-disabled。
//!   ——用户已自定义此面的取色：bg=input_solid_bg/hover/pressed、border=input_border + 底边 input_border_bottom，
//!     此处保留用户取色，仅复刻几何/构造/状态逻辑。
//! * `.style-accent`：border 1px solid control-stroke-on-accent-default + border-bottom control-stroke-on-accent-secondary
//!   （**实心环 + 底边线**，非 WinUI 渐变 ElevationBorder）、bg accent-default、color text-on-accent-primary、
//!   `transition: var(--control-faster-duration) ease border-color`（83ms，仅 border-color；bg 即时切换）；
//!   hover→accent-secondary；active→border-color transparent + accent-tertiary + text-on-accent-secondary；
//!   disabled→border-color transparent + accent-disabled + text-on-accent-disabled。

use crate::anim::ColorTransition;
use crate::color::Color;
use crate::tokens::Tokens;
use crate::typography::TextStyle;
use crate::widget::*;

const PADDING_L: f32 = 11.0; // Button.scss:10 padding-inline:11px
const PADDING_T: f32 = 4.0; // Button.scss:9 padding-block:4px 6px (top)
const PADDING_R: f32 = 11.0; // Button.scss:10 padding-inline:11px
const PADDING_B: f32 = 6.0; // Button.scss:9 padding-block:4px 6px (bottom)
const CORNER: f32 = 4.0; // var(--control-corner-radius)=4px (theme.css:32)
const BORDER: f32 = 1.0; // Button.scss:24/48 border:1px
const MIN_HEIGHT: f32 = 32.0; // line-height 20 + padding 4+6 + border 1+1
const BG_TRANSITION: f64 = 0.083; // var(--control-faster-duration)=83ms (theme.css:40)
const BORDER_TRANSITION: f64 = 0.083; // accent: transition ... ease border-color (Button.scss:52)

// 焦点环（Button.scss:18-20，--fds-focus-stroke = 0 0 0 1px inner, 0 0 0 3px outer）。
// 贴 border-box 外扩：0→1px 内环（focus-stroke-inner），1px→3px 外环（focus-stroke-outer，2px 厚）。瞬时。
const FOCUS_INNER: f32 = 1.0; // 内环厚（box-shadow spread 第一段 0→1px）
const FOCUS_OUTER: f32 = 2.0; // 外环厚（box-shadow spread 第二段 1px→3px）

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Standard,
    Accent,
}

pub struct Button {
    pub text: String,
    pub style: ButtonStyle,
    pub interaction: Interaction,
    rect: Rect,
    bg: ColorTransition,
    /// 蓝色按钮 border-color 过渡（环色，83ms ease；active/disabled→transparent）。
    border_ring: ColorTransition,
    /// 蓝色按钮 border-bottom-color 过渡（底边，与环同步过渡）。
    border_bottom: ColorTransition,
    initialized: bool,
    /// 焦点环仅在**键盘聚焦**(:focus-visible)时显示；鼠标点击不显示。当前框架无 Tab 导航，故恒 false。
    kbd_focus: bool,
}

impl Button {
    pub fn new(text: impl Into<String>, style: ButtonStyle) -> Button {
        Button {
            text: text.into(),
            style,
            interaction: Interaction::default(),
            rect: Rect::default(),
            bg: ColorTransition::instant(Color::TRANSPARENT),
            border_ring: ColorTransition::instant(Color::TRANSPARENT),
            border_bottom: ColorTransition::instant(Color::TRANSPARENT),
            initialized: false,
            kbd_focus: false,
        }
    }

    pub fn standard(text: impl Into<String>) -> Button {
        Button::new(text, ButtonStyle::Standard)
    }
    pub fn accent(text: impl Into<String>) -> Button {
        Button::new(text, ButtonStyle::Accent)
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.interaction.enabled = enabled;
    }

    fn bg_for(&self, t: &Tokens, vs: VisualState) -> Color {
        match (self.style, vs) {
            // 用户取色（保留）：标准按钮 = ComboBox 同一组 input_* 取色。
            (ButtonStyle::Standard, VisualState::Normal) => t.input_solid_bg(),
            (ButtonStyle::Standard, VisualState::PointerOver) => t.input_bg_hover(),
            (ButtonStyle::Standard, VisualState::Pressed) => t.input_bg_pressed(),
            (ButtonStyle::Standard, VisualState::Disabled) => t.input_solid_bg(),
            // 蓝色按钮 = fluent-svelte 取色（accent-default/secondary/tertiary/disabled）。
            (ButtonStyle::Accent, VisualState::Normal) => t.accent_fill_default(),
            (ButtonStyle::Accent, VisualState::PointerOver) => t.accent_fill_secondary(),
            (ButtonStyle::Accent, VisualState::Pressed) => t.accent_fill_tertiary(),
            (ButtonStyle::Accent, VisualState::Disabled) => t.accent_fill_disabled,
        }
    }

    fn fg_for(&self, t: &Tokens, vs: VisualState) -> Color {
        match (self.style, vs) {
            // Button.scss:37 active→text-secondary；:44 disabled→text-disabled；rest/hover→text-primary
            (ButtonStyle::Standard, VisualState::Pressed) => t.text_secondary,
            (ButtonStyle::Standard, VisualState::Disabled) => t.text_disabled,
            (ButtonStyle::Standard, _) => t.text_primary,
            // Button.scss:61 active→text-on-accent-secondary；:67 disabled→text-on-accent-disabled；rest/hover→primary
            (ButtonStyle::Accent, VisualState::Pressed) => t.text_on_accent_secondary,
            (ButtonStyle::Accent, VisualState::Disabled) => t.text_on_accent_disabled,
            (ButtonStyle::Accent, _) => t.text_on_accent_primary,
        }
    }

    /// 蓝色按钮 border-color 目标（环色）：rest/hover=stroke-on-accent-default，active/disabled=transparent。
    /// Button.scss:48 border-color、:59 active→transparent、:65 disabled→transparent。
    fn accent_border_ring(&self, t: &Tokens, vs: VisualState) -> Color {
        match vs {
            VisualState::Normal | VisualState::PointerOver => t.stroke_on_accent_default,
            _ => Color::TRANSPARENT,
        }
    }
    /// 蓝色按钮 border-bottom-color 目标：rest/hover=stroke-on-accent-secondary，active/disabled=transparent。
    /// Button.scss:49 border-bottom-color；active/disabled 时整圈 border-color 被覆盖为 transparent。
    fn accent_border_bottom(&self, t: &Tokens, vs: VisualState) -> Color {
        match vs {
            VisualState::Normal | VisualState::PointerOver => t.stroke_on_accent_secondary,
            _ => Color::TRANSPARENT,
        }
    }

    /// 内边距盒（文字居中于此，匹配 11,4,11,6 的非对称内边距）。
    fn content_box(&self) -> Rect {
        Rect {
            x: self.rect.x + PADDING_L,
            y: self.rect.y + PADDING_T,
            w: (self.rect.w - PADDING_L - PADDING_R).max(0.0),
            h: (self.rect.h - PADDING_T - PADDING_B).max(0.0),
        }
    }
}

impl Widget for Button {
    fn measure(&mut self, _available: Size) -> Size {
        // 宽 = 文字宽 + 左右内边距；高 = max(32, 行高 + 上下内边距)。
        // 文字宽在 paint 阶段用真实 DWrite 测量更准；measure 给一个基于字符数的估算上界。
        let approx_text_w = self.text.chars().count() as f32 * 7.0;
        Size {
            w: approx_text_w + PADDING_L + PADDING_R,
            h: MIN_HEIGHT.max(TextStyle::BODY.line_height + PADDING_T + PADDING_B),
        }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn hit_test(&self, p: Point) -> bool {
        self.rect.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        let t = ctx.tokens;
        let vs = self.interaction.visual_state();
        let r = self.rect;

        // —— 背景 ——
        // 标准按钮：background 过渡 83ms（Button.scss:16 base transition ... background）。
        // 蓝色按钮：transition 被 .accent 覆盖为仅 border-color → background 即时切换（Button.scss:52）。
        let bg_target = self.bg_for(t, vs);
        // 蓝色按钮 border-color 目标（83ms ease 过渡）。
        let ring_target = self.accent_border_ring(t, vs);
        let bottom_target = self.accent_border_bottom(t, vs);

        if !self.initialized {
            self.bg = ColorTransition::instant(bg_target);
            self.border_ring = ColorTransition::instant(ring_target);
            self.border_bottom = ColorTransition::instant(bottom_target);
            self.initialized = true;
        } else {
            match self.style {
                ButtonStyle::Standard => {
                    if bg_target != self.bg.to {
                        self.bg.retarget(bg_target, ctx.now, BG_TRANSITION);
                    }
                }
                ButtonStyle::Accent => {
                    // bg 即时；border-color 83ms 过渡。
                    self.bg = ColorTransition::instant(bg_target);
                    if ring_target != self.border_ring.to {
                        self.border_ring.retarget(ring_target, ctx.now, BORDER_TRANSITION);
                    }
                    if bottom_target != self.border_bottom.to {
                        self.border_bottom.retarget(bottom_target, ctx.now, BORDER_TRANSITION);
                    }
                }
            }
        }
        let bg = self.bg.value(ctx.now);

        match self.style {
            // 蓝色按钮：background-clip 默认 border-box → 背景铺满外缘（OuterBorderEdge），边框压其上。
            ButtonStyle::Accent => {
                ctx.painter.fill_rounded_rect(r, CORNER, bg);
            }
            // 标准按钮：background-clip padding-box（InnerBorderEdge）→ 背景只铺到边框内沿。
            ButtonStyle::Standard => {
                ctx.painter
                    .fill_rounded_rect(r.inset(BORDER), (CORNER - BORDER).max(0.0), bg);
            }
        }

        // —— 边框 ——
        match self.style {
            // 蓝色按钮（fluent-svelte 构造）：实心环 stroke-on-accent-default + 底边 stroke-on-accent-secondary，
            // 整体 83ms 过渡（active/disabled 时两者→transparent）。
            ButtonStyle::Accent => {
                let ring = self.border_ring.value(ctx.now);
                let bottom = self.border_bottom.value(ctx.now);
                ctx.painter.stroke_inner(r, CORNER, ring, BORDER);
                // border-bottom-color：覆盖底边那 1px。直边段（避开圆角）填 secondary 色。
                let bl = Rect {
                    x: r.x + CORNER,
                    y: r.bottom() - BORDER,
                    w: (r.w - CORNER * 2.0).max(0.0),
                    h: BORDER,
                };
                ctx.painter.fill_rect(bl, bottom);
            }
            // 标准按钮（保留用户取色）：整圈 input_border + 底边 input_border_bottom（与 ComboBox 一致）。
            ButtonStyle::Standard => {
                ctx.painter.stroke_inner(r, CORNER, t.input_border(), BORDER);
                let bl = Rect {
                    x: r.x + CORNER,
                    y: r.bottom() - BORDER,
                    w: (r.w - CORNER * 2.0).max(0.0),
                    h: BORDER,
                };
                ctx.painter.fill_rect(bl, t.input_border_bottom());
            }
        }

        // —— 文字 ——
        let fg = self.fg_for(t, vs);
        let _ = ctx
            .painter
            .draw_text_centered(&self.text, TextStyle::BODY, self.content_box(), fg);

        // —— 焦点环（Button.scss:18-20 :focus-visible box-shadow var(--focus-stroke)）——
        // box-shadow 不在任何 transition 列表 → 瞬时显隐。贴 border-box 外扩，无 margin。
        if self.kbd_focus && self.interaction.enabled {
            // 外环：0→3px（focus-stroke-outer），故矩形外扩 FOCUS_INNER+FOCUS_OUTER=3px，描 2px。
            let outer = Rect {
                x: r.x - (FOCUS_INNER + FOCUS_OUTER),
                y: r.y - (FOCUS_INNER + FOCUS_OUTER),
                w: r.w + 2.0 * (FOCUS_INNER + FOCUS_OUTER),
                h: r.h + 2.0 * (FOCUS_INNER + FOCUS_OUTER),
            };
            ctx.painter.stroke_inner(
                outer,
                CORNER + FOCUS_INNER + FOCUS_OUTER,
                t.focus_stroke_outer,
                FOCUS_OUTER,
            );
            // 内环：0→1px（focus-stroke-inner），贴 border-box 外 1px，描 1px。
            let inner = Rect {
                x: r.x - FOCUS_INNER,
                y: r.y - FOCUS_INNER,
                w: r.w + 2.0 * FOCUS_INNER,
                h: r.h + 2.0 * FOCUS_INNER,
            };
            ctx.painter
                .stroke_inner(inner, CORNER + FOCUS_INNER, t.focus_stroke_inner, FOCUS_INNER);
        }
    }

    fn on_event(&mut self, ev: InputEvent, _now: f64) -> EventResult {
        if !self.interaction.enabled {
            return EventResult::NONE;
        }
        let before = self.interaction;
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
                } else {
                    self.interaction.focused = false;
                }
            }
            InputEvent::PointerUp(_) => self.interaction.pressed = false,
            _ => {}
        }
        let changed = before.visual_state() != self.interaction.visual_state()
            || before.focused != self.interaction.focused;
        EventResult { redraw: changed, animating: changed }
    }

    fn is_animating(&self, now: f64) -> bool {
        self.bg.is_active(now)
            || self.border_ring.is_active(now)
            || self.border_bottom.is_active(now)
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Button
    }
    fn accessible_name(&self) -> String {
        self.text.clone()
    }
}
