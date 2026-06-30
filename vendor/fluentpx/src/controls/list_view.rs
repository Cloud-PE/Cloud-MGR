//! ListView / ListBox：卡片容器 + item 选中/悬停态 + 左侧选中指示条 + 动画。
//!
//! 真值来源：`controls/dev/CommonStyles/ListViewItem_themeresources.xaml`
//! * Item MinHeight 40，圆角 4（ControlCornerRadius）
//! * 背景：rest 透明；PointerOver `SubtleFillColorSecondary`；Selected `SubtleFillColorSecondary`；
//!   SelectedPointerOver `SubtleFillColorTertiary`；前景 `TextFillColorPrimary`
//! * 选中指示条：`AccentFillColorDefault`，CornerRadius 1.5，宽 3、高 16，左侧垂直居中
//!
//! 动画：① item 悬停背景淡入（~120ms）② 选中指示条切换时高度从 0 弹出（~250ms ease）。

use crate::anim::{ease_out, lerp};
use crate::color::Color;
use crate::typography::TextStyle;
use crate::widget::*;

const ITEM_H: f32 = 40.0;
const ITEM_CORNER: f32 = 4.0;
const INDICATOR_W: f32 = 3.0;
const INDICATOR_H: f32 = 16.0;
const INDICATOR_R: f32 = 1.5;
const CONTENT_LEFT: f32 = 16.0;
const PAD: f32 = 4.0; // 容器内边距
const HOVER_DUR: f64 = 0.12;
const SEL_DUR: f64 = 0.25;

pub struct ListView {
    pub items: Vec<String>,
    pub selected: Option<usize>,
    hovered: Option<usize>,
    pressed: Option<usize>,
    rect: Rect,
    pub enabled: bool,
    hover_start: f64,
    sel_start: f64,
}

impl ListView {
    pub fn new(items: Vec<String>, selected: Option<usize>) -> ListView {
        ListView {
            items,
            selected,
            hovered: None,
            pressed: None,
            rect: Rect::default(),
            enabled: true,
            hover_start: -1.0,
            sel_start: -1.0,
        }
    }

    fn item_rect(&self, i: usize) -> Rect {
        Rect {
            x: self.rect.x + PAD,
            y: self.rect.y + PAD + i as f32 * ITEM_H,
            w: self.rect.w - PAD * 2.0,
            h: ITEM_H,
        }
    }

    fn index_at(&self, p: Point) -> Option<usize> {
        if !self.rect.contains(p) {
            return None;
        }
        (0..self.items.len()).find(|&i| self.item_rect(i).contains(p))
    }
}

impl Widget for ListView {
    fn measure(&mut self, available: Size) -> Size {
        Size {
            w: available.w.max(320.0),
            h: ITEM_H * self.items.len() as f32 + PAD * 2.0,
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
        let now = ctx.now;

        // 容器卡片
        ctx.painter.fill_rounded_rect(self.rect, ITEM_CORNER + 1.0, t.card_bg_default);
        ctx.painter.stroke_inner(self.rect, ITEM_CORNER + 1.0, t.divider_stroke_default, 1.0);

        let hover_p = ((now - self.hover_start) / HOVER_DUR).clamp(0.0, 1.0) as f32;
        let sel_p = ease_out(((now - self.sel_start) / SEL_DUR).clamp(0.0, 1.0) as f32);

        for i in 0..self.items.len() {
            let r = self.item_rect(i);
            let selected = self.selected == Some(i);
            let hovered = self.hovered == Some(i);
            let pressed = self.pressed == Some(i);

            // 选中底色（即时），悬停底色（淡入）。
            let sel_bg = if selected {
                if pressed { t.subtle_fill_secondary } else if hovered { t.subtle_fill_tertiary } else { t.subtle_fill_secondary }
            } else {
                Color::TRANSPARENT
            };
            if sel_bg.a != 0 {
                ctx.painter.fill_rounded_rect(r, ITEM_CORNER, sel_bg);
            }
            // 悬停叠加（仅当前悬停项淡入）
            if hovered && !selected {
                let a = ease_out(hover_p);
                let hb = t.subtle_fill_secondary;
                ctx.painter.fill_rounded_rect(r, ITEM_CORNER, hb.with_opacity(a));
            }

            // 选中指示条（切换时高度弹出动画）
            if selected {
                let h = if self.sel_start >= 0.0 { lerp(INDICATOR_H * 0.3, INDICATOR_H, sel_p) } else { INDICATOR_H };
                let ind = Rect { x: r.x + 0.0, y: r.center_y() - h / 2.0, w: INDICATOR_W, h };
                ctx.painter.fill_rounded_rect(ind, INDICATOR_R, t.accent_fill_default());
            }

            let fg = if self.enabled { t.text_primary } else { t.text_disabled };
            let text_rect = Rect { x: r.x + CONTENT_LEFT, y: r.y, w: r.w - CONTENT_LEFT - 8.0, h: r.h };
            let _ = ctx.painter.draw_text_leading(&self.items[i], TextStyle::BODY, text_rect, fg);
        }
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if !self.enabled {
            return EventResult::NONE;
        }
        let mut redraw = false;
        let mut animating = false;
        match ev {
            InputEvent::PointerMove(p) => {
                let h = self.index_at(p);
                if h != self.hovered {
                    self.hovered = h;
                    self.hover_start = now;
                    redraw = true;
                    animating = true;
                }
            }
            InputEvent::PointerLeave => {
                if self.hovered.is_some() || self.pressed.is_some() {
                    self.hovered = None;
                    self.pressed = None;
                    redraw = true;
                }
            }
            InputEvent::PointerDown(p) => {
                self.pressed = self.index_at(p);
                redraw = self.pressed.is_some();
            }
            InputEvent::PointerUp(p) => {
                if let Some(i) = self.index_at(p) {
                    if self.pressed == Some(i) && self.selected != Some(i) {
                        self.selected = Some(i);
                        self.sel_start = now;
                        animating = true;
                    }
                    redraw = true;
                }
                self.pressed = None;
            }
            _ => {}
        }
        EventResult { redraw, animating }
    }

    fn is_animating(&self, now: f64) -> bool {
        (now - self.hover_start) < HOVER_DUR || (now - self.sel_start) < SEL_DUR
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::List
    }
}
