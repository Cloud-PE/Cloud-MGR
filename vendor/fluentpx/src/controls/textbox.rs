//! TextBox（编辑框，单行）：焦点 / 光标 / 选区 / 清除按钮 / **右键上下文菜单**（剪切·复制·粘贴·全选）。
//!
//! 真值参考：**fluent-svelte**（MIT Svelte/CSS WinUI 移植）——
//! `src/lib/TextBox/TextBox.scss` + `TextBox.svelte` + `TextBoxButton.scss`，
//! token 经 `src/lib/theme.css` 解析（`--fds-*`）。结构按其 SCSS 1:1 复刻：
//!
//! 几何（TextBox.scss）：
//! * 容器 `.text-box-container`：`border: 1px solid --control-stroke-default`，
//!   `border-radius: --control-corner-radius (4px)`，`inline-size:100%`，flex align center。
//! * 输入 `.text-box`：`min-block-size: 30px`，`padding-inline: 10px`（左右各 10），`flex:1 1 auto`。
//! * 下划线 `.text-box-underline`：绝对定位 `inset-inline-start:-1px; inset-block-start:-1px;`
//!   `inline-size: calc(100%+2px); block-size: calc(100%+2px); overflow:hidden;`，
//!   其 `::after` 画 `border-bottom`：rest = `1px --control-strong-stroke-default`，
//!   focus-within = `2px --accent-default`；disabled 时 `display:none`（隐藏底边）。
//! * 按钮组 `.text-box-buttons`：flex align center，`flex:0 0 auto`；按钮间 `margin-inline-start:6px`
//!   （首个 0），末个 `margin-inline-end:4px`。
//!
//! 背景（`.text-box-container` 逐状态，无 transition → 0ms 瞬切）：
//! * rest = `--control-fill-default`，hover = `--control-fill-secondary`，
//!   focus-within = `--control-fill-input-active`，disabled = `--control-fill-disabled`。
//!
//! 文字：rest 正文 `--text-primary`，占位符 `--text-secondary`，
//! focus-within 占位符 `--text-tertiary`，disabled `--text-disabled`。
//!
//! 清除按钮（TextBoxButton.scss，`.text-box-clear-button`，默认 `display:none`）：
//! 仅在 focus-within 且有值（且非 readonly/disabled）时显示。
//! `min-inline-size:26px; min-block-size:22px; padding:3px 5px; border-radius:4px;`
//! 色：rest fg `--text-secondary` / bg `--subtle-fill-transparent`（透明）；
//! hover bg `--subtle-fill-secondary`；active fg `--text-tertiary` / bg `--subtle-fill-tertiary`；
//! 图标 ✕ 12×12（viewBox 0 0 12 12，`fill:currentColor`）。
//!
//! 用户指定调色（保留，不随 fluent-svelte 重新着色）：TextBox 整圈边框用 `search_border`，
//! 底边 rest 用 `search_border_bottom`（focus 时底边按结构改为 2px 强调色）。
//!
//! 限制：仅处理 WM_CHAR 与基本编辑键，**中文 IME 需对接 TSF**；鼠标拖选暂未做。
//! 仅实现默认 `type=text` 的清除按钮；search/reveal 按钮需 type 形参（构造仅取 placeholder）暂未接。

use crate::clipboard;
use crate::color::Color;
use crate::typography::TextStyle;
use crate::widget::*;

// —— 几何真值（fluent-svelte TextBox.scss / TextBoxButton.scss + theme.css）——
const BOX_H: f32 = 32.0; // 容器 content-box：输入 min-block-size 30 + 上下各 1px 边框 = 32（外框总高）
const MIN_W: f32 = 64.0; // 无源码最小宽（inline-size:100% 铺满）；保底 64 避免退化
const CORNER: f32 = 4.0; // --fds-control-corner-radius: 4px
const BORDER: f32 = 1.0; // .text-box-container border: 1px
const UNDERLINE_REST: f32 = 1.0; // underline::after rest border-bottom: 1px
const UNDERLINE_FOCUS: f32 = 2.0; // underline::after focus border-bottom: 2px
const PAD_INLINE: f32 = 10.0; // .text-box padding-inline: 10px（左右各 10）
// 按钮组（TextBoxButton.scss）
const BTN_MIN_W: f32 = 26.0; // min-inline-size: 26px
const BTN_MIN_H: f32 = 22.0; // min-block-size: 22px
const BTN_PAD_X: f32 = 5.0; // padding 3px 5px（水平）
const BTN_GAP: f32 = 6.0; // .text-box-button margin-inline-start: 6px
const BTN_MARGIN_END: f32 = 4.0; // 末个 margin-inline-end: 4px
const BTN_ICON: f32 = 12.0; // 清除图标 12×12
const LEADING_ICON: f32 = 16.0; // 框内前导搜索图标 16×16
const LEADING_GAP: f32 = 8.0; // 图标与文字间距

const BLINK_MS: f64 = 530.0;
const MENU_W: f32 = 150.0;
const MENU_ITEM_H: f32 = 32.0;
const MENU_VPAD: f32 = 4.0;

const MENU_ITEMS: [&str; 4] = ["剪切", "复制", "粘贴", "全选"];

pub struct TextBox {
    pub text: String,
    pub placeholder: String,
    caret: usize,
    sel_anchor: Option<usize>, // 选区另一端；None 表示无选区
    focused: bool,
    hovered: bool,
    clear_hovered: bool,
    clear_pressed: bool,
    rect: Rect,
    pub enabled: bool,
    pub readonly: bool,
    focus_time: f64,
    // 右键菜单
    ctx_open: bool,
    ctx_pos: Point,
    ctx_hover: Option<usize>,
    /// 框内左侧前导图标（如搜索放大镜 \u{E721}）；None=无。
    leading_icon: Option<char>,
    /// 上一帧光标（caret）的逻辑坐标，供 IME 候选窗定位（每帧 paint 更新）。
    caret_px: Point,
    /// IME 内联组字串（未上屏的拼音）；自绘在框内、带下划线；空=无组字。
    composition: String,
}

impl TextBox {
    pub fn new(placeholder: impl Into<String>) -> TextBox {
        TextBox {
            text: String::new(),
            placeholder: placeholder.into(),
            caret: 0,
            sel_anchor: None,
            focused: false,
            hovered: false,
            clear_hovered: false,
            clear_pressed: false,
            rect: Rect::default(),
            enabled: true,
            readonly: false,
            focus_time: 0.0,
            ctx_open: false,
            ctx_pos: Point::default(),
            ctx_hover: None,
            leading_icon: None,
            caret_px: Point::default(),
            composition: String::new(),
        }
    }

    /// 在框内左侧放一个 Segoe Fluent Icons 搜索放大镜（\u{E721}），文字向右让位。
    pub fn with_search_icon(mut self) -> Self {
        self.leading_icon = Some('\u{E721}');
        self
    }

    fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    fn caret_visible(&self, now: f64) -> bool {
        if !self.focused || self.sel_range().is_some() {
            return false;
        }
        let elapsed_ms = (now - self.focus_time) * 1000.0;
        ((elapsed_ms / BLINK_MS) as i64) % 2 == 0
    }

    /// 清除按钮是否可见：fluent-svelte `.text-box-clear-button` 默认 `display:none`，
    /// 仅 `focus-within` 时 `display:flex`，且 svelte 模板要求 `clearButton && value && !readonly && !disabled`。
    fn clear_visible(&self) -> bool {
        self.enabled && self.focused && !self.readonly && !self.text.is_empty()
    }

    /// 清除按钮矩形（按 TextBoxButton 几何贴右排布；末个 margin-inline-end:4px）。
    /// 高度取 min-block-size 22 与容器内高的较小者，垂直居中。
    fn clear_rect(&self) -> Rect {
        let r = self.rect;
        let bw = (BTN_ICON + BTN_PAD_X * 2.0).max(BTN_MIN_W); // 12 + 5*2 = 22 → 取 min 26
        let bh = BTN_MIN_H.min((r.h - BORDER * 2.0).max(0.0));
        let bx = r.right() - BORDER - BTN_MARGIN_END - bw;
        let by = r.y + (r.h - bh) / 2.0;
        Rect { x: bx, y: by, w: bw, h: bh }
    }

    /// 编辑区右内边界：有清除按钮时让出按钮 + 间隙，否则留 padding-inline。
    fn content_right(&self) -> f32 {
        let r = self.rect;
        if self.clear_visible() {
            self.clear_rect().x - BTN_GAP
        } else {
            r.right() - BORDER - PAD_INLINE
        }
    }

    /// 归一化选区（起,止），无选区返回 None。
    fn sel_range(&self) -> Option<(usize, usize)> {
        let a = self.sel_anchor?;
        if a == self.caret {
            None
        } else {
            Some((a.min(self.caret), a.max(self.caret)))
        }
    }

    fn selected_text(&self) -> String {
        match self.sel_range() {
            Some((s, e)) => self.text.chars().skip(s).take(e - s).collect(),
            None => String::new(),
        }
    }

    /// 删除当前选区（若有），caret 落到选区起点。返回是否删除了内容。
    fn delete_selection(&mut self) -> bool {
        if let Some((s, e)) = self.sel_range() {
            let bs = byte_index(&self.text, s);
            let be = byte_index(&self.text, e);
            self.text.replace_range(bs..be, "");
            self.caret = s;
            self.sel_anchor = None;
            true
        } else {
            false
        }
    }

    fn insert_str(&mut self, ins: &str) {
        self.delete_selection();
        let idx = byte_index(&self.text, self.caret);
        self.text.insert_str(idx, ins);
        self.caret += ins.chars().count();
        self.sel_anchor = None;
    }

    fn do_action(&mut self, action: usize) {
        match action {
            0 => {
                // 剪切
                let t = self.selected_text();
                if !t.is_empty() {
                    clipboard::set_text(&t);
                    self.delete_selection();
                }
            }
            1 => {
                // 复制
                let t = self.selected_text();
                if !t.is_empty() {
                    clipboard::set_text(&t);
                }
            }
            2 => {
                // 粘贴
                if let Some(s) = clipboard::get_text() {
                    let s: String = s.chars().filter(|c| !c.is_control()).collect();
                    self.insert_str(&s);
                }
            }
            3 => {
                // 全选
                if self.char_len() > 0 {
                    self.sel_anchor = Some(0);
                    self.caret = self.char_len();
                }
            }
            _ => {}
        }
    }

    fn menu_rect(&self) -> Rect {
        let h = MENU_ITEMS.len() as f32 * MENU_ITEM_H + MENU_VPAD * 2.0;
        Rect { x: self.ctx_pos.x, y: self.ctx_pos.y, w: MENU_W, h }
    }

    fn menu_item_rect(&self, i: usize) -> Rect {
        let m = self.menu_rect();
        Rect { x: m.x, y: m.y + MENU_VPAD + i as f32 * MENU_ITEM_H, w: m.w, h: MENU_ITEM_H }
    }
}

impl Widget for TextBox {
    fn measure(&mut self, available: Size) -> Size {
        // fluent-svelte：容器 inline-size:100%（铺满父级），输入 min-block-size:30px。
        Size { w: available.w.max(MIN_W), h: BOX_H }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = Rect { x: rect.x, y: rect.y, w: rect.w, h: BOX_H };
    }

    fn hit_test(&self, p: Point) -> bool {
        self.rect.contains(p) || (self.ctx_open && self.menu_rect().contains(p))
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        let t = ctx.tokens;
        let r = self.rect;

        // —— 容器背景（.text-box-container background-color，逐状态，无 transition → 0ms）——
        // rest=control-fill-default，hover=control-fill-secondary，
        // focus-within=control-fill-input-active，disabled=control-fill-disabled。
        let bg = if !self.enabled {
            t.control_fill_disabled
        } else if self.focused {
            t.control_fill_input_active
        } else if self.hovered {
            t.control_fill_secondary
        } else {
            t.control_fill_default
        };
        // 背景落在 1px 边框内沿（border-box；background-clip: padding-box）。
        let bg_rect = Rect {
            x: r.x + BORDER,
            y: r.y + BORDER,
            w: (r.w - BORDER * 2.0).max(0.0),
            h: (r.h - BORDER * 2.0).max(0.0),
        };
        ctx.painter.fill_rounded_rect(bg_rect, (CORNER - BORDER).max(0.0), bg);

        // —— 容器边框（.text-box-container border: 1px）——
        // 结构：整圈 1px；用户指定 TextBox 用 search_border 整圈。各状态边框色不变
        //（fluent-svelte 中 --control-stroke-default 不随 focus 改变；只有下划线底边变）。
        let border_col = t.search_border();
        ctx.painter.stroke_inner(r, CORNER, border_col, BORDER);

        // —— 下划线 ::after（.text-box-underline::after border-bottom）——
        // disabled 时 `display:none`（隐藏底边）；否则 rest=1px、focus=2px。
        // underline 元素 inset -1px / size +2px + overflow:hidden + border-radius，
        // 故底边沿 4px 圆角裁剪——在圆角内不画直段，避免覆盖转角。
        if self.enabled {
            let (thick, col) = if self.focused {
                // focus-within：2px solid --accent-default（结构所定，非用户色）。
                (UNDERLINE_FOCUS, t.accent_fill_default())
            } else {
                // rest/hover：用户指定 search_border_bottom（结构上等价 1px strong-stroke 底边）。
                (UNDERLINE_REST, t.search_border_bottom())
            };
            // 满宽直线（x..right），y 恒定；用容器圆角裁剪层把底部两角切掉
            // —— 线始终是直的，只在转角处下缘被圆角裁去一部分（overflow:hidden+border-radius）。
            let line = Rect { x: r.x, y: r.bottom() - thick, w: r.w, h: thick };
            ctx.painter.push_rounded_clip(r, CORNER);
            ctx.painter.fill_rect(line, col);
            ctx.painter.pop_layer();
        }

        // —— 编辑区（.text-box，padding-inline:10px，flex:1 1 auto）——
        // 框内左侧前导图标（搜索放大镜）：先画，再把文字内沿右移让位。
        let lead = if self.leading_icon.is_some() { LEADING_ICON + LEADING_GAP } else { 0.0 };
        if let Some(g) = self.leading_icon {
            let icr = Rect {
                x: r.x + BORDER + PAD_INLINE,
                y: r.y + (r.h - LEADING_ICON) / 2.0,
                w: LEADING_ICON,
                h: LEADING_ICON,
            };
            let col = if !self.enabled { t.text_disabled } else { t.text_secondary };
            let _ = ctx.painter.draw_icon(g, LEADING_ICON, icr, col);
        }
        // 左：border + 10 + 前导图标；右：让出清除按钮或 border + 10。垂直居中（flex align center）。
        let inner_left = r.x + BORDER + PAD_INLINE + lead;
        let inner_right = self.content_right();
        let inner = Rect {
            x: inner_left,
            y: r.y,
            w: (inner_right - inner_left).max(0.0),
            h: r.h,
        };

        // —— 选区高亮 —— 浏览器原生 ::selection（强调色背景、满不透明、直角整行）。
        if let Some((s, e)) = self.sel_range() {
            let pre: String = self.text.chars().take(s).collect();
            let mid: String = self.text.chars().skip(s).take(e - s).collect();
            let x0 = inner.x + ctx.painter.measure_text(&pre, TextStyle::BODY).map(|z| z.w).unwrap_or(0.0);
            let w = ctx.painter.measure_text(&mid, TextStyle::BODY).map(|z| z.w).unwrap_or(0.0);
            // 行盒：边框内沿之间。
            let top = r.y + BORDER;
            let hl = Rect { x: x0, y: top, w, h: (r.bottom() - BORDER - top).max(0.0) };
            ctx.painter.fill_rect(hl, t.accent.base);
        }

        // —— 文字 / 占位符 / IME 内联组字串 ——
        let fg = if self.enabled { t.text_primary } else { t.text_disabled };
        let has_comp = !self.composition.is_empty();
        if self.text.is_empty() && !has_comp {
            // 占位符：rest=text-secondary，focus-within=text-tertiary，disabled=text-disabled。
            let ph = if !self.enabled {
                t.text_disabled
            } else if self.focused {
                t.text_tertiary
            } else {
                t.text_secondary
            };
            let _ = ctx.painter.draw_text_leading(&self.placeholder, TextStyle::BODY, inner, ph);
        } else {
            let _ = ctx.painter.draw_text_leading(&self.text, TextStyle::BODY, inner, fg);
            // IME 内联组字串：接在已上屏文字之后，自绘在框内并加下划线（标记「未上屏」），
            // 不再用系统组字浮窗（白底盖在框上很丑）。
            if has_comp {
                let tw = ctx.painter.measure_text(&self.text, TextStyle::BODY).map(|s| s.w).unwrap_or(0.0);
                let comp_x = inner.x + tw;
                let comp_inner = Rect { x: comp_x, ..inner };
                let _ = ctx.painter.draw_text_leading(&self.composition, TextStyle::BODY, comp_inner, fg);
                let cw = ctx.painter.measure_text(&self.composition, TextStyle::BODY).map(|s| s.w).unwrap_or(0.0);
                let uy = r.bottom() - 7.0;
                ctx.painter.draw_line(comp_x, uy, comp_x + cw, uy, fg, 1.0);
            }
        }

        // —— 光标 + IME 候选窗定位 ——
        if self.focused {
            let text_w = ctx.painter.measure_text(&self.text, TextStyle::BODY).map(|s| s.w).unwrap_or(0.0);
            if has_comp {
                // 组字中：候选窗放到组字串开头的下方；不画闪烁光标（已有下划线表示输入位置）。
                self.caret_px = Point { x: inner.x + text_w, y: r.y + 8.0 };
            } else {
                let prefix: String = self.text.chars().take(self.caret).collect();
                let w = ctx.painter.measure_text(&prefix, TextStyle::BODY).map(|s| s.w).unwrap_or(0.0);
                let cx = inner.x + w;
                self.caret_px = Point { x: cx, y: r.y + 8.0 };
                if self.caret_visible(ctx.now) {
                    ctx.painter.draw_line(cx, r.y + 6.0, cx, r.bottom() - 6.0, t.text_primary, 1.0);
                }
            }
        }

        // —— 清除按钮（.text-box-clear-button）——
        if self.clear_visible() {
            let br = self.clear_rect();
            // 背景：rest 透明 / hover subtle-secondary / active subtle-tertiary（无 transition）。
            let (btn_bg, btn_fg) = if self.clear_pressed {
                (t.subtle_fill_tertiary, t.text_tertiary)
            } else if self.clear_hovered {
                (t.subtle_fill_secondary, t.text_secondary)
            } else {
                (t.control_fill_transparent, t.text_secondary)
            };
            ctx.painter.fill_rounded_rect(br, CORNER, btn_bg);
            // ✕ 图标 12×12，居中（viewBox 0 0 12 12，currentColor）。
            let ic = Rect {
                x: br.x + (br.w - BTN_ICON) / 2.0,
                y: br.y + (br.h - BTN_ICON) / 2.0,
                w: BTN_ICON,
                h: BTN_ICON,
            };
            draw_clear_glyph(ctx, ic, btn_fg);
        }
    }

    fn paint_overlay(&mut self, ctx: &mut PaintCtx) {
        if !self.ctx_open {
            return;
        }
        let t = ctx.tokens;
        let m = self.menu_rect();
        ctx.painter.fill_rounded_rect(Rect { y: m.y + 2.0, ..m }, 8.0, Color::hex("#30000000"));
        ctx.painter.fill_rounded_rect(m, 8.0, t.solid_bg_tertiary);
        ctx.painter.stroke_inner(m, 8.0, t.surface_stroke_flyout, 1.0);
        for i in 0..MENU_ITEMS.len() {
            let ir = self.menu_item_rect(i);
            let enabled = match i {
                0 | 1 => self.sel_range().is_some(),
                _ => true,
            };
            if self.ctx_hover == Some(i) && enabled {
                ctx.painter.fill_rounded_rect(Rect { x: ir.x + 4.0, y: ir.y + 1.0, w: ir.w - 8.0, h: ir.h - 2.0 }, 4.0, t.subtle_fill_secondary);
            }
            let fg = if enabled { t.text_primary } else { t.text_disabled };
            let tr = Rect { x: ir.x + 12.0, y: ir.y, w: ir.w - 20.0, h: ir.h };
            let _ = ctx.painter.draw_text_leading(MENU_ITEMS[i], TextStyle::BODY, tr, fg);
        }
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if !self.enabled {
            return EventResult::NONE;
        }
        // 右键菜单打开时优先处理
        if self.ctx_open {
            match ev {
                InputEvent::PointerMove(p) => {
                    let h = (0..MENU_ITEMS.len()).find(|&i| self.menu_item_rect(i).contains(p));
                    if h != self.ctx_hover {
                        self.ctx_hover = h;
                        return EventResult::REDRAW;
                    }
                    return EventResult::NONE;
                }
                InputEvent::PointerDown(p) => {
                    if !self.menu_rect().contains(p) {
                        self.ctx_open = false;
                        return EventResult::REDRAW;
                    }
                    return EventResult::NONE;
                }
                InputEvent::PointerUp(p) => {
                    if let Some(i) = (0..MENU_ITEMS.len()).find(|&i| self.menu_item_rect(i).contains(p)) {
                        let enabled = matches!(i, 2 | 3) || self.sel_range().is_some();
                        if enabled {
                            self.do_action(i);
                        }
                        self.ctx_open = false;
                        self.focus_time = now;
                        return EventResult::REDRAW;
                    }
                    return EventResult::NONE;
                }
                _ => return EventResult::NONE,
            }
        }

        let mut redraw = false;
        match ev {
            InputEvent::ContextMenu(p) => {
                if self.rect.contains(p) {
                    self.focused = true;
                    self.focus_time = now;
                    self.ctx_open = true;
                    self.ctx_pos = p;
                    self.ctx_hover = None;
                    redraw = true;
                }
            }
            InputEvent::PointerMove(p) => {
                let h = self.rect.contains(p);
                if h != self.hovered {
                    self.hovered = h;
                    redraw = true;
                }
                // 清除按钮 hover（仅当其可见时）。
                let ch = self.clear_visible() && self.clear_rect().contains(p);
                if ch != self.clear_hovered {
                    self.clear_hovered = ch;
                    redraw = true;
                }
                if !ch && self.clear_pressed {
                    self.clear_pressed = false;
                    redraw = true;
                }
            }
            InputEvent::PointerLeave => {
                if self.hovered || self.clear_hovered || self.clear_pressed {
                    self.hovered = false;
                    self.clear_hovered = false;
                    self.clear_pressed = false;
                    redraw = true;
                }
            }
            InputEvent::PointerDown(p) => {
                // 先看是否按在清除按钮上（focus-within 时可见）。
                if self.clear_visible() && self.clear_rect().contains(p) {
                    self.clear_pressed = true;
                    self.focus_time = now;
                    redraw = true;
                } else {
                    let inside = self.rect.contains(p);
                    if inside != self.focused {
                        self.focused = inside;
                        redraw = true;
                    }
                    if inside {
                        self.caret = self.char_len();
                        self.sel_anchor = None;
                        self.focus_time = now;
                        redraw = true;
                    } else {
                        // 点击框外：清除按钮随焦点消失。
                        self.clear_pressed = false;
                        self.clear_hovered = false;
                    }
                }
            }
            InputEvent::PointerUp(p) => {
                if self.clear_pressed {
                    // 抬起仍在按钮内 → 执行清除（svelte handleClear：清空 + 聚焦）。
                    if self.clear_rect().contains(p) {
                        self.text.clear();
                        self.caret = 0;
                        self.sel_anchor = None;
                        self.focused = true;
                        self.focus_time = now;
                    }
                    self.clear_pressed = false;
                    redraw = true;
                }
            }
            InputEvent::Char(c) => {
                if self.focused && !c.is_control() {
                    self.insert_str(&c.to_string());
                    self.focus_time = now;
                    redraw = true;
                }
            }
            InputEvent::KeyDown(vk) if self.focused => {
                match vk {
                    0x08 => {
                        if !self.delete_selection() && self.caret > 0 {
                            let start = byte_index(&self.text, self.caret - 1);
                            let end = byte_index(&self.text, self.caret);
                            self.text.replace_range(start..end, "");
                            self.caret -= 1;
                        }
                        redraw = true;
                    }
                    0x2E => {
                        if !self.delete_selection() {
                            let n = self.char_len();
                            if self.caret < n {
                                let start = byte_index(&self.text, self.caret);
                                let end = byte_index(&self.text, self.caret + 1);
                                self.text.replace_range(start..end, "");
                            }
                        }
                        redraw = true;
                    }
                    0x25 => { self.sel_anchor = None; if self.caret > 0 { self.caret -= 1; } redraw = true; }
                    0x27 => { self.sel_anchor = None; if self.caret < self.char_len() { self.caret += 1; } redraw = true; }
                    0x24 => { self.sel_anchor = None; self.caret = 0; redraw = true; }
                    0x23 => { self.sel_anchor = None; self.caret = self.char_len(); redraw = true; }
                    0x41 => { /* Ctrl+A 由 KeyDown 无修饰位，简化：用菜单全选 */ }
                    _ => {}
                }
                self.focus_time = now;
            }
            _ => {}
        }
        EventResult { redraw, animating: false }
    }

    fn is_animating(&self, _now: f64) -> bool {
        // 状态切换为 0ms 瞬切（SCSS 无 transition）；唯一持续重绘是光标方波闪烁。
        self.focused && self.sel_range().is_none()
    }

    fn wants_keyboard(&self) -> bool {
        self.focused
    }

    fn cursor_at(&self, p: Point) -> Cursor {
        // 框内（不含清除按钮）显示 I 形文本光标；清除按钮上仍用箭头。
        if self.enabled && self.rect.contains(p) && !(self.clear_visible() && self.clear_rect().contains(p)) {
            Cursor::Text
        } else {
            Cursor::Default
        }
    }

    fn caret_pos(&self) -> Option<Point> {
        if self.focused {
            Some(self.caret_px)
        } else {
            None
        }
    }

    fn set_composition(&mut self, s: &str) {
        if self.composition != s {
            self.composition.clear();
            self.composition.push_str(s);
        }
    }

    fn wants_modal(&self) -> bool {
        self.ctx_open
    }

    fn accessible_role(&self) -> AccessibleRole {
        // TextBox 的 UIA 控件类型应为 **Edit**；共享枚举 `AccessibleRole` 暂无该变体
        //（见 sharedNeeds），用最接近的输入类 `ComboBox` 占位。
        AccessibleRole::ComboBox
    }
    fn accessible_name(&self) -> String {
        self.text.clone()
    }
}

/// 绘制清除按钮的 ✕ 图标（fluent-svelte 内联 SVG，viewBox 0 0 12 12）。
/// 用两道圆头细线近似官方路径（无字体依赖，避免缺字方块）。线宽约 1px@12。
fn draw_clear_glyph(ctx: &mut PaintCtx, r: Rect, color: Color) {
    // viewBox 0..12 → r 的映射；官方 ✕ 两笔约从 2 到 10。
    let m = |u: f32, v: f32| (r.x + u / 12.0 * r.w, r.y + v / 12.0 * r.h);
    let w = (r.w / 12.0 * 1.1).max(1.0);
    ctx.painter.stroke_polyline(&[m(2.5, 2.5), m(9.5, 9.5)], color, w);
    ctx.painter.stroke_polyline(&[m(9.5, 2.5), m(2.5, 9.5)], color, w);
}

/// 字符索引 → 字节索引（UTF-8 安全编辑）。
fn byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
}
