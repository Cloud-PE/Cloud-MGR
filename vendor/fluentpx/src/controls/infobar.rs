//! InfoBar（通知条）：Informational / Success / Warning / Error 四态。
//!
//! 真值来源：fluent-svelte `src/lib/InfoBar/InfoBar.scss` + `InfoBadge/InfoBadge.scss`
//! + `theme.css`（逐 token 解析）。结构（几何/边框/图标/关闭按钮/过渡）严格对齐 fluent-svelte，
//! 用户自选的语义背景色 token（system_*_bg / card_bg_secondary）保留不动。
//!
//! 关键几何（InfoBar.scss）：
//! * `.info-bar` min-block-size:48px；padding-inline-start:15px；border 1px `card-stroke-default`；
//!   border-radius `control-corner-radius`(4)。severity 背景：information=`card-background-secondary`，
//!   success/caution/critical=`system-background-*`。
//! * `.info-bar-icon` align-self:flex-start + margin-block-start:16px（顶部对齐，距顶 16）。
//!   图标是 InfoBadge：min 16×16、border-radius:16（药丸/圆）、padding 2px 4px、bg=system 语义色、
//!   内含 8×8 svg 字形（`text-on-accent-primary`）。
//! * `.info-bar-content` margin-inline-start:13px、block 上下各 7px、竖直居中。
//! * h5 = Body weight600（BODY_STRONG），margin-inline-end:12px，color `text-primary`。
//! * p = Body weight400（BODY），margin-inline-end:15px，color `text-primary`。
//! * `.info-bar-close-button` 38×38、margin 4px、align-self:flex-start、border-radius:4、
//!   transparent → hover `subtle-fill-secondary` → active `subtle-fill-tertiary`+`text-secondary`，
//!   transition fast(167ms) `fast-out-slow-in`(cubic-bezier 0,0,0,1)，svg 12×12。

use crate::anim::ColorTransition;
use crate::color::Color;
use crate::gfx::Icon;
use crate::typography::TextStyle;
use crate::widget::*;

// —— InfoBar.scss 像素真值 ——
const MIN_H: f32 = 48.0; // min-block-size: 48px
const CORNER: f32 = 4.0; // border-radius: var(--control-corner-radius) = 4
const PAD_START: f32 = 15.0; // padding-inline-start: 15px
const ICON_MARGIN_TOP: f32 = 16.0; // .info-bar-icon margin-block-start: 16px (flex-start)
const CONTENT_MARGIN_START: f32 = 13.0; // .info-bar-content margin-inline-start: 13px
const CONTENT_MARGIN_BLOCK: f32 = 7.0; // .info-bar-content margin-block-start/end: 7px
const LINE_HEIGHT: f32 = 20.0; // h5,p line-height: 20px
const MSG_MARGIN_END: f32 = 15.0; // p margin-inline-end: 15px
// .info-bar-icon 是 InfoBadge：min 16×16、padding 2px 4px、圆角 16。
const BADGE_W: f32 = 16.0; // InfoBadge min-inline-size（药丸宽，单字形时取最小）
const BADGE_H: f32 = 16.0; // InfoBadge min-block-size
const BADGE_RADIUS: f32 = 8.0; // border-radius:16 在 16px 高上即半圆 → 角半径 = h/2
const BADGE_GLYPH: f32 = 8.0; // InfoBadge svg 8×8
// .info-bar-close-button
const CLOSE_SIZE: f32 = 38.0; // 38×38
const CLOSE_MARGIN: f32 = 4.0; // margin: 4px
const CLOSE_RADIUS: f32 = 4.0; // border-radius: var(--control-corner-radius)
const CLOSE_GLYPH: f32 = 12.0; // svg 12×12

/// `--fds-control-fast-duration` = 167ms（关闭按钮背景过渡）。
const FAST_DUR: f64 = 0.167;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Informational,
    Success,
    Warning,
    Error,
}

pub struct InfoBar {
    pub severity: Severity,
    pub title: String,
    pub message: String,
    pub closed: bool,
    /// 是否显示关闭按钮（对应 InfoBar.svelte `closable`，默认 true）。
    pub closable: bool,
    hover_close: bool,
    press_close: bool,
    /// 关闭按钮背景的颜色过渡（transparent ↔ subtle-fill-*），fast(167ms)。
    close_bg: ColorTransition,
    close_bg_init: bool,
    rect: Rect,
}

impl InfoBar {
    pub fn new(severity: Severity, title: impl Into<String>, message: impl Into<String>) -> InfoBar {
        InfoBar {
            severity,
            title: title.into(),
            message: message.into(),
            closed: false,
            closable: true,
            hover_close: false,
            press_close: false,
            close_bg: ColorTransition::instant(Color::TRANSPARENT),
            close_bg_init: false,
            rect: Rect::default(),
        }
    }

    /// InfoBadge 内符号字形（叠在语义色药丸之上，`text-on-accent-primary`）。
    /// 取 Segoe Fluent Icons / MDL2 对应码位（与 InfoBadge.svelte 的 svg 语义一致）：
    /// information=Info、success=Completed、caution=Warning、critical=ErrorBadge。
    fn symbol_glyph(&self) -> char {
        match self.severity {
            Severity::Informational => Icon::Info.codepoint(),
            Severity::Success => Icon::Success.codepoint(),
            Severity::Warning => Icon::Warning.codepoint(),
            Severity::Error => Icon::Error.codepoint(),
        }
    }

    /// `.info-bar.severity-*` 背景（InfoBar.scss:14-30）。
    /// information=`card-background-secondary`；success/caution/critical=`system-background-*`。
    /// 这些是用户保留的语义背景 token。
    fn bg(&self, t: &crate::tokens::Tokens) -> Color {
        match self.severity {
            Severity::Informational => t.card_bg_secondary,
            Severity::Success => t.system_success_bg,
            Severity::Warning => t.system_caution_bg,
            Severity::Error => t.system_critical_bg,
        }
    }

    /// InfoBadge 药丸背景色（InfoBadge.scss:17-33 + theme.css `--fds-system-*`）。
    /// information=`system-solid-neutral`（深 hsl(0,0%,62%)=#9E9E9E / 浅 hsl(0,0%,54%)=#8A8A8A，
    /// 无对应 fluentpx token，按 theme.css 实色内联）；success=`system-success`、
    /// caution=`system-caution`、critical=`system-critical`（解析到同义 fluentpx token）。
    fn badge_bg(&self, t: &crate::tokens::Tokens) -> Color {
        match self.severity {
            Severity::Informational => {
                if t.is_dark { Color::hex("#9E9E9E") } else { Color::hex("#8A8A8A") }
            }
            Severity::Success => t.system_success,
            Severity::Warning => t.system_caution,
            Severity::Error => t.system_critical,
        }
    }

    /// `.info-bar-icon` 的 InfoBadge 矩形：顶部对齐，距条顶 margin-block-start:16px，
    /// 左缘 = padding-inline-start:15px。
    fn badge_rect(&self) -> Rect {
        Rect {
            x: self.rect.x + PAD_START,
            y: self.rect.y + ICON_MARGIN_TOP,
            w: BADGE_W,
            h: BADGE_H,
        }
    }

    /// 内容（标题+正文）左缘 = padding-start + 药丸宽 + content margin-inline-start。
    fn content_left(&self) -> f32 {
        self.rect.x + PAD_START + BADGE_W + CONTENT_MARGIN_START
    }

    /// `.info-bar-close-button` 矩形：38×38，右上角，margin 4px（align-self:flex-start）。
    fn close_rect(&self) -> Rect {
        Rect {
            x: self.rect.right() - CLOSE_MARGIN - CLOSE_SIZE,
            y: self.rect.y + CLOSE_MARGIN,
            w: CLOSE_SIZE,
            h: CLOSE_SIZE,
        }
    }

    /// 关闭按钮背景目标色：active→`subtle-fill-tertiary`，hover→`subtle-fill-secondary`，
    /// 否则 transparent（InfoBar.scss:104,110,114）。
    fn close_bg_target(&self, t: &crate::tokens::Tokens) -> Color {
        if self.press_close {
            t.subtle_fill_tertiary
        } else if self.hover_close {
            t.subtle_fill_secondary
        } else {
            Color::TRANSPARENT
        }
    }

    /// 关闭按钮前景色：active→`text-secondary`，否则 `text-primary`（InfoBar.scss:102,113）。
    fn close_fg(&self, t: &crate::tokens::Tokens) -> Color {
        if self.press_close {
            t.text_secondary
        } else {
            t.text_primary
        }
    }
}

impl Widget for InfoBar {
    fn measure(&mut self, available: Size) -> Size {
        if self.closed {
            return Size { w: 0.0, h: 0.0 };
        }
        // 高度：min-block-size:48；内容（标题行 + 正文行）若超过则撑高。
        // 单行（仅正文 / 仅标题）≈ 20 行高 + 上下各 7 margin = 34 < 48 → 取 48。
        // 标题+正文两行 = 40 + 14 = 54 > 48。
        let mut h = MIN_H;
        if !self.title.is_empty() && !self.message.is_empty() {
            // 两行内容：上下各 7 margin + 2×20 行高 = 54。
            h = MIN_H.max(2.0 * CONTENT_MARGIN_BLOCK + 2.0 * LINE_HEIGHT);
        }
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

        // 首帧把关闭按钮背景过渡置到当前目标（避免初次淡入）。
        if !self.close_bg_init {
            self.close_bg = ColorTransition::instant(self.close_bg_target(t));
            self.close_bg_init = true;
        }

        // —— 背景 + 边框 ——（InfoBar.scss:3-13）
        // severity 背景可为半透明（如 information=card-background-secondary #08FFFFFF/#80F6F6F6），
        // 在页面底色（solid_bg_base）上合成出实色后填入圆角矩形。
        let page = t.solid_bg_base;
        ctx.painter.fill_rounded_rect(r, CORNER, self.bg(t).over(page));
        // border: 1px solid var(--card-stroke-default)。background-clip:padding-box → 边框落在
        // 背景之外、直接压在页面底色上；card-stroke-default 半透明，故 over(page)。
        let border = t.card_stroke_default.over(page);
        ctx.painter.stroke_inner(r, CORNER, border, 1.0);

        // —— 图标：InfoBadge 药丸（语义色底 + text-on-accent 字形）——
        // border-radius:16 在 16px 高度上即半圆；药丸内 8×8 svg 字形居中。
        let badge = self.badge_rect();
        ctx.painter.fill_rounded_rect(badge, BADGE_RADIUS, self.badge_bg(t));
        let _ = ctx.painter.draw_icon(self.symbol_glyph(), BADGE_GLYPH, badge, t.text_on_accent_primary);

        // —— 标题 + 正文 ——（h5/p，均 text-primary，line-height 20）
        // 内容块竖直居中（.info-bar-content align:center），左缘 = content_left()。
        // 右侧到关闭按钮（或边缘）之间留 p 的 margin-inline-end:15px。
        let left = self.content_left();
        let right_limit = if self.closable {
            self.close_rect().x // 关闭按钮左缘
        } else {
            r.right()
        };
        let text_w = (right_limit - left - MSG_MARGIN_END).max(0.0);
        let has_title = !self.title.is_empty();
        let has_msg = !self.message.is_empty();

        if has_title && has_msg {
            // 两行：内容块高 = 2×20，竖直居中于条内（上下各 7 margin 对称）。
            let block_top = r.center_y() - LINE_HEIGHT;
            let title_rect = Rect { x: left, y: block_top, w: text_w, h: LINE_HEIGHT };
            let _ = ctx.painter.draw_text_leading(&self.title, TextStyle::BODY_STRONG, title_rect, t.text_primary);
            let msg_rect = Rect { x: left, y: block_top + LINE_HEIGHT, w: text_w, h: LINE_HEIGHT };
            let _ = ctx.painter.draw_text_wrapped(&self.message, TextStyle::BODY, msg_rect, t.text_primary);
        } else if has_title {
            // 仅标题：单行竖直居中。
            let rect = Rect { x: left, y: r.center_y() - LINE_HEIGHT / 2.0, w: text_w, h: LINE_HEIGHT };
            let _ = ctx.painter.draw_text_leading(&self.title, TextStyle::BODY_STRONG, rect, t.text_primary);
        } else {
            // 仅正文：单行竖直居中。
            let rect = Rect { x: left, y: r.center_y() - LINE_HEIGHT / 2.0, w: text_w, h: LINE_HEIGHT };
            let _ = ctx.painter.draw_text_wrapped_centered(&self.message, TextStyle::BODY, rect, t.text_primary);
        }

        // —— 关闭按钮 ——（38×38，圆角 4，背景过渡 fast，字形 12×12）
        if self.closable {
            // 过渡到当前目标色（hover/press 改变时由 on_event 触发 retarget）。
            self.close_bg.retarget(self.close_bg_target(t), ctx.now, FAST_DUR);
            let cr = self.close_rect();
            let bg = self.close_bg.value(ctx.now);
            if bg.a != 0 {
                ctx.painter.fill_rounded_rect(cr, CLOSE_RADIUS, bg);
            }
            let _ = ctx.painter.draw_icon(Icon::Close.codepoint(), CLOSE_GLYPH, cr, self.close_fg(t));
        }
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if self.closed || !self.closable {
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
}
