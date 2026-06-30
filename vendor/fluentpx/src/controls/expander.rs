//! Expander（可折叠卡片）—— 1:1 取自 **fluent-svelte** `src/lib/Expander/`（默认 direction=down）。
//!
//! 几何真值（Expander.scss）：
//! * `.expander-header`（Expander.scss:71-96）：`@include flex(align:center)`、typography-body、
//!   **padding: 8px**（注意 `padding-inline-start:16px` 在后续 `padding:8px` 简写处被覆盖 → 四周 8px）、
//!   `box-sizing:border-box`、`background-color: var(--card-background-default)`、
//!   `border: 1px solid var(--card-stroke-default)`、`border-radius: var(--control-corner-radius)`(4px)。
//!   无 min-height：行高由 chevron(32) + 上下 padding(8+8) 撑成 ≈48px。
//! * `.expander-icon`（Expander.scss:61-70）：**16×16**、`color: var(--text-primary)`、`margin-inline-end: 16px`。
//! * `.expander-header-title`（:83-85）：`flex:1 1 auto`，typography-body。
//! * `.expander-chevron`（Expander.scss:97-119）：**32×32**、`margin-inline-start: 20px`、`border:none`、
//!   `border-radius: var(--control-corner-radius)`(4px)、`background-color: var(--subtle-fill-transparent)`(透明)。
//!   内含 svg 12×12，`fill: currentColor`，∨ 路径（down），expanded 时 `rotate(180deg)`。
//! * `.expander-content`（Expander.scss:120-133 + direction-down :11-22）：`background-color: var(--card-background-secondary)`
//!   （与 header **不同色**）、`border: 1px solid var(--card-stroke-default)`、`border-block-start: none`(无顶边)、
//!   `border-radius` 仅下两角(0,0,4,4)、**padding: 16px**、`transform: translateY(-100%)`(折叠)。
//!   外层 `.expander-content-anchor`（:127-132）`overflow:hidden` + `max-height:0→huge` 充当固定裁剪框。
//!
//! 状态（Expander.scss）：
//! * `:hover .expander-chevron`（:89-91）→ chevron 背景 `var(--subtle-fill-secondary)`。
//! * `:active .expander-chevron`（:92-95）→ chevron `color: var(--text-secondary)` + 背景 `var(--subtle-fill-tertiary)`。
//!   （chevron 背景无 transition 声明 → 离散切换。）
//! * `:focus-visible`（:86-88, :109-111）→ `box-shadow: var(--focus-stroke)`（内 1px focus-stroke-inner + 外 3px focus-stroke-outer）。
//!
//! 动画（三段，全部出自 Expander.scss 的 transition）：
//! * **内容滑入（展开）**（:46-47）：`transition: var(--control-slow-duration)(333ms) var(--control-fast-out-slow-in-easing)(cubic-bezier 0,0,0,1) transform` → translateY(-100%)→none。
//! * **内容滑出（折叠）**（:126）：`transition: var(--control-fast-duration)(167ms) cubic-bezier(1,1,0,1) transform` → none→translateY(-100%)。
//! * **裁剪框 max-height**（:131）：`transition: 0ms linear var(--control-slow-duration)(333ms delay) max-height`；
//!   展开时 `transition:none`（:50，立即变高）→ 内容立即可布局；折叠时延迟 333ms 后 0ms 内塌到 0 → 内容高度保持 333ms 再消失。
//! * **chevron 旋转**（:116-117）：`transition: calc(var(--control-faster-duration)*1.2)(≈99.6ms) linear transform var(--control-faster-duration)(83ms delay)`。

use crate::anim::cubic_bezier;
use crate::gfx::Icon;
use crate::typography::TextStyle;
use crate::widget::*;

const HEADER_H: f32 = 48.0; // 无 min-height；chevron(32)+padding(8+8)=48
const CORNER: f32 = 4.0; // --control-corner-radius
const HEADER_PAD: f32 = 8.0; // .expander-header padding:8px（覆盖 padding-inline-start:16px）
const ICON_SIZE: f32 = 16.0; // .expander-icon 16×16
const ICON_MARGIN_R: f32 = 16.0; // .expander-icon margin-inline-end:16px
const CHEVRON_BTN: f32 = 32.0; // .expander-chevron 32×32
const CHEVRON_MARGIN_L: f32 = 20.0; // .expander-chevron margin-inline-start:20px
const CHEVRON_GLYPH: f32 = 12.0; // svg 12×12
const CONTENT_PAD: f32 = 16.0; // .expander-content padding:16px
const ROW_H: f32 = 24.0;

// 内容揭示时间线（Expander.scss transition）。
const EXPAND_DUR: f64 = 0.333; // .expanded .expander-content transform：--control-slow-duration
const COLLAPSE_DUR: f64 = 0.167; // .expander-content transform：--control-fast-duration
// 裁剪框 max-height 折叠延迟（:131 transition delay = --control-slow-duration），到时内容高度才塌到 0。
const COLLAPSE_HIDE: f64 = 0.333;

// Chevron 旋转时间线（Expander.scss:116-117）：duration = faster×1.2、linear、delay = faster。
const CHEVRON_DELAY: f64 = 0.083; // var(--control-faster-duration)
const CHEVRON_DUR: f64 = 0.0996; // calc(var(--control-faster-duration) * 1.2)

pub struct Expander {
    pub title: String,
    pub icon: Option<Icon>,
    /// 内容行（label, value），仿 Windows 设置「关于」卡片（本工程演示用扩展，非 WinUI 模板内容）。
    pub rows: Vec<(String, String)>,
    pub expanded: bool,
    label_col: f32,
    anim_from: f32,
    anim_start: f64,
    expanding: bool,
    interaction: Interaction,
    rect: Rect,
}

impl Expander {
    pub fn new(title: impl Into<String>, icon: Option<Icon>, rows: Vec<(String, String)>, expanded: bool) -> Expander {
        Expander {
            title: title.into(),
            icon,
            rows,
            expanded,
            label_col: 110.0,
            anim_from: if expanded { 1.0 } else { 0.0 },
            anim_start: -1.0,
            expanding: expanded,
            interaction: Interaction::default(),
            rect: Rect::default(),
        }
    }

    /// 设置 label 列宽（value 起始 x 偏移）。
    pub fn with_label_col(mut self, w: f32) -> Self {
        self.label_col = w;
        self
    }

    fn content_full_h(&self) -> f32 {
        CONTENT_PAD * 2.0 + self.rows.len() as f32 * ROW_H
    }

    fn cur_dur(&self) -> f64 {
        if self.expanding { EXPAND_DUR } else { COLLAPSE_DUR }
    }

    fn target(&self) -> f32 {
        if self.expanded { 1.0 } else { 0.0 }
    }

    /// 已逝动画时间（秒）；anim_start<0 表示无动画（处于稳态）。
    fn elapsed(&self, now: f64) -> Option<f64> {
        if self.anim_start < 0.0 {
            None
        } else {
            Some(now - self.anim_start)
        }
    }

    /// 内容滑动进度 0(折叠/translateY -100%)→1(展开/translateY none)。
    /// 展开缓动 cubic-bezier(0,0,0,1)（Expander.scss:46）；折叠 cubic-bezier(1,1,0,1)（Expander.scss:126）。
    fn progress(&self, now: f64) -> f32 {
        let Some(e) = self.elapsed(now) else {
            return self.target();
        };
        let t = (e / self.cur_dur()).clamp(0.0, 1.0) as f32;
        let eased = if self.expanding {
            cubic_bezier(0.0, 0.0, 0.0, 1.0, t) // 展开：--control-fast-out-slow-in-easing
        } else {
            cubic_bezier(1.0, 1.0, 0.0, 1.0, t) // 折叠：cubic-bezier(1,1,0,1)
        };
        self.anim_from + (self.target() - self.anim_from) * eased
    }

    /// 内容是否仍应被布局/绘制（满尺寸）。对应 `.expander-content-anchor` 的 `overflow:hidden` + max-height。
    /// 展开瞬间 max-height 变高（transition:none）→ 立即可见；折叠时 max-height transition 延迟 333ms（:131）
    /// 才在 0ms 内塌到 0 → 内容保持满高度到 COLLAPSE_HIDE(333ms)。
    fn content_visible(&self, now: f64) -> bool {
        if self.expanded {
            return true; // 已展开（含展开动画全程）
        }
        // 折叠中：直到 COLLAPSE_HIDE 才隐藏；之后（稳态折叠）不可见。
        match self.elapsed(now) {
            Some(e) if !self.expanding => e < COLLAPSE_HIDE,
            _ => false,
        }
    }

    /// chevron 旋转进度（Expander.scss:116-117：linear、duration ≈99.6ms、delay 83ms）。
    /// 在 delay 前保持起点；之后线性插到目标。
    fn chevron_progress(&self, now: f64) -> f32 {
        let Some(e) = self.elapsed(now) else {
            return self.target();
        };
        let t = ((e - CHEVRON_DELAY) / CHEVRON_DUR).clamp(0.0, 1.0) as f32; // linear，含 delay
        self.anim_from + (self.target() - self.anim_from) * t
    }

    /// 当前总高（含揭示动画）。供宿主排布。
    /// WinUI：内容 Row 为 `*`，Visible 时即占满尺寸（非渐长）；滑入只在固定裁剪框内位移。
    /// 故 header 高恒定，内容区在「应可见」时整段占位，否则 0。
    pub fn height(&self, now: f64) -> f32 {
        if self.content_visible(now) {
            HEADER_H + self.content_full_h()
        } else {
            HEADER_H
        }
    }

    fn toggle(&mut self, now: f64) {
        self.anim_from = self.progress(now);
        self.expanded = !self.expanded;
        self.expanding = self.expanded;
        self.anim_start = now;
    }

    fn chevron_rect(&self) -> Rect {
        // chevron 贴 header 右侧 padding(8px) 内沿，竖向居中（flex align:center）。
        Rect {
            x: self.rect.right() - HEADER_PAD - CHEVRON_BTN,
            y: self.rect.y + (HEADER_H - CHEVRON_BTN) / 2.0,
            w: CHEVRON_BTN,
            h: CHEVRON_BTN,
        }
    }

    fn header_rect(&self) -> Rect {
        Rect { x: self.rect.x, y: self.rect.y, w: self.rect.w, h: HEADER_H }
    }
}

impl Widget for Expander {
    fn measure(&mut self, available: Size) -> Size {
        Size { w: available.w, h: HEADER_H }
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
        let r = self.rect;
        let disabled = !self.interaction.enabled;

        let content_h = self.content_full_h();
        let show_content = self.content_visible(now) && content_h > 0.5;
        // 揭示进度（0=折叠/全隐，1=展开/全显）。内容沿 TranslateY 从 -content_h 滑到 0。
        let p = self.progress(now).clamp(0.0, 1.0);

        // —— 边框/背景预合成色 ——
        // `border: 1px solid var(--card-stroke-default)`（Expander.scss:81,124）。--card-stroke-default 半透明
        // （深 hsla(0,0%,0%,10%)=#19000000 / 浅 hsla(0,0%,0%,5.78%)），`background-clip: padding-box` 下叠在窗口底色上。
        let stroke = t.card_stroke_default.over(t.solid_bg_base);
        // header `background-color: var(--card-background-default)`；content `var(--card-background-secondary)`（不同色）。
        let header_bg = t.card_bg_default.over(t.solid_bg_base);
        let content_bg = t.card_bg_secondary.over(t.solid_bg_base);

        // —— Header 背景 ——
        // 折叠：四角全圆（border-radius: var(--control-corner-radius) = 4）。
        // 展开 down：仅上两角圆（Expander.scss:18-22 border-end-*-radius:0）。
        // painter 仅有 uniform 圆角：先画圆角矩形，展开时用方块补齐底部两角。
        let header = self.header_rect();
        ctx.painter.fill_rounded_rect(header, CORNER, header_bg);
        if show_content {
            // 展开：方块覆盖 header 底部两角，使下沿成直角（与内容无缝衔接）。
            let patch = Rect { x: header.x, y: header.bottom() - CORNER, w: header.w, h: CORNER };
            ctx.painter.fill_rect(patch, header_bg);
        }

        // —— 内容区（满尺寸，固定裁剪 + TranslateY 滑入）——
        if show_content {
            let content_top = r.y + HEADER_H;
            // 固定裁剪框 = `.expander-content-anchor { overflow: hidden }`（Expander.scss:127-130）。
            let clip = Rect { x: r.x, y: content_top, w: r.w, h: content_h };
            ctx.painter.push_clip(clip);

            // 内容背景：var(--card-background-secondary)（仅下两角圆，border-start-*-radius:0）。
            // translateY：折叠 -100% → 展开 none（净进度 p）。
            let ty = -content_h * (1.0 - p);
            let content_rect = Rect { x: r.x, y: content_top + ty, w: r.w, h: content_h };
            ctx.painter.fill_rounded_rect(content_rect, CORNER, content_bg);
            // 内容仅下圆：用方块补齐顶部两角（贴 header 处为直角）。
            let top_patch = Rect { x: content_rect.x, y: content_rect.y, w: content_rect.w, h: CORNER };
            ctx.painter.fill_rect(top_patch, content_bg);

            // 信息行：label（次要色）+ value（主色）。随内容一起 TranslateY 位移。
            // 内容左内边距 = .expander-content padding:16px（Expander.scss:125），无额外缩进。
            let label_x = r.x + CONTENT_PAD;
            let value_x = label_x + self.label_col;
            let label_col = t.text_secondary;
            let value_col = t.text_primary;
            let mut ry = content_rect.y + CONTENT_PAD;
            for (label, value) in &self.rows {
                let _ = ctx.painter.draw_text_leading(label, TextStyle::BODY, Rect::new(label_x, ry, self.label_col, ROW_H), label_col);
                let _ = ctx.painter.draw_text_leading(value, TextStyle::BODY, Rect::new(value_x, ry, (r.right() - value_x - CONTENT_PAD).max(0.0), ROW_H), value_col);
                ry += ROW_H;
            }
            ctx.painter.pop_clip();
        }

        // —— 边框 ——
        // header `border:1px`（全周）；content `border:1px` 但 `border-block-start:none`（无顶边，Expander.scss:12）。
        // 折叠时：header 一圈圆角描边。展开时：header 上圆角 + content 下圆角，接缝处只有 header 底边
        // （content 无顶边）→ 单条分隔线。用整卡圆角描边 + 一条接缝线近似。
        if show_content {
            let content_top = r.y + HEADER_H;
            let card = Rect { x: r.x, y: r.y, w: r.w, h: HEADER_H + content_h };
            ctx.painter.stroke_inner(card, CORNER, stroke, 1.0);
            // header/content 接缝（header 底边）。
            ctx.painter.draw_line(r.x + 1.0, content_top, r.right() - 1.0, content_top, stroke, 1.0);
        } else {
            ctx.painter.stroke_inner(header, CORNER, stroke, 1.0);
        }

        // —— Header 内容：图标 + 标题（左起 padding 8px）——
        // .expander-icon color = var(--text-primary)（Expander.scss:63）；标题 typography-body（非 strong）。
        let fg_title = if disabled { t.text_disabled } else { t.text_primary };
        let mut tx = r.x + HEADER_PAD;
        if let Some(ic) = self.icon {
            let ir = Rect::new(tx, r.y + (HEADER_H - ICON_SIZE) / 2.0, ICON_SIZE, ICON_SIZE);
            ctx.painter.draw_glyph(ic, ir, if disabled { t.text_disabled } else { t.text_primary });
            tx += ICON_SIZE + ICON_MARGIN_R;
        }
        let cr = self.chevron_rect();
        // 标题与 chevron 之间为 chevron 的 margin-inline-start:20px。
        let title_w = (cr.x - CHEVRON_MARGIN_L - tx).max(0.0);
        let _ = ctx.painter.draw_text_leading(&self.title, TextStyle::BODY, Rect::new(tx, r.y, title_w, HEADER_H), fg_title);

        // —— Chevron 按钮背景（header 状态驱动，离散切换无 transition）+ 矢量 ∨ 路径旋转 ——
        // Expander.scss:89-95：hover → subtle-fill-secondary；active(pressed) → subtle-fill-tertiary。
        let vs = self.interaction.visual_state();
        // 用户要求：悬浮 / 点击 card 头时，右侧 chevron 都不要那圈「阴影」（hover/pressed 背景）。
        // 反馈完全交给 chevron 自身的 ∨↔∧ 旋转，背景什么都不画。
        // chevron 前景 = currentColor：rest .expander-chevron color = var(--text-primary)；
        // :active 时 color: var(--text-secondary)（Expander.scss:93）；disabled → text-disabled。
        let chev_fg = if disabled {
            t.text_disabled
        } else if vs == VisualState::Pressed {
            t.text_secondary
        } else {
            t.text_primary
        };
        // expanded 时 svg rotate(180deg)：∨→∧。progress 0→1 映射 0°→180°。
        let angle = self.chevron_progress(now).clamp(0.0, 1.0) * 180.0;
        ctx.painter.set_rotation_about(Point { x: cr.center_x(), y: cr.center_y() }, angle);
        // ∨ 路径（apex 朝下）：取 svelte path d 的三个拐点 (2.15,4.65)(6,7.79)(9.85,4.65) / 12 归一化。
        // Round 端帽/接合由 stroke_polyline 提供。
        let gx = cr.center_x() - CHEVRON_GLYPH / 2.0;
        let gy = cr.center_y() - CHEVRON_GLYPH / 2.0;
        let pt = |u: f32, v: f32| (gx + u * CHEVRON_GLYPH, gy + v * CHEVRON_GLYPH);
        ctx.painter.stroke_polyline(&[pt(0.179, 0.387), pt(0.500, 0.649), pt(0.821, 0.387)], chev_fg, 1.2);
        ctx.painter.set_transform(None);

        // —— focus-visible 焦点环（Expander.scss:86-88 / :109-111：box-shadow: var(--focus-stroke)）——
        // --fds-focus-stroke = 0 0 0 1px inner, 0 0 0 3px outer：外 3px(focus-stroke-outer) + 内 1px(focus-stroke-inner)。
        // header 焦点环描在 header 轮廓外（折叠=整卡圆角，展开=卡片整体）。
        if self.interaction.focused && !disabled {
            let card = if show_content {
                Rect { x: r.x, y: r.y, w: r.w, h: HEADER_H + content_h }
            } else {
                header
            };
            // 外环：向外扩 3px、2px 厚（中心线落在 +2 处，外缘≈+3）。
            let outer = card.inset(-2.0);
            ctx.painter.stroke_inner(outer, CORNER + 2.0, t.focus_stroke_outer, 2.0);
            // 内环：贴轮廓内沿 1px。
            ctx.painter.stroke_inner(card, CORNER, t.focus_stroke_inner, 1.0);
        }
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if !self.interaction.enabled {
            return EventResult::NONE;
        }
        let header = self.header_rect();
        let before = self.interaction;
        let mut toggled = false;
        match ev {
            InputEvent::PointerMove(p) => self.interaction.hovered = header.contains(p),
            InputEvent::PointerLeave => {
                self.interaction.hovered = false;
                self.interaction.pressed = false;
            }
            InputEvent::PointerDown(p) => {
                if header.contains(p) {
                    self.interaction.pressed = true;
                }
            }
            InputEvent::PointerUp(p) => {
                if self.interaction.pressed && header.contains(p) {
                    toggled = true;
                }
                self.interaction.pressed = false;
            }
            _ => {}
        }
        if toggled {
            self.toggle(now);
        }
        let changed = toggled || before.visual_state() != self.interaction.visual_state();
        EventResult { redraw: changed, animating: changed || self.is_animating(now) }
    }

    fn is_animating(&self, now: f64) -> bool {
        let Some(e) = self.elapsed(now) else {
            return false;
        };
        // 取内容滑动、裁剪框塌缩(折叠时延迟 333ms)、chevron(delay+dur) 三者最长，确保整段动画期间持续刷新。
        let content_dur = if self.expanding { EXPAND_DUR } else { COLLAPSE_DUR.max(COLLAPSE_HIDE) };
        let chev_dur = CHEVRON_DELAY + CHEVRON_DUR;
        e < content_dur.max(chev_dur)
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Button
    }
    fn accessible_name(&self) -> String {
        self.title.clone()
    }
}
