//! Menu（菜单 / MenuFlyout）：按钮触发，弹出菜单列表，含开合动画。
//!
//! 真值参考：`controls/dev/MenuFlyout/*` 与 CommonStyles。
//! * 弹层圆角 `OverlayCornerRadius 8`，背景 acrylic（此处纯色近似，**待办：实时模糊**）
//! * MenuFlyoutItem 高 ~32，悬停 `SubtleFillColorSecondary`，圆角 4，内边距左 ~12
//! * 打开动画：透明度 0→1 + 纵向位移 -8→0 + 轻微缩放，~150ms ease（仿官方弹出）。

use crate::anim::ease_out;
use crate::color::Color;
use crate::gfx::Icon;
use crate::typography::TextStyle;
use crate::widget::*;

const BTN_H: f32 = 32.0;
const CORNER: f32 = 4.0;
const BORDER: f32 = 1.0;
const POPUP_CORNER: f32 = 8.0;
const ITEM_H: f32 = 36.0; // MenuFlyoutItemThemePadding 11,8,11,9 → 20+16≈36
const POPUP_VPAD: f32 = 2.0; // MenuFlyoutPresenterThemePadding 0,2,0,2
const ITEM_PAD_L: f32 = 11.0;
const POPUP_W: f32 = 200.0;
const OPEN_DUR: f64 = 0.15;

pub struct Menu {
    pub title: String,
    pub items: Vec<String>,
    pub open: bool,
    pub last_selected: Option<usize>,
    hovered_btn: bool,
    pressed_btn: bool,
    hovered_item: Option<usize>,
    rect: Rect,
    open_start: f64,
}

impl Menu {
    pub fn new(title: impl Into<String>, items: Vec<String>) -> Menu {
        Menu {
            title: title.into(),
            items,
            open: false,
            last_selected: None,
            hovered_btn: false,
            pressed_btn: false,
            hovered_item: None,
            rect: Rect::default(),
            open_start: 0.0,
        }
    }

    fn popup_rect(&self) -> Rect {
        let h = self.items.len() as f32 * ITEM_H + POPUP_VPAD * 2.0;
        Rect { x: self.rect.x, y: self.rect.bottom() + 4.0, w: POPUP_W, h }
    }

    fn popup_item_rect(&self, i: usize) -> Rect {
        let p = self.popup_rect();
        Rect { x: p.x, y: p.y + POPUP_VPAD + i as f32 * ITEM_H, w: p.w, h: ITEM_H }
    }

    fn item_at(&self, pt: Point) -> Option<usize> {
        if !self.popup_rect().contains(pt) {
            return None;
        }
        (0..self.items.len()).find(|&i| self.popup_item_rect(i).contains(pt))
    }
}

impl Widget for Menu {
    fn measure(&mut self, _available: Size) -> Size {
        Size { w: 160.0, h: BTN_H }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = Rect { x: rect.x, y: rect.y, w: rect.w.max(140.0).min(180.0), h: BTN_H };
    }

    fn hit_test(&self, p: Point) -> bool {
        self.rect.contains(p) || (self.open && self.popup_rect().contains(p))
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        let t = ctx.tokens;
        let r = self.rect;
        let bg = if self.pressed_btn || self.open {
            t.control_fill_tertiary
        } else if self.hovered_btn {
            t.control_fill_secondary
        } else {
            t.control_fill_default
        };
        ctx.painter.fill_rounded_rect(r.inset(BORDER), (CORNER - BORDER).max(0.0), bg);
        let _ = ctx.painter.stroke_inner_gradient(r, CORNER, &t.control_elevation_border(), BORDER);
        // 标题 + ▾
        let label = match self.last_selected {
            Some(i) => format!("{}：{}", self.title, self.items[i]),
            None => self.title.clone(),
        };
        let tr = Rect { x: r.x + 12.0, y: r.y, w: r.w - 32.0, h: r.h };
        let _ = ctx.painter.draw_text_leading(&label, TextStyle::BODY, tr, t.text_primary);
        let gly = Rect { x: r.right() - 24.0, y: r.center_y() - 6.0, w: 12.0, h: 12.0 };
        ctx.painter.draw_glyph(Icon::ChevronDown, gly, t.text_secondary);
    }

    fn paint_overlay(&mut self, ctx: &mut PaintCtx) {
        if !self.open {
            return;
        }
        let t = ctx.tokens;
        // 开合动画：纵向裁剪揭示（从顶向下「展开」）+ 轻微淡入。
        let prog = ease_out(((ctx.now - self.open_start) / OPEN_DUR).clamp(0.0, 1.0) as f32);
        let alpha = (prog * 1.6).min(1.0);
        let p = self.popup_rect();
        let reveal_h = (p.h * prog).max(1.0);
        ctx.painter.push_clip(Rect { x: p.x - 4.0, y: p.y, w: p.w + 8.0, h: reveal_h });

        ctx.painter.fill_rounded_rect(Rect { y: p.y + 2.0, ..p }, POPUP_CORNER, Color::hex("#30000000").with_opacity(alpha));
        ctx.painter.fill_rounded_rect(p, POPUP_CORNER, t.solid_bg_tertiary.with_opacity(alpha));
        ctx.painter.stroke_inner(p, POPUP_CORNER, t.surface_stroke_flyout.with_opacity(alpha), 1.0);

        for i in 0..self.items.len() {
            let ir = self.popup_item_rect(i);
            if self.hovered_item == Some(i) {
                ctx.painter.fill_rounded_rect(
                    Rect { x: ir.x + 4.0, y: ir.y + 1.0, w: ir.w - 8.0, h: ir.h - 2.0 },
                    CORNER,
                    t.subtle_fill_secondary.with_opacity(alpha),
                );
            }
            let text_rect = Rect { x: ir.x + ITEM_PAD_L, y: ir.y, w: ir.w - ITEM_PAD_L - 8.0, h: ir.h };
            let _ = ctx.painter.draw_text_leading(&self.items[i], TextStyle::BODY, text_rect, t.text_primary.with_opacity(alpha));
        }
        ctx.painter.pop_clip();
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        let mut redraw = false;
        match ev {
            InputEvent::PointerMove(pt) => {
                let hb = self.rect.contains(pt);
                if hb != self.hovered_btn {
                    self.hovered_btn = hb;
                    redraw = true;
                }
                if self.open {
                    let hi = self.item_at(pt);
                    if hi != self.hovered_item {
                        self.hovered_item = hi;
                        redraw = true;
                    }
                }
            }
            InputEvent::PointerLeave => {
                self.hovered_btn = false;
                self.hovered_item = None;
                redraw = true;
            }
            InputEvent::PointerDown(pt) => {
                if self.rect.contains(pt) {
                    self.pressed_btn = true;
                    redraw = true;
                } else if self.open && !self.popup_rect().contains(pt) {
                    self.open = false;
                    redraw = true;
                }
            }
            InputEvent::PointerUp(pt) => {
                if self.pressed_btn && self.rect.contains(pt) {
                    self.open = !self.open;
                    if self.open {
                        self.open_start = now;
                    }
                    redraw = true;
                } else if self.open {
                    if let Some(i) = self.item_at(pt) {
                        self.last_selected = Some(i);
                        self.open = false;
                        redraw = true;
                    }
                }
                self.pressed_btn = false;
            }
            _ => {}
        }
        EventResult { redraw, animating: self.open }
    }

    fn is_animating(&self, now: f64) -> bool {
        self.open && (now - self.open_start) < OPEN_DUR
    }

    fn wants_modal(&self) -> bool {
        self.open
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::List
    }
}
