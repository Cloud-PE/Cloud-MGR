//! ContentDialog：Smoke 遮罩 + 模态卡片 + 主/次按钮。
//!
//! 真值来源：`controls/dev/CommonStyles/ContentDialog_themeresources.xaml`
//! * 遮罩 `SmokeFillColorDefault`(#4D000000 深色) 铺满全窗
//! * 卡片背景 `SolidBackgroundFillColorBase`，边框 `SurfaceStrokeColorDefault` 1px，
//!   圆角 8（OverlayCornerRadius），Padding 24，MinWidth 320 / MaxWidth 548，按钮间距 8
//! * 标题 Subtitle(20 SemiBold)，正文 Body(14)，底部主按钮=Accent、次按钮=Standard
//!
//! 模态焦点捕获：打开时由 gallery 把事件**仅**派发给本控件（见 main 的 dispatch）。
//! 触发器：本控件在主层画一个「Show ContentDialog」按钮，点击后打开。

use crate::anim::cubic_bezier;
use crate::typography::TextStyle;
use crate::widget::*;

// 源码真值：Scale 1.05→1.0 / ControlNormalAnimationDuration=250ms / FastOutSlowIn(0,0,0,1)；
// Opacity（含 smoke+对话框）0→1 线性 / ControlFasterAnimationDuration=83ms。
const SCALE_DUR: f64 = 0.25;
const FADE_DUR: f64 = 0.083;
const OPEN_DUR: f64 = 0.25;

const CORNER: f32 = 8.0;
const PAD: f32 = 24.0;
const BTN_H: f32 = 32.0;
const BTN_SPACING: f32 = 8.0;
const BORDER: f32 = 1.0;

pub struct ContentDialog {
    pub title: String,
    pub body: String,
    pub primary_text: String,
    pub close_text: String,
    pub open: bool,
    trigger: Rect,
    trigger_hover: bool,
    trigger_pressed: bool,
    // 模态按钮命中区（在 paint_overlay 中按视口计算并缓存，供事件使用）
    primary_rect: Rect,
    close_rect: Rect,
    hover_primary: bool,
    hover_close: bool,
    viewport: Size,
    open_start: f64,
}

impl ContentDialog {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> ContentDialog {
        ContentDialog {
            title: title.into(),
            body: body.into(),
            primary_text: "Save".into(),
            close_text: "Cancel".into(),
            open: false,
            trigger: Rect::default(),
            trigger_hover: false,
            trigger_pressed: false,
            primary_rect: Rect::default(),
            close_rect: Rect::default(),
            hover_primary: false,
            hover_close: false,
            viewport: Size { w: 800.0, h: 600.0 },
            open_start: 0.0,
        }
    }

    /// 依据视口计算居中卡片矩形。
    fn dialog_rect(&self, vp: Size) -> Rect {
        // MinWidth 320 / MaxWidth 548；取一个适中的固定宽度并夹在视口内。
        let w: f32 = (vp.w - 32.0).clamp(320.0, 548.0).min(420.0);
        let h: f32 = 200.0;
        Rect { x: (vp.w - w) / 2.0, y: (vp.h - h) / 2.0, w, h }
    }
}

impl Widget for ContentDialog {
    fn measure(&mut self, _available: Size) -> Size {
        Size { w: 200.0, h: BTN_H }
    }

    fn arrange(&mut self, rect: Rect) {
        self.trigger = Rect { x: rect.x, y: rect.y, w: rect.w.max(180.0).min(220.0), h: BTN_H };
    }

    fn hit_test(&self, p: Point) -> bool {
        self.trigger.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        // 触发按钮（Standard 外观）。
        let t = ctx.tokens;
        let r = self.trigger;
        let bg = if self.trigger_pressed {
            t.control_fill_tertiary
        } else if self.trigger_hover {
            t.control_fill_secondary
        } else {
            t.control_fill_default
        };
        ctx.painter.fill_rounded_rect(r.inset(BORDER), (4.0 - BORDER).max(0.0), bg);
        let _ = ctx.painter.stroke_inner_gradient(r, 4.0, &t.control_elevation_border(), BORDER);
        let _ = ctx.painter.draw_text_centered("Show ContentDialog", TextStyle::BODY, r, t.text_primary);
    }

    fn paint_overlay(&mut self, ctx: &mut PaintCtx) {
        if !self.open {
            return;
        }
        self.viewport = ctx.viewport;
        let t = ctx.tokens;
        let vp = ctx.viewport;

        // 入场动画（源码真值）：透明度 0→1 线性 83ms；缩放 1.05→1.0，250ms，FastOutSlowIn(0,0,0,1)。
        let alpha = (((ctx.now - self.open_start) / FADE_DUR).clamp(0.0, 1.0)) as f32;
        let scale_prog = cubic_bezier(0.0, 0.0, 0.0, 1.0, ((ctx.now - self.open_start) / SCALE_DUR).clamp(0.0, 1.0) as f32);

        // 全窗 smoke 遮罩（与对话框同步淡入）
        ctx.painter.fill_rect(Rect { x: 0.0, y: 0.0, w: vp.w, h: vp.h }, t.smoke_fill_default.with_opacity(alpha));

        // 事件命中用最终矩形；绘制用缩放矩形（围绕中心）。
        let d_final = self.dialog_rect(vp);
        let scale = 1.05 + (1.0 - 1.05) * scale_prog;
        let cx = d_final.center_x();
        let cy = d_final.center_y();
        let d = Rect {
            x: cx - d_final.w * scale / 2.0,
            y: cy - d_final.h * scale / 2.0,
            w: d_final.w * scale,
            h: d_final.h * scale,
        };
        // 卡片
        ctx.painter.fill_rounded_rect(d, CORNER, t.solid_bg_base.with_opacity(alpha));
        ctx.painter.stroke_inner(d, CORNER, t.surface_stroke_default.with_opacity(alpha), BORDER);

        // 标题 + 正文
        let title_rect = Rect { x: d.x + PAD, y: d.y + PAD, w: d.w - PAD * 2.0, h: 28.0 };
        let _ = ctx.painter.draw_text_leading(&self.title, TextStyle::SUBTITLE, title_rect, t.text_primary.with_opacity(alpha));
        let body_rect = Rect { x: d.x + PAD, y: title_rect.bottom() + 8.0, w: d.w - PAD * 2.0, h: 40.0 };
        let _ = ctx.painter.draw_text_leading(&self.body, TextStyle::BODY, body_rect, t.text_secondary.with_opacity(alpha));

        // 命中区用最终矩形（不随入场缩放抖动）；绘制用缩放后的 d（按钮随卡片一起缩放）。
        let hit_row_y = d_final.bottom() - PAD - BTN_H;
        let hit_total = d_final.w - PAD * 2.0;
        let hit_bw = (hit_total - BTN_SPACING) / 2.0;
        self.primary_rect = Rect { x: d_final.x + PAD, y: hit_row_y, w: hit_bw, h: BTN_H };
        self.close_rect = Rect { x: self.primary_rect.right() + BTN_SPACING, y: hit_row_y, w: hit_bw, h: BTN_H };

        let row_y = d.bottom() - PAD - BTN_H * scale;
        let total_w = d.w - PAD * 2.0 * scale;
        let bw = (total_w - BTN_SPACING * scale) / 2.0;
        let bh = BTN_H * scale;
        let primary_draw = Rect { x: d.x + PAD * scale, y: row_y, w: bw, h: bh };
        let close_draw = Rect { x: primary_draw.right() + BTN_SPACING * scale, y: row_y, w: bw, h: bh };

        // 主按钮（Accent）
        let pbg = if self.hover_primary { t.accent_fill_secondary() } else { t.accent_fill_default() };
        ctx.painter.fill_rounded_rect(primary_draw, 4.0, pbg.with_opacity(alpha));
        let _ = ctx.painter.draw_text_centered(&self.primary_text, TextStyle::BODY, primary_draw, t.text_on_accent_primary.with_opacity(alpha));

        // 次按钮（Standard）
        let cbg = if self.hover_close { t.control_fill_secondary } else { t.control_fill_default };
        ctx.painter.fill_rounded_rect(close_draw.inset(BORDER), 3.0, cbg.with_opacity(alpha));
        ctx.painter.stroke_inner(close_draw, 4.0, t.stroke_default.with_opacity(alpha), 1.0);
        let _ = ctx.painter.draw_text_centered(&self.close_text, TextStyle::BODY, close_draw, t.text_primary.with_opacity(alpha));
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        let mut redraw = false;
        if self.open {
            // 模态：仅处理两个按钮的悬停/点击；点遮罩不关闭（与 WinUI 一致）。
            match ev {
                InputEvent::PointerMove(p) => {
                    let hp = self.primary_rect.contains(p);
                    let hc = self.close_rect.contains(p);
                    if hp != self.hover_primary || hc != self.hover_close {
                        self.hover_primary = hp;
                        self.hover_close = hc;
                        redraw = true;
                    }
                }
                InputEvent::PointerUp(p) => {
                    if self.primary_rect.contains(p) || self.close_rect.contains(p) {
                        self.open = false;
                        self.hover_primary = false;
                        self.hover_close = false;
                        redraw = true;
                    }
                }
                InputEvent::KeyDown(vk) => {
                    if vk == 0x1B {
                        // Esc 关闭
                        self.open = false;
                        redraw = true;
                    }
                }
                _ => {}
            }
            return EventResult { redraw, animating: false };
        }

        // 关闭态：触发按钮交互
        match ev {
            InputEvent::PointerMove(p) => {
                let h = self.trigger.contains(p);
                if h != self.trigger_hover {
                    self.trigger_hover = h;
                    redraw = true;
                }
            }
            InputEvent::PointerLeave => {
                self.trigger_hover = false;
                self.trigger_pressed = false;
                redraw = true;
            }
            InputEvent::PointerDown(p) => {
                if self.trigger.contains(p) {
                    self.trigger_pressed = true;
                    redraw = true;
                }
            }
            InputEvent::PointerUp(p) => {
                if self.trigger_pressed && self.trigger.contains(p) {
                    self.open = true;
                    self.open_start = now;
                }
                self.trigger_pressed = false;
                redraw = true;
            }
            _ => {}
        }
        EventResult { redraw, animating: false }
    }

    fn is_animating(&self, now: f64) -> bool {
        self.open && (now - self.open_start) < OPEN_DUR
    }

    fn wants_modal(&self) -> bool {
        self.open
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Dialog
    }
}
