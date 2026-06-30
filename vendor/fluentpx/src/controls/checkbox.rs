//! CheckBox —— 1:1 复刻 fluent-svelte 的 `Checkbox`（MIT Svelte/CSS WinUI 端口）。
//!
//! 真值来源：`fluent-svelte/src/lib/Checkbox/Checkbox.scss` + `Checkbox.svelte`，
//! token 解析自 `fluent-svelte/src/lib/theme.css`（durations / easing / 颜色）。
//!
//! 结构（Checkbox.scss）：
//! * 方框 `.checkbox` `inline-size:20px; block-size:20px;`，`border:1px solid`，
//!   `border-radius: var(--control-corner-radius)=4px`（描边居中，非内沿——CSS `border`）。
//! * 行容器 `.checkbox-container` `min-block-size:32px`，`> span { padding-inline-start:8px }`。
//! * 勾形 `.checkbox-glyph`：`position:absolute; inline-size:12px; block-size:12px;`，
//!   居中于 20×20 框（`.checkbox-inner` flex center）→ 左上落在框内 (4,4)。
//!   - checkmark：viewBox `0 0 24 24`，`d="M 4.5303 12.9697 L 8.5 16.9393 L 18.9697 6.4697"`，
//!     `transform:scale(1.2)`（transform-origin center=vb(12,12)），`stroke-width:2`，圆头圆角，
//!     `stroke-dasharray:20.5; stroke-dashoffset:20.5`。
//!     映射：vb→glyph px ×0.5，glyph 左上 (4,4)。scale(1.2) 关于 (12,12)。
//!       P0 vb(4.5303,12.9697) → px(5.518,10.582)
//!       P1 vb(8.5,   16.9393) → px(7.900,12.964)
//!       P2 vb(18.9697,6.4697) → px(14.182,6.682)（相对框左上）
//!     线宽 = 2 × 1.2 × 0.5 = 1.2px。路径长≈20.42vb ≈ dasharray 20.5 → 全长揭示。
//!   - indeterminate：viewBox `171 470 683 85` 的胶囊（683×85，几乎填满 viewBox），
//!     `transform:scale(2/3)`。meet 后宽填满 12px、高 85/683×12≈1.49px，再 ×2/3 →
//!     **8px 宽 × ~1px 高的胶囊，居中于框中心 (10,10)，圆角=半高≈0.5**。
//! * 颜色（Checkbox.scss 各状态）：
//!   - Unchecked 填充：rest=control-alt-fill-secondary / hover=tertiary / active=quarternary / disabled=disabled
//!   - Unchecked 描边：rest+hover=control-strong-stroke-default / active+disabled=control-strong-stroke-disabled
//!   - Checked|Indeterminate：`border:none`（无描边，含 disabled）；
//!     填充 rest=accent-default / hover=accent-secondary / active=accent-tertiary / disabled=accent-disabled
//!   - 勾/横前景：默认 text-on-accent-primary；active=text-on-accent-secondary；disabled=text-on-accent-disabled
//! * 动画（Checkbox.scss 第 59-62 行）：仅 checkmark 的 `stroke-dashoffset` 过渡，
//!   `var(--control-normal-duration)=250ms`，`cubic-bezier(0.55,0,0,1)`，TrimEnd 揭示（无擦除动画）。
//!   背景/描边颜色无 transition（瞬时）；indeterminate 无 transition（瞬时吸附）。
//! * 焦点（theme.css `--fds-focus-stroke`）：作用于 20×20 框，
//!   `box-shadow: 0 0 0 1px inner, 0 0 0 3px outer`——内环 1px（spread 1）+ 外环带 2px（spread 1→3），
//!   颜色 inner=focus-stroke-inner / outer=focus-stroke-outer，瞬时显隐。

use crate::anim::cubic_bezier;
use crate::typography::TextStyle;
use crate::widget::*;

const BOX: f32 = 20.0; // inline/block-size 20px
const CORNER: f32 = 4.0; // --control-corner-radius
const BORDER: f32 = 1.0; // border 1px
const LABEL_GAP: f32 = 8.0; // span padding-inline-start
const ROW_MIN_H: f32 = 32.0; // .checkbox-container min-block-size

// 勾揭示时长/缓动（Checkbox.scss:60）：--control-normal-duration=250ms，cubic-bezier(0.55,0,0,1)。
const DRAW_DUR: f64 = 0.250;

// checkmark 折线顶点（相对框左上，px），由 vb 顶点经 scale(1.2)@center + ×0.5 + 偏移(4,4) 算得。
const CHECK_P0: (f32, f32) = (5.518, 10.582);
const CHECK_P1: (f32, f32) = (7.900, 12.964);
const CHECK_P2: (f32, f32) = (14.182, 6.682);
// 勾线宽：stroke-width 2 × scale 1.2 × (12/24) = 1.2px。
const CHECK_STROKE: f32 = 1.2;

// indeterminate 胶囊：8px 宽 × ~1px 高，居中于框心，圆角=半高。
const DASH_W: f32 = 8.0;
const DASH_H: f32 = 1.0;

// 焦点环（theme.css --fds-focus-stroke）：spread 1px 内环 + spread 3px 外环（外带 2px），贴 20×20 框。
const FOCUS_INNER_SPREAD: f32 = 1.0;
const FOCUS_OUTER_SPREAD: f32 = 3.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Unchecked,
    Checked,
    Indeterminate,
}

pub struct CheckBox {
    pub state: CheckState,
    pub label: String,
    pub interaction: Interaction,
    rect: Rect,
    /// 勾揭示动画起点（秒）；<0 表示无动画（吸附到终态，dashoffset=0）。
    anim_start: f64,
}

impl CheckBox {
    pub fn new(label: impl Into<String>, state: CheckState) -> CheckBox {
        CheckBox {
            state,
            label: label.into(),
            interaction: Interaction::default(),
            rect: Rect::default(),
            anim_start: -1.0,
        }
    }

    fn box_rect(&self) -> Rect {
        Rect { x: self.rect.x, y: self.rect.center_y() - BOX / 2.0, w: BOX, h: BOX }
    }

    fn toggle(&mut self, now: f64) {
        // 二态切换（Indeterminate 视作已选，点击后变 Unchecked）。
        let was_checked = self.state == CheckState::Checked;
        self.state = match self.state {
            CheckState::Unchecked => CheckState::Checked,
            _ => CheckState::Unchecked,
        };
        let now_checked = self.state == CheckState::Checked;
        // 仅 Unchecked→Checked 触发勾的 dashoffset 揭示（fluent-svelte 只有 :checked 带 transition）。
        // Checked→Unchecked 无过渡（CSS 移除 transition）→ 瞬时吸附（勾消失）。
        if now_checked && !was_checked {
            self.anim_start = now;
        } else {
            self.anim_start = -1.0;
        }
    }

    /// 勾揭示的线性时间进度 0..1（无动画返回 1）。缓动在绘制处应用。
    fn anim_lin(&self, now: f64) -> f32 {
        if self.anim_start < 0.0 {
            return 1.0;
        }
        ((now - self.anim_start) / DRAW_DUR).clamp(0.0, 1.0) as f32
    }
}

impl Widget for CheckBox {
    fn measure(&mut self, _available: Size) -> Size {
        let w = if self.label.is_empty() {
            BOX
        } else {
            BOX + LABEL_GAP + self.label.chars().count() as f32 * 8.0
        };
        Size { w, h: ROW_MIN_H }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn hit_test(&self, p: Point) -> bool {
        self.rect.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        let t = ctx.tokens;
        let b = self.box_rect();
        let vs = self.interaction.visual_state();
        let enabled = self.interaction.enabled;
        let filled = self.state != CheckState::Unchecked; // :checked | :indeterminate

        if filled {
            // Checked / Indeterminate：实心 accent 框，`border:none`（无描边，含 disabled）。
            let fill = if !enabled {
                t.accent_fill_disabled // --accent-disabled
            } else {
                match vs {
                    VisualState::PointerOver => t.accent_fill_secondary(), // --accent-secondary
                    VisualState::Pressed => t.accent_fill_tertiary(),      // --accent-tertiary
                    _ => t.accent_fill_default(),                          // --accent-default
                }
            };
            ctx.painter.fill_rounded_rect(b, CORNER, fill);
        } else {
            // Unchecked：弱填充 + 强描边（border 1px，CSS 居中描边 → 用 stroke_inner 内沿近似）。
            let fill = match vs {
                VisualState::PointerOver => t.control_alt_fill_tertiary,    // hover
                VisualState::Pressed => t.control_alt_fill_quarternary,     // active
                VisualState::Disabled => t.control_alt_fill_disabled,       // disabled
                _ => t.control_alt_fill_secondary,                          // rest
            };
            ctx.painter.fill_rounded_rect(b, CORNER, fill);
            // 描边：rest+hover=strong-stroke-default；active+disabled=strong-stroke-disabled。
            let stroke = match vs {
                VisualState::Pressed | VisualState::Disabled => t.strong_stroke_disabled,
                _ => t.strong_stroke_default,
            };
            ctx.painter.stroke_inner(b, CORNER, stroke, BORDER);
        }

        // 勾/横前景色：默认 text-on-accent-primary；active=text-on-accent-secondary；disabled=text-on-accent-disabled。
        let glyph = if !enabled {
            t.text_on_accent_disabled
        } else {
            match vs {
                VisualState::Pressed => t.text_on_accent_secondary,
                _ => t.text_on_accent_primary,
            }
        };

        match self.state {
            CheckState::Checked => {
                // dashoffset 揭示：TrimEnd 0→1，cubic-bezier(0.55,0,0,1)，保留 [0..te]。
                let p0 = (b.x + CHECK_P0.0, b.y + CHECK_P0.1);
                let p1 = (b.x + CHECK_P1.0, b.y + CHECK_P1.1);
                let p2 = (b.x + CHECK_P2.0, b.y + CHECK_P2.1);
                let full = [p0, p1, p2];
                let te = if self.anim_start < 0.0 {
                    1.0
                } else {
                    cubic_bezier(0.55, 0.0, 0.0, 1.0, self.anim_lin(ctx.now))
                };
                let pts = partial_polyline(&full, te);
                if pts.len() >= 2 {
                    ctx.painter.stroke_polyline(&pts, glyph, CHECK_STROKE);
                }
            }
            CheckState::Indeterminate => {
                // 居中胶囊 8×1（无过渡，瞬时）。
                let dash = Rect {
                    x: b.center_x() - DASH_W / 2.0,
                    y: b.center_y() - DASH_H / 2.0,
                    w: DASH_W,
                    h: DASH_H,
                };
                ctx.painter.fill_rounded_rect(dash, DASH_H / 2.0, glyph);
            }
            CheckState::Unchecked => {}
        }

        // 标签（.checkbox-container > span，左侧间距 8px，垂直居中；色 text-primary / 禁用 text-disabled）。
        if !self.label.is_empty() {
            let fg = if enabled { t.text_primary } else { t.text_disabled };
            let lr = Rect {
                x: b.right() + LABEL_GAP,
                y: self.rect.y,
                w: self.rect.w - BOX - LABEL_GAP,
                h: self.rect.h,
            };
            let _ = ctx.painter.draw_text_leading(&self.label, TextStyle::BODY, lr, fg);
        }

        // 焦点环（--fds-focus-stroke）：贴 20×20 框，内环 1px（spread 1）+ 外环带 2px（spread 1→3），瞬时。
        // box-shadow spread：外缘 = 框外扩 spread，圆角 = border-radius + spread。
        if self.interaction.focused && enabled {
            // 外环（focus-stroke-outer）：框外扩 3px 的 1px 描边落在 spread 边界（外缘 +3，内缘 +1）。
            let outer = Rect {
                x: b.x - FOCUS_OUTER_SPREAD,
                y: b.y - FOCUS_OUTER_SPREAD,
                w: b.w + 2.0 * FOCUS_OUTER_SPREAD,
                h: b.h + 2.0 * FOCUS_OUTER_SPREAD,
            };
            let outer_band = FOCUS_OUTER_SPREAD - FOCUS_INNER_SPREAD; // 2px
            ctx.painter.stroke_inner(outer, CORNER + FOCUS_OUTER_SPREAD, t.focus_stroke_outer, outer_band);
            // 内环（focus-stroke-inner）：框外扩 1px 的 1px 描边（spread 0→1）。
            let inner = Rect {
                x: b.x - FOCUS_INNER_SPREAD,
                y: b.y - FOCUS_INNER_SPREAD,
                w: b.w + 2.0 * FOCUS_INNER_SPREAD,
                h: b.h + 2.0 * FOCUS_INNER_SPREAD,
            };
            ctx.painter.stroke_inner(inner, CORNER + FOCUS_INNER_SPREAD, t.focus_stroke_inner, FOCUS_INNER_SPREAD);
        }
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
                if vk == 0x20 {
                    self.toggle(now);
                    toggled = true;
                }
            }
            _ => {}
        }
        let changed = toggled
            || before.visual_state() != self.interaction.visual_state()
            || before.focused != self.interaction.focused;
        EventResult { redraw: changed, animating: changed || toggled }
    }

    fn is_animating(&self, now: f64) -> bool {
        self.anim_start >= 0.0 && (now - self.anim_start) < DRAW_DUR
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::CheckBox
    }
    fn accessible_name(&self) -> String {
        self.label.clone()
    }
}

/// 取折线前 `frac` 长度比例的子折线（dashoffset 揭示）。
fn partial_polyline(pts: &[(f32, f32)], frac: f32) -> Vec<(f32, f32)> {
    if frac >= 1.0 || pts.len() < 2 {
        return pts.to_vec();
    }
    if frac <= 0.0 {
        return vec![pts[0]];
    }
    let mut seg = Vec::with_capacity(pts.len());
    let total: f32 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    let target = total * frac;
    let mut acc = 0.0f32;
    seg.push(pts[0]);
    for w in pts.windows(2) {
        let d = dist(w[0], w[1]);
        if acc + d >= target {
            let r = (target - acc) / d.max(0.0001);
            seg.push((w[0].0 + (w[1].0 - w[0].0) * r, w[0].1 + (w[1].1 - w[0].1) * r));
            break;
        }
        seg.push(w[1]);
        acc += d;
    }
    seg
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}
