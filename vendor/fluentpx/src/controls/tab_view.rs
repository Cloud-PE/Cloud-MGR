//! TabView（选择夹）：标签头 + 选中形状 + 切换时内容区进场动画。
//!
//! 真值来源：`controls/dev/TabView/TabView_themeresources.xaml`
//! * TabViewItemHeaderPadding `8,3,4,3`，MinWidth 100；选中背景 `SolidBackgroundFillColorTertiary`
//! * 未选中前景 `TextFillColorSecondary`，标签间分隔线 `DividerStrokeColorDefault`
//! * 切换标签时内容区做进场（Entrance：下移 + 淡入），与官方 TabView 内容转场一致

use crate::anim::ease_out;
use crate::typography::TextStyle;
use crate::widget::*;

const TAB_H: f32 = 32.0;
const TAB_MIN_W: f32 = 110.0;
const PAD_L: f32 = 8.0;
const CORNER_TOP: f32 = 7.0;
const CONTENT_H: f32 = 92.0;
const PAGE_DUR: f64 = 0.3;
const PAGE_OFFSET: f32 = 14.0;

pub struct TabView {
    pub tabs: Vec<String>,
    pub selected: usize,
    hovered: Option<usize>,
    rect: Rect,
    sel_start: f64,
}

impl TabView {
    pub fn new(tabs: Vec<String>, selected: usize) -> TabView {
        TabView { tabs, selected, hovered: None, rect: Rect::default(), sel_start: -1.0 }
    }

    fn tab_rect(&self, i: usize) -> Rect {
        Rect { x: self.rect.x + i as f32 * TAB_MIN_W, y: self.rect.y, w: TAB_MIN_W, h: TAB_H }
    }

    /// 命中仅限标签行（内容区不作为标签）。
    fn index_at(&self, p: Point) -> Option<usize> {
        if p.y < self.rect.y || p.y >= self.rect.y + TAB_H {
            return None;
        }
        let i = ((p.x - self.rect.x) / TAB_MIN_W).floor() as i32;
        if i >= 0 && (i as usize) < self.tabs.len() {
            Some(i as usize)
        } else {
            None
        }
    }
}

impl Widget for TabView {
    fn measure(&mut self, _available: Size) -> Size {
        Size { w: (TAB_MIN_W * self.tabs.len() as f32).max(360.0), h: TAB_H + CONTENT_H }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn hit_test(&self, p: Point) -> bool {
        self.rect.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        let t = ctx.tokens;

        // 内容区底板（与选中标签同色衔接）
        let content = Rect { x: self.rect.x, y: self.rect.y + TAB_H, w: self.rect.w, h: CONTENT_H };
        ctx.painter.fill_rounded_rect(content, 6.0, t.solid_bg_tertiary);

        for i in 0..self.tabs.len() {
            let r = self.tab_rect(i);
            let selected = i == self.selected;
            let hovered = self.hovered == Some(i);

            if selected {
                let shape = Rect { x: r.x, y: r.y, w: r.w, h: r.h + CORNER_TOP };
                ctx.painter.fill_rounded_rect(shape, CORNER_TOP, t.solid_bg_tertiary);
            } else if hovered {
                ctx.painter.fill_rounded_rect(r.inset(2.0), 4.0, t.subtle_fill_secondary);
            }

            if i + 1 < self.tabs.len() && !selected && i + 1 != self.selected {
                let x = r.right();
                let sep = Rect { x: x - 0.5, y: r.y + 8.0, w: 1.0, h: r.h - 16.0 };
                ctx.painter.fill_rect(sep, t.divider_stroke_default);
            }

            let fg = if selected { t.text_primary } else { t.text_secondary };
            let text_rect = Rect { x: r.x + PAD_L, y: r.y, w: r.w - PAD_L - 16.0, h: r.h };
            let _ = ctx.painter.draw_text_leading(&self.tabs[i], TextStyle::BODY, text_rect, fg);
        }

        // 内容区：切换时进场（下移 + 淡入），裁剪避免溢出。
        ctx.painter.push_clip(content);
        let pp = if self.sel_start < 0.0 {
            1.0
        } else {
            ease_out(((ctx.now - self.sel_start) / PAGE_DUR).clamp(0.0, 1.0) as f32)
        };
        let dy = (1.0 - pp) * PAGE_OFFSET;
        let title = format!("{} 的内容", self.tabs.get(self.selected).cloned().unwrap_or_default());
        let _ = ctx.painter.draw_text_leading(
            &title,
            TextStyle::SUBTITLE,
            Rect { x: content.x + 16.0, y: content.y + 18.0 + dy, w: content.w - 32.0, h: 30.0 },
            t.text_primary.with_opacity(pp),
        );
        let _ = ctx.painter.draw_text_leading(
            "切换上方标签时，此内容区做进场动画（下移 + 淡入）。",
            TextStyle::BODY,
            Rect { x: content.x + 16.0, y: content.y + 52.0 + dy, w: content.w - 32.0, h: 24.0 },
            t.text_secondary.with_opacity(pp),
        );
        ctx.painter.pop_clip();
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        let mut redraw = false;
        let mut animating = false;
        match ev {
            InputEvent::PointerMove(p) => {
                let h = self.index_at(p);
                if h != self.hovered {
                    self.hovered = h;
                    redraw = true;
                }
            }
            InputEvent::PointerLeave => {
                if self.hovered.is_some() {
                    self.hovered = None;
                    redraw = true;
                }
            }
            InputEvent::PointerDown(p) => {
                if let Some(i) = self.index_at(p) {
                    if i != self.selected {
                        self.selected = i;
                        self.sel_start = now;
                        redraw = true;
                        animating = true;
                    }
                }
            }
            _ => {}
        }
        EventResult { redraw, animating }
    }

    fn is_animating(&self, now: f64) -> bool {
        self.sel_start >= 0.0 && (now - self.sel_start) < PAGE_DUR
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Tab
    }
}
