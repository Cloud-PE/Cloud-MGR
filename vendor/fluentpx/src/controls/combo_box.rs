//! ComboBox：闭合态 + 展开弹出列表。1:1 取自 WinUI `controls/dev/ComboBox/*`。
//!
//! 关键行为（非可编辑 ComboBox / CarouselPanel 居中选中）：
//! * 弹窗**纵向重定位**，使「选中项」正好压在闭合框上（经典居中选中）。
//! * 开：**SplitOpenThemeAnimation** —— 以「选中项中心（=闭合框中心）」为中点，裁剪带从
//!   弹窗 50% 高对开揭示到满，缓动 cubic-bezier(0,0,0,1)，250ms；弹窗本体**不做透明度渐变**
//!   （WinUI 的 OverlayOpening 故事板没绑定目标、是废弃资源）。**关闭无动画**（{#if open} 直接移除）。
//! 几何对齐 fluent-svelte（ComboBox.scss / ComboBoxItem.scss / Button.scss）：
//! * 闭合框（Button standard）：高 32、padding-inline 11、圆角 4、border 1px；
//!   chevron 12×12、右距 11（按钮内边距）、margin-inline-start 8；内容色 text-primary，
//!   :active→text-secondary（含 chevron），disabled→text-disabled。
//! * 弹窗：圆角 8（overlay）、padding 1、宽 100%+8、左移 5、上移 6、背景
//!   solid-background-quarternary（浅#FFFFFF/深#2C2C2C）、边框 surface-stroke-flyout 1px、
//!   投影 flyout-shadow 经 shadow-in（延迟 250ms 后 250ms 淡入）。
//! * item：可见盒 32、margin 4（行距 40）、圆角 4、padding-inline 11；hover/selected=subtle
//!   secondary、:active=subtle tertiary（文字 secondary）；选中 pill 3×16 圆角 3 强调色，
//!   :active scaleY(0.625)。
//!
//! 限制：不可编辑；长列表暂不做 7-per-side 滚动（本工程下拉项很少）。

use crate::anim::{cubic_bezier, lerp, ColorTransition};
use crate::color::Color;
use crate::gfx::Icon;
use crate::typography::TextStyle;
use crate::widget::*;

/// WinUI ControlFastOutSlowInKeySpline = (0,0,0,1)，用于 Split 裁剪揭示。
fn split_ease(t: f32) -> f32 {
    cubic_bezier(0.0, 0.0, 0.0, 1.0, t.clamp(0.0, 1.0))
}

const BOX_H: f32 = 32.0;
const CORNER: f32 = 4.0; // --control-corner-radius (theme.css:32)
const BORDER: f32 = 1.0; // Button.scss:24 border: 1px solid
const BOX_PAD_L: f32 = 11.0; // Button.scss:10 padding-inline: 11px
const CHEVRON_SIZE: f32 = 12.0; // ComboBox.scss:108-109 combo-box-icon 12x12
const CHEVRON_RIGHT: f32 = 11.0; // Button.scss:10 padding-inline: 11px (icon右边距=按钮内边距)
const POPUP_CORNER: f32 = 8.0; // --overlay-corner-radius (theme.css:33)
const POPUP_PAD: f32 = 1.0; // ComboBox.scss:119 dropdown padding: 1px
const POPUP_MARGIN_TOP: f32 = -6.0; // ComboBox.scss:117 margin-block-start: -6px
const POPUP_MARGIN_LEFT: f32 = -5.0; // ComboBox.scss:118 margin-inline-start: -5px
const POPUP_EXTRA_W: f32 = 8.0; // ComboBox.scss:129 inline-size: calc(100% + 8px)
const ITEM_BOX_H: f32 = 32.0; // ComboBoxItem.scss:19 block-size: 32px
const ITEM_MARGIN: f32 = 4.0; // ComboBoxItem.scss:11 margin: 4px (四周)
const ITEM_PITCH: f32 = ITEM_BOX_H + ITEM_MARGIN * 2.0; // 相邻 flex 项 margin 不折叠 → 行距 40
const ITEM_CORNER: f32 = 4.0; // ComboBoxItem.scss:13 border-radius: --control-corner-radius (4px)
const ITEM_PAD_L: f32 = 11.0; // ComboBoxItem.scss:12 padding: 0 11px
const PILL_W: f32 = 3.0; // ComboBoxItem.scss:29 inline-size: 3px
const PILL_H: f32 = 16.0; // ComboBoxItem.scss:65 selected block-size: 16px
const PILL_CORNER: f32 = 3.0; // ComboBoxItem.scss:24 border-radius: 3px
const PILL_PRESS_SCALE_Y: f32 = 0.625; // ComboBoxItem.scss:47 :active &::before scaleY(0.625)
const OPEN_DUR: f64 = 0.25; // menu-in --control-normal-duration 250ms (ComboBox.scss:125)
const SHADOW_DUR: f64 = 0.25; // shadow-in --control-normal-duration 250ms (ComboBox.scss:126)
const SHADOW_DELAY: f64 = 0.25; // shadow-in delay = --control-normal-duration (ComboBox.scss:127)
const BG_DUR: f64 = 0.083; // Button.scss:16 transition: --control-faster-duration ease background

pub struct ComboBox {
    pub items: Vec<String>,
    pub selected: usize,
    pub open: bool,
    pub enabled: bool,
    hovered_box: bool,
    pressed_box: bool,
    hovered_item: Option<usize>,
    /// 当前按住（:active）的下拉项，用于背景=tertiary、文字=secondary、pill scaleY(0.625)。
    pressed_item: Option<usize>,
    rect: Rect,
    /// 重定位 + 视口夹取后的弹窗矩形（每帧在 paint 计算并缓存，供事件命中复用）。
    popup_cached: Rect,
    open_start: f64,
    /// 闭合框底色的悬停/按下过渡（83ms）。
    bg_anim: ColorTransition,
    bg_init: bool,
}

impl ComboBox {
    pub fn new(items: Vec<String>, selected: usize) -> ComboBox {
        ComboBox {
            items,
            selected,
            open: false,
            enabled: true,
            hovered_box: false,
            pressed_box: false,
            hovered_item: None,
            pressed_item: None,
            rect: Rect::default(),
            popup_cached: Rect::default(),
            open_start: 0.0,
            bg_anim: ColorTransition::instant(Color::TRANSPARENT),
            bg_init: false,
        }
    }

    /// 计算弹窗矩形：让选中项压在闭合框上，并夹取进视口。
    /// 几何来自 ComboBox.scss：宽 = 100%+8（:129），左移 5（:118），上移 6（:117），padding 1（:119）。
    fn compute_popup(&self, viewport: Size) -> Rect {
        let n = self.items.len().max(1) as f32;
        let h = n * ITEM_PITCH + POPUP_PAD * 2.0;
        let w = self.rect.w + POPUP_EXTRA_W;
        let x = self.rect.x + POPUP_MARGIN_LEFT;
        // 选中项「可见盒」中心 = 闭合框中心；再叠加 margin-block-start: -6（carousel 重定位之上）。
        // 选中项可见盒中心相对 popup.y 的偏移：POPUP_PAD + i*PITCH + ITEM_MARGIN + ITEM_BOX_H/2。
        let sel_center_off = POPUP_PAD + self.selected as f32 * ITEM_PITCH + ITEM_MARGIN + ITEM_BOX_H / 2.0;
        let ideal_y = self.rect.center_y() - sel_center_off + POPUP_MARGIN_TOP;
        let max_y = (viewport.h - h - 4.0).max(4.0);
        let y = ideal_y.clamp(4.0, max_y);
        Rect { x, y, w, h }
    }

    fn popup_rect(&self) -> Rect {
        self.popup_cached
    }

    /// 整行命中区（含 4px margin），行距 40。
    fn popup_item_rect(&self, i: usize) -> Rect {
        let p = self.popup_cached;
        Rect { x: p.x, y: p.y + POPUP_PAD + i as f32 * ITEM_PITCH, w: p.w, h: ITEM_PITCH }
    }

    /// 可见项盒（32px，四周内缩 4px margin）：背景/pill/文字基于此。
    fn popup_item_box(&self, i: usize) -> Rect {
        let r = self.popup_item_rect(i);
        Rect { x: r.x + ITEM_MARGIN, y: r.y + ITEM_MARGIN, w: (r.w - ITEM_MARGIN * 2.0).max(0.0), h: ITEM_BOX_H }
    }

    fn item_at(&self, pt: Point) -> Option<usize> {
        if !self.popup_rect().contains(pt) {
            return None;
        }
        (0..self.items.len()).find(|&i| self.popup_item_rect(i).contains(pt))
    }

    fn close_now(&mut self, _now: f64) {
        // 关闭=**瞬间移除**弹窗，无收缩动画：fluent-svelte（{#if open} 直接移除）与 WinUI 都没有
        // 退场过渡，只有展开揭示动画。用户多次明确要求砍掉关闭动画。
        self.open = false;
        self.hovered_item = None;
        self.pressed_item = None;
    }
}

impl Widget for ComboBox {
    fn measure(&mut self, available: Size) -> Size {
        Size { w: available.w.clamp(200.0, 300.0), h: BOX_H }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = Rect { x: rect.x, y: rect.y, w: rect.w, h: BOX_H };
    }

    fn hit_test(&self, p: Point) -> bool {
        self.rect.contains(p) || (self.open && self.popup_rect().contains(p))
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        // 仅**打开时**重算弹窗位置；关闭动画期间冻结，避免选中项变化导致弹窗跳位（破坏收拢动画）。
        if self.open {
            self.popup_cached = self.compute_popup(ctx.viewport);
        }

        let t = ctx.tokens;
        let r = self.rect;
        // 底色：用户指定 #373737（深）/#FEFEFE（浅），hover/pressed 叠加细微深浅，83ms 平滑过渡。
        let target_bg = if !self.enabled {
            t.input_solid_bg()
        } else if self.pressed_box || self.open {
            t.input_bg_pressed()
        } else if self.hovered_box {
            t.input_bg_hover()
        } else {
            t.input_solid_bg()
        };
        if !self.bg_init {
            self.bg_anim = ColorTransition::instant(target_bg);
            self.bg_init = true;
        } else {
            self.bg_anim.retarget(target_bg, ctx.now, BG_DUR);
        }
        let bg = self.bg_anim.value(ctx.now);
        ctx.painter.fill_rounded_rect(r.inset(BORDER), (CORNER - BORDER).max(0.0), bg);
        // 边框：整圈 input_border（深 #414141）+ 底边 input_border_bottom（深 #3A3A3A）。
        ctx.painter.stroke_inner(r, CORNER, t.input_border(), BORDER);
        let bl = Rect { x: r.x + CORNER, y: r.bottom() - BORDER, w: (r.w - CORNER * 2.0).max(0.0), h: BORDER };
        ctx.painter.fill_rect(bl, t.input_border_bottom());

        // Button 内容色（标签 + chevron 同色 currentColor）：rest=text-primary（Button.scss:27）、
        // :active=text-secondary（Button.scss:37）、disabled=text-disabled（Button.scss:43）。
        // 用户未自定义文字色，保留 fluent-svelte 状态逻辑。
        let fg = if !self.enabled {
            t.text_disabled
        } else if self.pressed_box {
            t.text_secondary
        } else {
            t.text_primary
        };
        // chevron margin-inline-start: 8（ComboBox.scss:107）→ 标签右界 = chevron 左界 - 8。
        let chevron_left = r.right() - CHEVRON_RIGHT - CHEVRON_SIZE;
        let label_right = chevron_left - 8.0;
        let text_rect = Rect { x: r.x + BOX_PAD_L, y: r.y, w: (label_right - (r.x + BOX_PAD_L)).max(0.0), h: r.h };
        if let Some(s) = self.items.get(self.selected) {
            let _ = ctx.painter.draw_text_leading(s, TextStyle::BODY, text_rect, fg);
        }

        let gly = Rect {
            x: chevron_left,
            y: r.center_y() - CHEVRON_SIZE / 2.0,
            w: CHEVRON_SIZE,
            h: CHEVRON_SIZE,
        };
        ctx.painter.draw_glyph(Icon::ChevronDown, gly, fg);
    }

    fn paint_overlay(&mut self, ctx: &mut PaintCtx) {
        if !self.open {
            return; // 关闭即移除，无收缩动画
        }
        let now = ctx.now;
        let p = self.popup_cached;
        // SplitOpen 几何：裁剪带以「选中项中心（=闭合框中心）」为中点对开揭示。
        // ScaleY 0.5↔full（full 保证覆盖整窗格），即可见半高 from H/4 → H/2+|off|。
        let box_center = self.rect.center_y();
        let off_abs = (box_center - p.center_y()).abs();
        let half_full = p.h / 2.0 + off_abs;

        // 弹窗本身**不做透明度渐变**（真·WinUI 没有；那是 light-dismiss 遮罩的事）。
        // 仅靠裁剪带的伸/缩揭示与收拢。alpha 恒为 1。
        let alpha = 1.0_f32;
        // 仅展开揭示（上面已 return 保证 self.open）；关闭无动画。
        let vh = {
            let t = ((now - self.open_start) / OPEN_DUR).clamp(0.0, 1.0) as f32;
            lerp(p.h / 4.0, half_full, split_ease(t))
        };
        let top = (box_center - vh).max(p.y);
        let bot = (box_center + vh).min(p.bottom());
        if bot - top < 1.0 {
            return;
        }
        let t = ctx.tokens;
        // 弹窗投影（真·D2D 高斯）：shadow-in = 延迟 SHADOW_DELAY 后 SHADOW_DUR 内淡入（ComboBox.scss:126-127）。
        // 关闭时随裁剪带一并消失（不单独反向淡出）。在 push_clip **之前**画（阴影需溢出弹窗矩形）。
        if self.open {
            let st = (((now - self.open_start) - SHADOW_DELAY) / SHADOW_DUR).clamp(0.0, 1.0) as f32;
            if st > 0.0 {
                let (off_y, blur, color) = t.flyout_shadow();
                ctx.painter.drop_shadow(p, POPUP_CORNER, off_y, blur, color.with_opacity(split_ease(st)));
            }
        }
        ctx.painter.push_clip(Rect { x: p.x - 1.0, y: top, w: p.w + 2.0, h: bot - top });

        // 背景 = --solid-background-quarternary（theme.css:180/346：浅 #FFFFFF / 深 #2C2C2C），
        // 1px --surface-stroke-flyout 边框（ComboBox.scss:120,122）。
        ctx.painter.fill_rounded_rect(p, POPUP_CORNER, t.solid_bg_quarternary);
        ctx.painter.stroke_inner(p, POPUP_CORNER, t.surface_stroke_flyout, 1.0);

        for i in 0..self.items.len() {
            let ib = self.popup_item_box(i); // 可见 32px 项盒
            let selected = i == self.selected;
            let hovered = self.hovered_item == Some(i);
            let pressed = self.pressed_item == Some(i);
            // 背景（ComboBoxItem.scss:14,38-39,42-44,51-60）：
            // base=transparent；hover/selected=secondary；:active(pressed)=tertiary；
            // disabled=transparent（disabled+selected=secondary）。
            let item_bg = if !self.enabled {
                if selected { t.subtle_fill_secondary } else { Color::TRANSPARENT }
            } else if pressed {
                t.subtle_fill_tertiary
            } else if hovered || selected {
                t.subtle_fill_secondary
            } else {
                Color::TRANSPARENT
            };
            if item_bg.a != 0 {
                ctx.painter.fill_rounded_rect(ib, ITEM_CORNER, item_bg.with_opacity(alpha));
            }
            // 选中指示 pill：左贴项盒左缘（inset-inline-start:0），3×16 圆角 3，强调色；
            // :active 时 scaleY(0.625)（绕中心）。disabled+selected 用 accent-disabled。
            if selected {
                let scale_y = if pressed { PILL_PRESS_SCALE_Y } else { 1.0 };
                let ph = PILL_H * scale_y;
                let pill = Rect { x: ib.x, y: ib.center_y() - ph / 2.0, w: PILL_W, h: ph };
                let pill_color = if !self.enabled { t.accent_fill_disabled } else { t.accent_fill_default() };
                ctx.painter.fill_rounded_rect(pill, PILL_CORNER, pill_color.with_opacity(alpha));
            }
            // 文字（ComboBoxItem.scss:12,15,44,53）：左 padding 11；色 primary，
            // :active=secondary，disabled=disabled。
            let item_fg = if !self.enabled {
                t.text_disabled
            } else if pressed {
                t.text_secondary
            } else {
                t.text_primary
            };
            let tr = Rect { x: ib.x + ITEM_PAD_L, y: ib.y, w: (ib.w - ITEM_PAD_L * 2.0).max(0.0), h: ib.h };
            let _ = ctx.painter.draw_text_leading(&self.items[i], TextStyle::BODY, tr, item_fg.with_opacity(alpha));
        }
        ctx.painter.pop_clip();
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if !self.enabled {
            return EventResult::NONE;
        }
        let mut redraw = false;
        match ev {
            InputEvent::PointerMove(pt) => {
                let hb = self.rect.contains(pt);
                if hb != self.hovered_box {
                    self.hovered_box = hb;
                    redraw = true;
                }
                if self.open {
                    let hi = self.item_at(pt);
                    if hi != self.hovered_item {
                        self.hovered_item = hi;
                        redraw = true;
                    }
                    // 拖出按住项 → 取消 :active（按住移回才恢复）。
                    if self.pressed_item.is_some() && self.pressed_item != hi {
                        self.pressed_item = None;
                        redraw = true;
                    }
                }
            }
            InputEvent::PointerLeave => {
                self.hovered_box = false;
                self.hovered_item = None;
                self.pressed_item = None;
                redraw = true;
            }
            InputEvent::PointerDown(pt) => {
                if self.rect.contains(pt) {
                    self.pressed_box = true;
                    redraw = true;
                } else if self.open {
                    if let Some(i) = self.item_at(pt) {
                        // 项 :active 按下：背景 tertiary、文字 secondary、pill scaleY(0.625)。
                        self.pressed_item = Some(i);
                        redraw = true;
                    } else if !self.popup_rect().contains(pt) {
                        self.close_now(now);
                        redraw = true;
                    }
                }
            }
            InputEvent::PointerUp(pt) => {
                if self.pressed_box && self.rect.contains(pt) {
                    if self.open {
                        self.close_now(now);
                    } else {
                        self.open = true;
                        self.open_start = now;
                    }
                    redraw = true;
                } else if self.open {
                    if let Some(i) = self.item_at(pt) {
                        self.selected = i;
                        self.close_now(now);
                        redraw = true;
                    }
                }
                self.pressed_box = false;
                self.pressed_item = None;
            }
            _ => {}
        }
        EventResult { redraw, animating: self.open }
    }

    fn is_animating(&self, now: f64) -> bool {
        // 仅开启动画需要持续重绘（关闭=瞬间移除，无动画）。开启含延迟阴影淡入：
        // 持续到 open_start + SHADOW_DELAY + SHADOW_DUR（=0.5s）。
        let open_window = OPEN_DUR.max(SHADOW_DELAY + SHADOW_DUR);
        (self.open && (now - self.open_start) < open_window) || self.bg_anim.is_active(now)
    }

    fn wants_modal(&self) -> bool {
        self.open
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::ComboBox
    }
}
