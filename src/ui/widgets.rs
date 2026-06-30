//! 共享 UI 小工具：按钮点击追踪、纵向滚动、卡片/分隔/滚动条绘制。
//! fluentpx 控件不返回「点击/变化」信号，需读公共字段或自行追踪 press→release。

use crate::mode::PluginMode;
use crate::plugins::Plugin;
use fluentpx::controls::{Button, ButtonStyle, ProgressRing};
use fluentpx::gfx::Icon;
use fluentpx::typography::TextStyle;
use fluentpx::widget::*;

/// 按内容自动定宽（匹配 WinUI 按钮 = 文字宽 + 左右内边距 11+11），最小 46。
pub fn btn_width(ctx: &mut PaintCtx, label: &str) -> f32 {
    let tw = ctx.painter.measure_text(label, TextStyle::BODY).map(|s| s.w).unwrap_or(label.chars().count() as f32 * 14.0);
    (tw + 22.0).max(46.0)
}

/// 进行中的操作按钮：按钮本体变为禁用态（灰底），中间放一个小旋转环
/// （取代「旋转环放在按钮旁边」）。
pub fn loading_button(ctx: &mut PaintCtx, ring: &mut ProgressRing, r: Rect) {
    imm_button(ctx, r, "", false, false, false, false);
    let rs = (r.h - 10.0).clamp(16.0, 22.0);
    ring.arrange(Rect::new(r.center_x() - rs / 2.0, r.center_y() - rs / 2.0, rs, rs));
    ring.paint(ctx);
}

/// 可折叠分组头：标题（加粗）+ chevron（按展开度 `amt` 0→1 旋转 0°→180°，下↔上）+ 底部分隔线。
/// 调用方自行处理点击切换与展开动画。
pub fn section_head(ctx: &mut PaintCtx, r: Rect, title: &str, amt: f32) {
    let t = ctx.tokens;
    let _ = ctx.painter.draw_text_leading(title, TextStyle::BODY_STRONG, Rect::new(r.x + 4.0, r.y, r.w - 40.0, r.h), t.text_primary);
    let cc = Point { x: r.right() - 18.0, y: r.center_y() };
    ctx.painter.set_rotation_about(cc, amt * 180.0);
    let _ = ctx.painter.draw_icon(Icon::ChevronDown.codepoint(), 12.0, Rect::new(r.right() - 28.0, r.y, 20.0, r.h), t.text_secondary);
    ctx.painter.set_transform(None);
    ctx.painter.draw_line(r.x, r.bottom(), r.right(), r.bottom(), t.divider_stroke_default, 1.0);
}

/// 生成插件文件名（不含扩展名），按模式拼接字段；describe 中的非法/空白字符替换为 `_`。
pub fn generate_plugin_filename(mode: PluginMode, plugin: &Plugin) -> String {
    let safe: String = plugin
        .describe
        .chars()
        .map(|c| if matches!(c, ' ' | '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    match mode {
        PluginMode::CloudPE => format!("{}_{}_{}_{}", plugin.name, plugin.version, plugin.author, safe),
        PluginMode::HotPE => {
            if safe.is_empty() {
                format!("{}_{}_{}_{}", plugin.name, plugin.author, plugin.version, plugin.name)
            } else {
                format!("{}_{}_{}_{}", plugin.name, plugin.author, plugin.version, safe)
            }
        }
        PluginMode::Edgeless => format!("{}_{}_{}", plugin.name, plugin.version, plugin.author),
        _ => String::new(),
    }
}

/// 按钮 + 点击检测（press 进入、在按钮内 release 即点击）。
pub struct Clicky {
    pub btn: Button,
    was_pressed: bool,
}

impl Clicky {
    pub fn new(btn: Button) -> Self {
        Self { btn, was_pressed: false }
    }
    pub fn arrange(&mut self, r: Rect) {
        self.btn.arrange(r);
    }
    pub fn paint(&mut self, ctx: &mut PaintCtx) {
        self.btn.paint(ctx);
    }
    pub fn set_enabled(&mut self, e: bool) {
        self.btn.interaction.enabled = e;
    }
    /// 返回 (重绘/动画信息, 是否被点击)。
    pub fn event(&mut self, ev: InputEvent, now: f64) -> (EventResult, bool) {
        self.was_pressed = self.btn.interaction.pressed;
        let r = self.btn.on_event(ev, now);
        let clicked = if let InputEvent::PointerUp(p) = ev {
            self.was_pressed && self.btn.hit_test(p)
        } else {
            false
        };
        (r, clicked)
    }
    pub fn is_animating(&self, now: f64) -> bool {
        self.btn.is_animating(now)
    }
}

/// 纵向滚动状态。页面每帧据此把子项排布到「屏上 y = 基准 - offset」。
/// `offset` 为当前(动画中)位置；`target` 为目标位置。滚轮只设 target，
/// `tick` 每帧把 offset 平滑逼近 target（指数缓出 → 顺滑惯性手感，匹配 WinUI/网页）。
pub struct Scroll {
    pub offset: f32,
    target: f32,
    prev_now: f64,
    /// 拖拽滑块时记录的抓取点（鼠标相对滑块顶端的偏移）；None=未拖拽。
    drag: Option<f32>,
}

impl Scroll {
    pub fn new() -> Self {
        Self { offset: 0.0, target: 0.0, prev_now: -1.0, drag: None }
    }
    /// 处理滚轮（delta>0 向上）：设定**目标**位置（平滑滚动，不再瞬跳）。返回是否变化。
    pub fn apply_wheel(&mut self, delta: f32, max: f32) -> bool {
        let t = (self.target - delta * 64.0).clamp(0.0, max.max(0.0));
        let changed = (t - self.target).abs() > 0.01;
        self.target = t;
        changed
    }

    /// 每帧推进平滑滚动（指数缓出，时间常数 ~60ms）。返回是否仍在动。
    /// 拖拽时直接跟手（target 跟随 offset，不做平滑）。
    pub fn tick(&mut self, now: f64) -> bool {
        if self.prev_now < 0.0 {
            self.prev_now = now;
        }
        let dt = ((now - self.prev_now) as f32).clamp(0.0, 0.05);
        self.prev_now = now;
        if self.drag.is_some() {
            self.target = self.offset;
            return false;
        }
        let diff = self.target - self.offset;
        if diff.abs() < 0.5 {
            self.offset = self.target;
            return false;
        }
        self.offset += diff * (1.0 - (-dt * 16.0).exp());
        true
    }

    /// 是否仍在平滑滚动中（供页面 is_animating 续帧）。
    pub fn is_settling(&self) -> bool {
        self.drag.is_none() && (self.target - self.offset).abs() >= 0.5
    }

    /// 内容/视口变化后把**目标**夹回 [0,max]；offset 不硬夹——内容骤减（如卡片收起）导致
    /// offset>max 时，靠 tick 指数**平滑滚回** target，避免「收起到最后跳一下」。
    pub fn clamp(&mut self, max: f32) {
        let m = max.max(0.0);
        self.target = self.target.clamp(0.0, m);
        if self.offset < 0.0 {
            self.offset = 0.0;
        }
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// 滑块矩形 + 最大偏移（content 超出 view 时才有）。
    fn thumb(&self, view: Rect, content_h: f32) -> Option<(Rect, f32)> {
        if content_h <= view.h + 0.5 {
            return None;
        }
        let thumb_h = (view.h * view.h / content_h).max(28.0);
        let max = (content_h - view.h).max(0.001);
        let thumb_y = view.y + (view.h - thumb_h) * (self.offset / max).clamp(0.0, 1.0);
        Some((Rect::new(view.right() - 9.0, thumb_y, 5.0, thumb_h), max))
    }

    /// 处理滚动条拖拽 / 轨道点击。返回是否消费了该事件。
    pub fn scrollbar_event(&mut self, ev: InputEvent, view: Rect, content_h: f32) -> bool {
        match ev {
            InputEvent::PointerDown(p) => {
                if let Some((thumb, max)) = self.thumb(view, content_h) {
                    let grab = Rect::new(thumb.x - 6.0, thumb.y, thumb.w + 12.0, thumb.h);
                    if grab.contains(p) {
                        self.drag = Some(p.y - thumb.y);
                        return true;
                    }
                    // 点轨道空白处：直接跳到该位置并进入拖拽。
                    let track = Rect::new(thumb.x - 6.0, view.y, thumb.w + 12.0, view.h);
                    if track.contains(p) {
                        let t = ((p.y - view.y - thumb.h / 2.0) / (view.h - thumb.h)).clamp(0.0, 1.0);
                        self.offset = t * max;
                        self.target = self.offset;
                        self.drag = Some(thumb.h / 2.0);
                        return true;
                    }
                }
                false
            }
            InputEvent::PointerMove(p) => {
                if let Some(grab) = self.drag {
                    if let Some((thumb, max)) = self.thumb(view, content_h) {
                        let t = ((p.y - grab - view.y) / (view.h - thumb.h)).clamp(0.0, 1.0);
                        self.offset = t * max;
                        self.target = self.offset;
                    }
                    return true;
                }
                false
            }
            InputEvent::PointerUp(_) | InputEvent::PointerLeave => {
                if self.drag.take().is_some() {
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

/// 卡片背景（圆角填充 + CardStroke 实色描边，与真·WinUI 一致）。圆角 4 = ControlCornerRadius。
pub fn draw_card(ctx: &mut PaintCtx, r: Rect) {
    let t = ctx.tokens;
    // 卡片不画分层投影（大面元上会糊成带状黑边）；靠描边定义边界，与真·WinUI 极淡卡片阴影观感一致。
    ctx.painter.fill_rounded_rect(r, 4.0, t.card_bg_default);
    let stroke = t.card_stroke_default.over(t.solid_bg_base);
    ctx.painter.stroke_inner(r, 4.0, stroke, 1.0);
}

/// 顶部标题 + 细分隔线（页头）。返回分隔线下方 y。
pub fn page_header(ctx: &mut PaintCtx, area: Rect, title: &str) -> f32 {
    let t = ctx.tokens;
    let _ = ctx.painter.draw_text_leading(title, TextStyle::TITLE, Rect::new(area.x, area.y, area.w, 40.0), t.text_primary);
    let y = area.y + 48.0;
    let stroke = t.divider_stroke_default;
    ctx.painter.draw_line(area.x, y, area.right(), y, stroke, 1.0);
    y + 12.0
}

/// Fluent 空状态：在区域内**居中**绘制一个大号弱化图标 + 一行说明文字
/// （WinUI 没有单独的 Empty 控件，这是其推荐的空状态版式）。
pub fn empty_state(ctx: &mut PaintCtx, area: Rect, glyph: char, text: &str) {
    let t = ctx.tokens;
    let isz = 48.0;
    let cy = area.y + (area.h * 0.34).max(40.0);
    let _ = ctx.painter.draw_icon(glyph, isz, Rect::new(area.x, cy, area.w, isz), t.text_tertiary);
    let _ = ctx.painter.draw_text_centered(text, TextStyle::BODY, Rect::new(area.x, cy + isz + 14.0, area.w, 24.0), t.text_secondary);
}

/// 即时模式按钮（用于动态列表里的操作按钮，无法为每项预留 Clicky）。
/// 由调用方维护 hover/pressed 状态与点击命中（见各页 hotspots 机制）。
pub fn imm_button(ctx: &mut PaintCtx, r: Rect, text: &str, accent: bool, hovered: bool, pressed: bool, enabled: bool) {
    // 直接复用真·fluentpx Button 渲染（含 ControlElevationBorder 渐变边框、正确内边距），
    // 与单独的 Button 控件逐像素一致；仅少了 hover 的 83ms 颜色过渡（每帧重建临时控件）。
    let mut b = Button::new(text, if accent { ButtonStyle::Accent } else { ButtonStyle::Standard });
    b.interaction = Interaction { hovered, pressed, focused: false, enabled };
    b.arrange(r);
    b.paint(ctx);
}

/// 在内容区右侧画滚动条滑块（仅 content 超出 view 时）。
pub fn draw_scrollbar(ctx: &mut PaintCtx, scroll: &Scroll, view: Rect, content_h: f32) {
    if let Some((thumb, _)) = scroll.thumb(view, content_h) {
        let t = ctx.tokens;
        let col = if scroll.dragging() { t.text_secondary } else { t.text_tertiary };
        ctx.painter.fill_rounded_rect(thumb, thumb.w / 2.0, col);
    }
}
