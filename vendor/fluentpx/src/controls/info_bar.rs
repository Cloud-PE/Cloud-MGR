//! InfoBar（通知条）：Information / Success / Caution / Critical / Attention 五态。
//!
//! 真值来源：fluent-svelte `src/lib/InfoBar/InfoBar.scss`（123行）+ `InfoBadge/InfoBadge.scss`
//! + `theme.css`（severity token 深/浅两套）。结构（几何/边框/图标/关闭按钮/过渡）严格对齐 fluent-svelte，
//! 与旧版 `infobar.rs` 的差别仅在于枚举变体名称与 Attention 语义。
//!
//! 关键几何（InfoBar.scss）：
//! * `.info-bar`  min-block-size:48px；padding-inline-start:15px；border 1px `card-stroke-default`；
//!   border-radius `control-corner-radius`(4)；background-clip:padding-box。
//! * severity 背景：information=`card-background-secondary`，attention=同；
//!   success/caution/critical=`system-background-*`（InfoBar.scss:14-30）。
//! * `.info-bar-icon` align-self:flex-start + margin-block-start:16px（顶部对齐，距顶 16）。
//!   图标是 InfoBadge：min 16×16、border-radius:16（药丸/圆）、padding 2px 4px（取最小尺寸）、
//!   bg=语义色、内含 8×8 svg 字形（`text-on-accent-primary`）。（InfoBadge.scss:7-10,34-37）
//! * `.info-bar-content` margin-inline-start:13px、block 上下各 7px；
//!   若 message-wrapped 则 block-start:13px / block-end:15px。（InfoBar.scss:40-68）
//! * h5 = Body weight600（BODY_STRONG），margin-inline-end:12px，color `text-primary`。（InfoBar.scss:80-81）
//! * p  = Body weight400（BODY），margin-inline-end:15px，color `text-primary`。（InfoBar.scss:74-77,83-85）
//! * `.info-bar-close-button` 38×38、margin 4px、align-self:flex-start、border-radius:4、
//!   transparent → hover `subtle-fill-secondary` → active `subtle-fill-tertiary`+`text-secondary`，
//!   transition fast(167ms) cubic-bezier(0,0,0,1)，svg 12×12。（InfoBar.scss:92-121）

use crate::anim::ColorTransition;
use crate::color::Color;
use crate::gfx::Icon;
use crate::tokens::Tokens;
use crate::typography::TextStyle;
use crate::widget::*;

// —— InfoBar.scss 像素真值 ——
const MIN_H: f32 = 48.0;               // InfoBar.scss:6  min-block-size: 48px
const CORNER: f32 = 4.0;               // InfoBar.scss:13 border-radius: var(--control-corner-radius)=4
const BORDER: f32 = 1.0;               // InfoBar.scss:12 border: 1px solid var(--card-stroke-default)
const PAD_START: f32 = 15.0;           // InfoBar.scss:7  padding-inline-start: 15px
const CONTENT_MARGIN_START: f32 = 13.0;// InfoBar.scss:46 .info-bar-content margin-inline-start: 13px
const CONTENT_MARGIN_BLOCK: f32 = 7.0; // InfoBar.scss:44 .info-bar-content margin-block-start/end: 7px
const LINE_HEIGHT: f32 = 20.0;         // InfoBar.scss:71 h5/p line-height: 20px (body-font-size=14)
#[allow(dead_code)]
const TITLE_MARGIN_END: f32 = 12.0;    // InfoBar.scss:81 h5 margin-inline-end: 12px（spec 参考值，实际由 text_w 统一约束）
const MSG_MARGIN_END: f32 = 15.0;      // InfoBar.scss:84 p margin-inline-end: 15px

// InfoBadge.scss
const BADGE_W: f32 = 16.0;     // InfoBadge.scss:7  min-inline-size: 16px
const BADGE_H: f32 = 16.0;     // InfoBadge.scss:8  min-block-size:  16px
const BADGE_RADIUS: f32 = 8.0; // InfoBadge.scss:9  border-radius:16 → 角半径 = h/2 = 8

// .info-bar-close-button (InfoBar.scss:92-121)
const CLOSE_SIZE: f32 = 38.0;    // InfoBar.scss:99-100 inline/block-size: 38px
const CLOSE_MARGIN: f32 = 4.0;   // InfoBar.scss:101    margin: 4px (top + right)
const CLOSE_RADIUS: f32 = 4.0;   // InfoBar.scss:103    border-radius: var(--control-corner-radius)
const CLOSE_GLYPH: f32 = 12.0;   // InfoBar.scss:116-120 svg 12×12

/// `--fds-control-fast-duration` = 167ms（关闭按钮背景过渡 cubic-bezier(0,0,0,1)）。
/// （InfoBar.scss:105; theme.css:38,43）
const FAST_DUR: f64 = 0.167;

/// InfoBar 严重程度枚举（对应 fluent-svelte Severity prop，5 个变体）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    /// information（默认/中性）— card-background-secondary 背景；灰色 Info 徽章。
    Information,
    /// success — system-background-success 背景；绿色 Completed 徽章。
    Success,
    /// caution — system-background-caution 背景；黄色 Warning 徽章。
    Caution,
    /// critical/error — system-background-critical 背景；红色 ErrorBadge 徽章。
    Critical,
    /// attention — 与 information 同色（card-background-secondary）。（InfoBar.scss:14-30）
    Attention,
}

/// InfoBar 通知条控件。
///
/// 布局（左→右）：
/// 1. severity 图标徽章（InfoBadge，16×16 药丸，距顶 16px，距左 15px）
/// 2. 内容区（标题粗体 + 正文换行，内边距 13px 开始，上下各 7px）
/// 3. （可选）操作按钮区（目前仅预留宽度；可在调用方叠加子控件）
/// 4. （可选）关闭按钮（38×38，右上角 margin 4px）
pub struct InfoBar {
    pub severity: Severity,
    pub title: String,
    pub message: String,
    /// 是否已由用户关闭（关闭后 measure→0×0，paint/hit_test 均跳过）。
    pub closed: bool,
    /// 是否显示关闭按钮（对应 InfoBar.svelte `closable` prop，默认 true）。
    pub closable: bool,
    /// 可选操作按钮标签（Some → 在文字右侧保留按钮宽度并绘制）。
    pub action_label: Option<String>,

    // 关闭按钮交互状态
    hover_close: bool,
    press_close: bool,
    /// 关闭按钮背景颜色过渡（transparent ↔ subtle-fill-*），fast 167ms。
    close_bg: ColorTransition,
    close_bg_init: bool,

    rect: Rect,
}

impl InfoBar {
    /// 创建一个 InfoBar，显示关闭按钮、无操作按钮。
    pub fn new(severity: Severity, title: impl Into<String>, message: impl Into<String>) -> InfoBar {
        InfoBar {
            severity,
            title: title.into(),
            message: message.into(),
            closed: false,
            closable: true,
            action_label: None,
            hover_close: false,
            press_close: false,
            close_bg: ColorTransition::instant(Color::TRANSPARENT),
            close_bg_init: false,
            rect: Rect::default(),
        }
    }

    /// 创建带操作按钮标签的 InfoBar。
    pub fn with_action(
        severity: Severity,
        title: impl Into<String>,
        message: impl Into<String>,
        action: impl Into<String>,
    ) -> InfoBar {
        InfoBar {
            action_label: Some(action.into()),
            ..InfoBar::new(severity, title, message)
        }
    }

    // ——————————————————— 颜色计算 ———————————————————

    /// `.info-bar.severity-*` 容器背景（InfoBar.scss:14-30）。
    ///  - information / attention → card-background-secondary（半透明，需 over 页面底色）
    ///  - success/caution/critical → system-background-* token（tokens.rs 深/浅两套）
    fn container_bg(&self, t: &Tokens) -> Color {
        match self.severity {
            Severity::Information | Severity::Attention => t.card_bg_secondary,
            Severity::Success => t.system_success_bg,
            Severity::Caution => t.system_caution_bg,
            Severity::Critical => t.system_critical_bg,
        }
    }

    /// InfoBadge 药丸背景色（InfoBadge.scss:17-33 + theme.css `--fds-system-*`）。
    ///  - information → system-solid-neutral（深 #9E9E9E / 浅 #8A8A8A，无 token，内联）
    ///  - attention   → system-attention 蓝（深 #61CDFF / 浅 #005FB8）（InfoBadge.scss:18-19；theme.css:194,370）
    ///  - success  → system_success 前景色（深 #6CCB5F / 浅 #0F7B0F）
    ///  - caution  → system_caution 前景色（深 #FCE100 / 浅 #9D5D00）
    ///  - critical → system_critical 前景色（深 #FF99A4 / 浅 #C42B1C）
    fn badge_bg(&self, t: &Tokens) -> Color {
        match self.severity {
            Severity::Information => {
                // `--fds-system-solid-neutral`：theme.css dark=#9E9E9E / light=#8A8A8A
                if t.is_dark { Color::hex("#9E9E9E") } else { Color::hex("#8A8A8A") }
            }
            Severity::Attention => {
                // `--fds-system-attention`：theme.css dark=hsl(199,100%,69%) / light=hsl(209,100%,36%)
                if t.is_dark { Color::hex("#61CDFF") } else { Color::hex("#005FB8") }
            }
            Severity::Success  => t.system_success,
            Severity::Caution  => t.system_caution,
            Severity::Critical => t.system_critical,
        }
    }

    /// InfoBadge 内字形码位（Segoe Fluent Icons / MDL2 Assets）。
    ///  - information / attention → Info ()
    ///  - success  → Completed ()
    ///  - caution  → Warning   ()
    ///  - critical → ErrorBadge()
    /// 在 16×16 药丸上画**纯符号**（白色矢量；圆圈由药丸背景提供，符号本身**不含圈**）。
    /// 对齐网页 InfoBadge：实心彩色圆 + 白色简单符号（i / ✓ / ! / ✕）。不再用 Segoe 的
    /// Info/Completed/ErrorBadge 字形——那些自带一圈，叠在背景圆上会出现「双层圈/一圈黑边」
    /// （用户反馈错误图标外面有一圈黑圈）。
    fn draw_badge_symbol(&self, ctx: &mut PaintCtx, b: Rect) {
        // 符号颜色对齐官方 InfoBadge SVG：深色主题（彩圆偏亮）用**黑**，浅色主题（彩圆偏深）用**白**。
        let col = if ctx.tokens.is_dark { Color::hex("#000000") } else { Color::hex("#FFFFFF") };
        let cx = b.x + b.w / 2.0;
        let cy = b.y + b.h / 2.0;
        let s = 3.0; // 符号半幅 ≈6px，居中于 16px 药丸
        let lw = 1.5; // 线宽（逻辑像素）
        let dot = 1.0; // 圆点半径
        match self.severity {
            Severity::Critical => {
                // ✕
                ctx.painter.stroke_polyline(&[(cx - s, cy - s), (cx + s, cy + s)], col, lw);
                ctx.painter.stroke_polyline(&[(cx + s, cy - s), (cx - s, cy + s)], col, lw);
            }
            Severity::Success => {
                // ✓
                ctx.painter.stroke_polyline(
                    &[(cx - s, cy + 0.2), (cx - s * 0.25, cy + s * 0.85), (cx + s, cy - s * 0.75)],
                    col,
                    lw,
                );
            }
            Severity::Caution => {
                // !（竖线 + 下方圆点）
                ctx.painter.stroke_polyline(&[(cx, cy - s), (cx, cy + s * 0.35)], col, lw);
                ctx.painter.fill_circle(cx, cy + s * 0.95, dot, col);
            }
            Severity::Information | Severity::Attention => {
                // i（上方圆点 + 竖线）
                ctx.painter.fill_circle(cx, cy - s * 0.85, dot, col);
                ctx.painter.stroke_polyline(&[(cx, cy - s * 0.15), (cx, cy + s)], col, lw);
            }
        }
    }

    /// 关闭按钮背景目标（InfoBar.scss:104,110,114）：
    ///   active → subtle-fill-tertiary；hover → subtle-fill-secondary；rest → transparent。
    fn close_bg_target(&self, t: &Tokens) -> Color {
        if self.press_close {
            t.subtle_fill_tertiary
        } else if self.hover_close {
            t.subtle_fill_secondary
        } else {
            Color::TRANSPARENT
        }
    }

    /// 关闭按钮前景（InfoBar.scss:102,113）：active → text-secondary；rest → text-primary。
    fn close_fg(&self, t: &Tokens) -> Color {
        if self.press_close { t.text_secondary } else { t.text_primary }
    }

    // ——————————————————— 矩形计算 ———————————————————

    /// `.info-bar-icon` 药丸矩形：左缘 = padding-inline-start(15)，顶边 = margin-block-start(16)。
    /// 内容区左缘 = padding-inline-start + 药丸宽 + content margin-inline-start（= +44 from left）。
    fn content_left(&self) -> f32 {
        self.rect.x + PAD_START + BADGE_W + CONTENT_MARGIN_START
    }

    /// 关闭按钮矩形：38×38，右上角内缩 margin(4)，align-self:flex-start。（InfoBar.scss:94-101）
    fn close_rect(&self) -> Rect {
        Rect {
            x: self.rect.right() - CLOSE_MARGIN - CLOSE_SIZE,
            y: self.rect.y + CLOSE_MARGIN,
            w: CLOSE_SIZE,
            h: CLOSE_SIZE,
        }
    }

    /// 文字区可用右边界（到关闭按钮左缘 / 或容器右缘）。
    fn text_right_limit(&self) -> f32 {
        if self.closable { self.close_rect().x } else { self.rect.right() }
    }
}

impl Widget for InfoBar {
    fn measure(&mut self, available: Size) -> Size {
        if self.closed {
            return Size { w: 0.0, h: 0.0 };
        }
        // 高度：min-block-size:48；含标题+正文两行时撑高到上下各 7px margin + 2×20 行高 = 54。
        // 单行内容 ≤ 20+7+7=34 < 48 → 取 MIN_H。两行 = 14+40 = 54 > 48。
        let h = if !self.title.is_empty() && !self.message.is_empty() {
            MIN_H.max(CONTENT_MARGIN_BLOCK * 2.0 + LINE_HEIGHT * 2.0) // = 54
        } else {
            MIN_H
        };
        Size { w: available.w.max(360.0), h }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn hit_test(&self, p: Point) -> bool {
        !self.closed && self.rect.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        if self.closed {
            return;
        }
        let t = ctx.tokens;
        let r = self.rect;

        // 首帧初始化关闭按钮背景（避免闪烁）。
        if !self.close_bg_init {
            self.close_bg = ColorTransition::instant(self.close_bg_target(t));
            self.close_bg_init = true;
        }

        // —— 背景（InfoBar.scss:10-13）——
        // severity 背景可为半透明（information/attention = card-background-secondary 带 alpha），
        // 先与 solid_bg_base（页面底色）做 over 得到实色，再填入圆角矩形。
        // background-clip:padding-box → 背景只铺到边框内沿。
        let page = t.solid_bg_base;
        let bg = self.container_bg(t).over(page);
        ctx.painter.fill_rounded_rect(r.inset(BORDER), (CORNER - BORDER).max(0.0), bg);

        // —— 边框（InfoBar.scss:12 border:1px solid var(--card-stroke-default)）——
        // card-stroke-default 半透明，over(page) 得实色。
        let border = t.card_stroke_default.over(page);
        ctx.painter.stroke_inner(r, CORNER, border, BORDER);

        // —— 文字布局：先算第一行（标题/正文）顶边 y，图标据此与标题行对齐 ——
        let left = self.content_left();
        let right_limit = self.text_right_limit();
        let text_w = (right_limit - left - MSG_MARGIN_END).max(0.0);
        let has_title = !self.title.is_empty();
        let has_msg = !self.message.is_empty();
        // 两行：第一行在上半行；单行：整条居中。
        let first_line_y = if has_title && has_msg {
            r.center_y() - LINE_HEIGHT
        } else {
            r.center_y() - LINE_HEIGHT / 2.0
        };

        // —— InfoBadge 图标：与**标题行**垂直居中对齐（距左 PAD_START、尺寸 16×16 不变）——
        // +2：CJK 字形在行盒内偏下，图标纯几何居中会显得偏上，下移一点对齐文字光学中线。
        let badge = Rect {
            x: r.x + PAD_START,
            y: first_line_y + (LINE_HEIGHT - BADGE_H) / 2.0 + 2.0,
            w: BADGE_W,
            h: BADGE_H,
        };
        ctx.painter.fill_rounded_rect(badge, BADGE_RADIUS, self.badge_bg(t));
        self.draw_badge_symbol(ctx, badge);

        if has_title && has_msg {
            // 两行内容：内容块高 = 2×LINE_HEIGHT(40)，竖直居中于条内。
            // 竖直居中：block_top = center_y - LINE_HEIGHT（一行往上偏半行）。
            let block_top = r.center_y() - LINE_HEIGHT;
            // h5（标题）：BODY_STRONG(600)，margin-inline-end:12 包含在 text_w 右边距中。
            // （InfoBar.scss:80-81 font-weight:600 + margin-inline-end:12px）
            let title_rect = Rect { x: left, y: block_top, w: text_w, h: LINE_HEIGHT };
            let _ = ctx.painter.draw_text_leading(
                &self.title,
                TextStyle::BODY_STRONG,
                title_rect,
                t.text_primary,
            );
            // p（正文）：BODY(400)，flex:1 1 auto，可换行。（InfoBar.scss:74-77,83-85）
            let msg_rect = Rect { x: left, y: block_top + LINE_HEIGHT, w: text_w, h: LINE_HEIGHT };
            let _ = ctx.painter.draw_text_wrapped(
                &self.message,
                TextStyle::BODY,
                msg_rect,
                t.text_primary,
            );
        } else if has_title {
            // 仅标题：单行竖直居中。
            let title_rect = Rect {
                x: left,
                y: r.center_y() - LINE_HEIGHT / 2.0,
                w: text_w,
                h: LINE_HEIGHT,
            };
            let _ = ctx.painter.draw_text_leading(
                &self.title,
                TextStyle::BODY_STRONG,
                title_rect,
                t.text_primary,
            );
        } else if has_msg {
            // 仅正文：单行竖直居中（无标题时正文居中于整条）。
            let msg_rect = Rect {
                x: left,
                y: r.center_y() - LINE_HEIGHT / 2.0,
                w: text_w,
                h: LINE_HEIGHT,
            };
            let _ = ctx.painter.draw_text_wrapped_centered(
                &self.message,
                TextStyle::BODY,
                msg_rect,
                t.text_primary,
            );
        }

        // —— 关闭按钮（InfoBar.scss:92-121）——
        // 38×38，右上角 margin 4px，border-radius 4，背景颜色 fast(167ms) 过渡。
        if self.closable {
            let target = self.close_bg_target(t);
            self.close_bg.retarget(target, ctx.now, FAST_DUR);
            let cr = self.close_rect();
            let bg = self.close_bg.value(ctx.now);
            if bg.a != 0 {
                ctx.painter.fill_rounded_rect(cr, CLOSE_RADIUS, bg);
            }
            // 关闭图标（✕，Cancel ，12×12 svg 等价）。（InfoBar.scss:116-120）
            let _ = ctx.painter.draw_icon(Icon::Close.codepoint(), CLOSE_GLYPH, cr, self.close_fg(t));
        }
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if self.closed {
            return EventResult::NONE;
        }
        if !self.closable {
            return EventResult::NONE;
        }
        let mut redraw = false;
        match ev {
            InputEvent::PointerMove(p) => {
                let h = self.close_rect().contains(p);
                if h != self.hover_close {
                    self.hover_close = h;
                    if !h {
                        self.press_close = false;
                    }
                    redraw = true;
                }
            }
            InputEvent::PointerDown(p) => {
                if self.close_rect().contains(p) {
                    self.press_close = true;
                    self.hover_close = true;
                    redraw = true;
                }
            }
            InputEvent::PointerLeave => {
                if self.hover_close || self.press_close {
                    self.hover_close = false;
                    self.press_close = false;
                    redraw = true;
                }
            }
            InputEvent::PointerUp(p) => {
                if self.close_rect().contains(p) {
                    // 用户点关闭：设置 closed = true，控件后续 measure → 0×0、paint 跳过。
                    self.closed = true;
                    redraw = true;
                } else if self.press_close {
                    self.press_close = false;
                    redraw = true;
                }
            }
            _ => {}
        }
        EventResult { redraw, animating: self.close_bg.is_active(now) }
    }

    fn is_animating(&self, now: f64) -> bool {
        !self.closed && self.close_bg.is_active(now)
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::ToolTip
    }

    fn accessible_name(&self) -> String {
        if !self.title.is_empty() {
            self.title.clone()
        } else {
            self.message.clone()
        }
    }
}
