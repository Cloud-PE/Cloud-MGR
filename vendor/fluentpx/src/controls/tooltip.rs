//! ToolTip：悬停延迟弹出 + 圆角 + 边框（阴影/Acrylic 近似）。
//!
//! 真值来源：`controls/dev/CommonStyles/ToolTip_themeresources.xaml`
//! * Padding `9,6,9,8`，CornerRadius 4（ControlCornerRadius），Border 1px，MaxWidth 320
//! * 背景 `SystemControlBackgroundChromeMediumLowBrush`（系统刷，acrylic）——
//!   此处用 `SolidBackgroundFillColorTertiary` 纯色近似，**待办：DirectComposition 实时模糊**
//! * 边框 `SurfaceStrokeColorFlyout`，前景 `TextFillColorPrimary`
//!
//! 默认弹出延迟 ~1s。弹层经 [`Widget::paint_overlay`] 置顶绘制。

use crate::typography::TextStyle;
use crate::widget::*;

const DELAY: f64 = 1.0;
const PAD_L: f32 = 9.0;
const PAD_T: f32 = 6.0;
const PAD_R: f32 = 9.0;
const PAD_B: f32 = 8.0;
const CORNER: f32 = 4.0;
const BORDER: f32 = 1.0;
const GAP: f32 = 8.0; // 气泡与锚点的间距

pub struct ToolTip {
    pub tip: String,
    pub anchor_text: String,
    rect: Rect, // 锚点区域
    hovered: bool,
    hover_start: f64,
}

impl ToolTip {
    pub fn new(anchor_text: impl Into<String>, tip: impl Into<String>) -> ToolTip {
        ToolTip {
            tip: tip.into(),
            anchor_text: anchor_text.into(),
            rect: Rect::default(),
            hovered: false,
            hover_start: 0.0,
        }
    }

    fn is_shown(&self, now: f64) -> bool {
        self.hovered && (now - self.hover_start) >= DELAY
    }
}

impl Widget for ToolTip {
    fn measure(&mut self, _available: Size) -> Size {
        Size { w: 160.0, h: 32.0 }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn hit_test(&self, p: Point) -> bool {
        self.rect.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        // 锚点：一个虚框 + 提示文字，表示「把鼠标悬停在这里」。
        let t = ctx.tokens;
        ctx.painter.fill_rounded_rect(self.rect, CORNER, t.control_fill_default);
        ctx.painter.stroke_inner(self.rect, CORNER, t.stroke_default, 1.0);
        let _ = ctx.painter.draw_text_centered(&self.anchor_text, TextStyle::BODY, self.rect, t.text_primary);
    }

    fn paint_overlay(&mut self, ctx: &mut PaintCtx) {
        if !self.is_shown(ctx.now) {
            return;
        }
        let t = ctx.tokens;
        // 淡入（PopupThemeTransition 近似）：显示后 ~150ms 内不透明度 0→1。
        let shown_for = ctx.now - (self.hover_start + DELAY);
        let alpha = (shown_for / 0.15).clamp(0.0, 1.0) as f32;
        // 测量气泡内容尺寸
        let text_size = ctx
            .painter
            .measure_text(&self.tip, TextStyle::BODY)
            .unwrap_or(Size { w: 80.0, h: 20.0 });
        let w = (text_size.w + PAD_L + PAD_R).min(320.0);
        let h = text_size.h + PAD_T + PAD_B;
        // 置于锚点上方居中
        let x = self.rect.center_x() - w / 2.0;
        let y = self.rect.y - GAP - h;
        let bubble = Rect { x, y, w, h };

        // 简易阴影（半透明黑下移 2px 的圆角块），待办：真实投影/模糊。
        let shadow = Rect { x: bubble.x, y: bubble.y + 2.0, w: bubble.w, h: bubble.h };
        ctx.painter.fill_rounded_rect(shadow, CORNER, crate::color::Color::hex("#30000000").with_opacity(alpha));

        ctx.painter.fill_rounded_rect(bubble, CORNER, t.solid_bg_tertiary.with_opacity(alpha));
        ctx.painter.stroke_inner(bubble, CORNER, t.surface_stroke_flyout.with_opacity(alpha), BORDER);
        let text_rect = Rect { x: bubble.x + PAD_L, y: bubble.y, w: bubble.w - PAD_L - PAD_R, h: bubble.h };
        let _ = ctx.painter.draw_text_leading(&self.tip, TextStyle::BODY, text_rect, t.text_primary.with_opacity(alpha));
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        match ev {
            InputEvent::PointerMove(p) => {
                let inside = self.rect.contains(p);
                if inside && !self.hovered {
                    self.hovered = true;
                    self.hover_start = now;
                    return EventResult { redraw: true, animating: true };
                } else if !inside && self.hovered {
                    self.hovered = false;
                    return EventResult::REDRAW;
                }
            }
            InputEvent::PointerLeave => {
                if self.hovered {
                    self.hovered = false;
                    return EventResult::REDRAW;
                }
            }
            _ => {}
        }
        EventResult::NONE
    }

    fn is_animating(&self, now: f64) -> bool {
        // 悬停后到「显示 + 淡入完成」期间持续刷新（触发延迟弹出 + 淡入）。
        self.hovered && (now - self.hover_start) < DELAY + 0.2
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::ToolTip
    }
}
