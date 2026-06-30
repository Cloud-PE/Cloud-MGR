//! NavigationView（左侧导航菜单）：可展开/收缩窗格 + 汉堡按钮 + 选中指示条 + 窗格滑动动画。
//!
//! 真值参考：`controls/dev/NavigationView/*`、`controls/dev/SplitView/*`、CommonStyles。
//! * 窗格 OpenPaneLength 320 / CompactPaneLength 48（NavigationView_themeresources.xaml:208 +
//!   SplitView_themeresources.xaml:5；本工程取源码默认 320）。
//! * 顶部 NavigationViewToggleButton（汉堡 `\u{E700}`，PaneToggleButtonHeight 36 / Width 40，
//!   FontSize 16；NavigationView_themeresources.xaml:205-206,271）。
//! * NavigationViewItem：MinHeight 36（NavigationViewItemOnLeftMinHeight），LayoutRoot 边距 4,2
//!   （NavigationViewItemButtonMargin），圆角 8（OverlayCornerRadius）；状态背景为即时切换的
//!   Setter（无淡入淡出，NavigationView_themeresources.xaml:454-504）。
//! * 选中指示条：3×16、圆角 2（NavigationViewSelectionIndicator{Width,Height,Radius}），
//!   AccentFillColorDefaultBrush，居左对齐于 LayoutRoot 左缘。
//! 为在画廊里演示，整体放在一个带边框的盒子里（左窗格 + 右内容区）。

use crate::anim::{cubic_bezier, lerp, ColorTransition};
use crate::color::Color;
use crate::gfx::Icon;
use crate::typography::TextStyle;
use crate::widget::*;

// —— 窗格几何（源码真值）——
const OPEN_W: f32 = 320.0; // SplitViewOpenPaneThemeLength（SplitView_themeresources.xaml:5）
const COMPACT_W: f32 = 48.0; // NavigationViewCompactPaneLength（NavigationView_themeresources.xaml:208）
const TOGGLE_H: f32 = 44.0; // PaneToggleButtonHeight 36 + ButtonHolderGrid 0,4 → 44 行
const ITEM_H: f32 = 36.0; // NavigationViewItemOnLeftMinHeight（NavigationView_themeresources.xaml:217）
const ITEM_CORNER: f32 = 4.0; // 选中/悬停高亮圆角（用户要求不要太圆润，取 ControlCornerRadius 4）
const ITEM_BG_DUR: f64 = 0.083; // 导航项底色悬停/选中过渡 83ms
const ITEM_MARGIN_L: f32 = 4.0; // NavigationViewItemButtonMargin 左 4（NavigationView_themeresources.xaml:228）
const ITEM_MARGIN_V: f32 = 2.0; // NavigationViewItemButtonMargin 上下 2
const ICON_CX: f32 = 24.0; // 图标中心相对窗格左缘（compact 48 时居中）
const LABEL_X: f32 = 48.0;
// 窗格开合：SplitView ClosedCompactLeft↔OpenInlineLeft，PaneTransform/Clip 用 OpenDuration 200ms、
// KeySpline 0.0,0.35 0.15,1.0（SplitView_themeresources.xaml:343,355-381 + :10）。
const ANIM_DUR: f64 = 0.2;
// 窗格滑动缓动 KeySpline（SplitView_themeresources.xaml:343）。
fn pane_ease(t: f32) -> f32 {
    cubic_bezier(0.0, 0.35, 0.15, 1.0, t)
}
// 页面（.navigation-view-page）几何 —— fluent-svelte NavigationView.svelte:69-80。
//   border-start-start-radius: var(--overlay-corner-radius) = 8px（仅左上角圆角）；
//   border: 1px solid var(--card-stroke-default)，且 border-block-end/inline-end:none（仅上+左边）；
//   padding-inline: 56px; padding-block: 44px。
const PAGE_CORNER: f32 = 8.0; // --fds-overlay-corner-radius（theme.css:33）
const PAGE_PAD_INLINE: f32 = 56.0; // .navigation-view-page padding-inline
const PAGE_PAD_BLOCK: f32 = 44.0; // .navigation-view-page padding-block
const IND_W: f32 = 3.0; // NavigationViewSelectionIndicatorWidth
const IND_H: f32 = 16.0; // NavigationViewSelectionIndicatorHeight
const IND_RADIUS: f32 = 2.0; // NavigationViewSelectionIndicatorRadius（NavigationView_themeresources.xaml:222）
// 选中指示条动画（源码 PlayIndicatorAnimations，NavigationView.cpp:2207-2216）：
// Offset.Y/Scale.Y 总 600ms，前缘 0.333 到位（StepEasing 单帧）、后缘 frame2 缓出。
const SEL_DUR: f64 = 0.6;
// 页面进入动画（宿主 Frame 的 EntranceThemeTransition，非 NavigationView 模板）：下移淡入，
// ~300ms、KeySpline 0.1,0.9 0.2,1.0。
const PAGE_DUR: f64 = 0.3;
const PAGE_OFFSET: f32 = 16.0;
// EntranceThemeTransition 缓动。
fn page_ease(t: f32) -> f32 {
    cubic_bezier(0.1, 0.9, 0.2, 1.0, t)
}

pub struct NavItem {
    pub icon: Icon,
    pub label: String,
}

pub struct NavigationView {
    pub items: Vec<NavItem>,
    pub selected: usize,
    pub expanded: bool,
    hovered: Option<i32>, // -1 = toggle, >=0 = item
    pressed: Option<i32>, // 当前按下的目标（-1=toggle，>=0=item），用于 Pressed 视觉态
    rect: Rect,
    anim_from: f32,
    anim_to: f32,
    anim_start: f64,
    // 选中指示条 + 页面切换动画
    prev_selected: usize,
    sel_start: f64,
    /// 每个导航项底色的悬停/选中过渡（83ms 平滑，匹配网页 hover 渐变）。
    item_bg: Vec<ColorTransition>,
    bg_init: bool,
    /// 应用外壳模式：铺满整个 rect、无卡片边框，内容区交给宿主应用绘制。
    pub app_shell: bool,
    /// 是否绘制内置演示内容（gallery 用 true；应用外壳用 false 由宿主填内容）。
    pub show_demo_content: bool,
}

impl NavigationView {
    pub fn new(items: Vec<NavItem>, selected: usize, expanded: bool) -> NavigationView {
        let w = if expanded { 1.0 } else { 0.0 };
        let n = items.len();
        NavigationView {
            items,
            selected,
            expanded,
            hovered: None,
            pressed: None,
            rect: Rect::default(),
            anim_from: w,
            anim_to: w,
            anim_start: -1.0,
            prev_selected: selected,
            sel_start: -1.0,
            item_bg: vec![ColorTransition::instant(Color::TRANSPARENT); n],
            bg_init: false,
            app_shell: false,
            show_demo_content: true,
        }
    }

    /// 作为应用外壳：铺满窗口、内容区由宿主填充。默认**收起（紧凑）**，点汉堡展开。
    pub fn shell(items: Vec<NavItem>, selected: usize) -> NavigationView {
        let mut n = NavigationView::new(items, selected, false);
        n.app_shell = true;
        n.show_demo_content = false;
        n
    }

    /// 当前内容区矩形（窗格右侧），供应用外壳模式下宿主绘制页面。
    pub fn content_area(&self, now: f64) -> Rect {
        let pane_w = self.pane_w(now);
        Rect { x: self.rect.x + pane_w + 1.0, y: self.rect.y, w: (self.rect.w - pane_w - 1.0).max(0.0), h: self.rect.h }
    }

    /// 默认演示项。
    pub fn demo() -> NavigationView {
        NavigationView::new(
            vec![
                NavItem { icon: Icon::Home, label: "主页".into() },
                NavItem { icon: Icon::Folder, label: "文件夹".into() },
                NavItem { icon: Icon::Star, label: "收藏".into() },
                NavItem { icon: Icon::Settings, label: "设置".into() },
            ],
            0,
            true,
        )
    }

    fn progress(&self, now: f64) -> f32 {
        if self.anim_start < 0.0 || (now - self.anim_start) >= ANIM_DUR {
            return self.anim_to;
        }
        let t = ((now - self.anim_start) / ANIM_DUR).clamp(0.0, 1.0) as f32;
        // 窗格滑动缓动（SplitView KeySpline 0.0,0.35 0.15,1.0）。
        lerp(self.anim_from, self.anim_to, pane_ease(t))
    }

    fn pane_w(&self, now: f64) -> f32 {
        lerp(COMPACT_W, OPEN_W, self.progress(now))
    }

    fn toggle_rect(&self, pane_w: f32) -> Rect {
        Rect { x: self.rect.x, y: self.rect.y, w: pane_w, h: TOGGLE_H }
    }

    fn item_rect(&self, i: usize, pane_w: f32) -> Rect {
        Rect { x: self.rect.x, y: self.rect.y + TOGGLE_H + 4.0 + i as f32 * ITEM_H, w: pane_w, h: ITEM_H }
    }
}

impl Widget for NavigationView {
    fn measure(&mut self, available: Size) -> Size {
        Size { w: available.w.clamp(420.0, 600.0), h: 280.0 }
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
        let pane_w = self.pane_w(now);
        let open_amt = self.progress(now); // 0..1 文字淡入

        // 整体卡片 + 边框（应用外壳模式下铺满、无卡片边框）。卡片底 = SolidBackgroundFillColorBase。
        if !self.app_shell {
            ctx.painter.fill_rounded_rect(self.rect, 6.0, t.solid_bg_base);
            ctx.painter.stroke_inner(self.rect, 6.0, t.divider_stroke_default, 1.0);
        }

        // 右侧内容区（.navigation-view-page，fluent-svelte NavigationView.svelte:69-80）。
        let content = Rect { x: self.rect.x + pane_w, y: self.rect.y, w: (self.rect.w - pane_w).max(0.0), h: self.rect.h };
        if !self.app_shell {
            // background-color: var(--layer-background-default)（dark hsla(0,0%,23%,30%)=#4C3A3A3A /
            // light hsla(0,0%,100%,50%)=#80FFFFFF），background-clip:padding-box。
            let layer_fill = if t.is_dark { Color::hex("#4C3A3A3A") } else { Color::hex("#80FFFFFF") };
            // border-start-start-radius: var(--overlay-corner-radius)=8px（仅左上角圆角）。
            // 先铺整块，再把左上角「抠圆」：用卡片底色 solid_bg_base 抹掉 8×8 角，再补一段 8px 圆角的填充。
            ctx.painter.fill_rect(content, layer_fill);
            let cr = PAGE_CORNER;
            ctx.painter.push_clip(Rect { x: content.x, y: content.y, w: cr, h: cr });
            ctx.painter.fill_rect(Rect { x: content.x, y: content.y, w: cr, h: cr }, t.solid_bg_base);
            // 圆角填充：左上角对齐 content 左上，其余三角推到裁剪框外只露出左上弧。
            ctx.painter.fill_rounded_rect(Rect { x: content.x, y: content.y, w: cr * 2.0, h: cr * 2.0 }, cr, layer_fill);
            ctx.painter.pop_clip();
            // border: 1px solid var(--card-stroke-default)，仅上+左边（border-block-end/inline-end:none）。
            // 左边：从圆角下沿起；上边：从圆角右沿起（左上角处由弧描边代替直角）。
            ctx.painter.fill_rect(Rect { x: content.x, y: content.y + cr, w: 1.0, h: (content.h - cr).max(0.0) }, t.card_stroke_default);
            ctx.painter.fill_rect(Rect { x: content.x + cr, y: content.y, w: (content.w - cr).max(0.0), h: 1.0 }, t.card_stroke_default);
        }

        // 窗格背景：
        // * 应用外壳：与右侧页面**完全同色**（SolidBackgroundFillColorBase，无分隔线）—— 用户要求左 nav 与右页面一致。
        // * 画廊：收起 acrylic 替身 ↔ 展开透明 间插值，圆角卡片。
        let pane = Rect { x: self.rect.x, y: self.rect.y, w: pane_w, h: self.rect.h };
        if self.app_shell {
            ctx.painter.fill_rect(pane, t.solid_bg_base);
        } else {
            let pane_bg = crate::anim::lerp_color(t.solid_bg_secondary, t.solid_bg_base, open_amt);
            ctx.painter.fill_rounded_rect(pane, 6.0, pane_bg);
        }

        // 汉堡按钮（PaneToggleButton：40×36、圆角 4=ControlCornerRadius，hover/pressed=SubtleFill）。
        let tr = self.toggle_rect(pane_w);
        let toggle_bg = if self.pressed == Some(-1) {
            Some(t.subtle_fill_tertiary) // NavigationViewButtonBackgroundPressed
        } else if self.hovered == Some(-1) {
            Some(t.subtle_fill_secondary) // NavigationViewButtonBackgroundPointerOver
        } else {
            None
        };
        if let Some(bg) = toggle_bg {
            ctx.painter.fill_rounded_rect(Rect { x: tr.x + 4.0, y: tr.y + 4.0, w: 40.0, h: 36.0 }, 4.0, bg);
        }
        let ham = Rect { x: tr.x + ICON_CX - 8.0, y: tr.center_y() - 8.0, w: 16.0, h: 16.0 };
        ctx.painter.draw_glyph(Icon::Hamburger, ham, t.text_primary);

        // 导航项（背景 / 图标 / 标签）——选中指示条单独做动画绘制。
        // 背景态在 WinUI 是即时 Setter；这里加 83ms 平滑过渡，匹配网页 hover 渐变手感。
        //   rest=透明, pointerOver=SubtleSecondary, pressed=SubtleTertiary,
        //   selected=SubtleSecondary, selected+pointerOver=SubtleTertiary, selected+pressed=SubtleSecondary。
        if self.item_bg.len() != self.items.len() {
            self.item_bg = vec![ColorTransition::instant(Color::TRANSPARENT); self.items.len()];
            self.bg_init = false;
        }
        for i in 0..self.items.len() {
            let r = self.item_rect(i, pane_w);
            let selected = i == self.selected;
            let hovered = self.hovered == Some(i as i32);
            let pressed = self.pressed == Some(i as i32);
            // rest 目标 = 同 RGB、alpha 0，保证淡入淡出是纯透明度变化（不会串成灰）。
            let target_bg = match (selected, pressed, hovered) {
                (false, false, false) => t.subtle_fill_secondary.with_opacity(0.0),
                (false, true, _) => t.subtle_fill_tertiary,
                (false, false, true) => t.subtle_fill_secondary,
                (true, true, _) => t.subtle_fill_secondary,
                (true, false, true) => t.subtle_fill_tertiary,
                (true, false, false) => t.subtle_fill_secondary,
            };
            if self.bg_init {
                self.item_bg[i].retarget(target_bg, now, ITEM_BG_DUR);
            } else {
                self.item_bg[i] = ColorTransition::instant(target_bg);
            }
            let c = self.item_bg[i].value(now);
            if c.a != 0 {
                // LayoutRoot：边距 4,2、圆角 4。
                let lr = Rect {
                    x: r.x + ITEM_MARGIN_L,
                    y: r.y + ITEM_MARGIN_V,
                    // 选中/悬停高亮：**展开时**缩短 1/7（原来太长）；折叠时不缩，保持图标方块对称
                    // （否则会把折叠态的小方块右边「啃掉」一块）。按展开进度线性插值。
                    w: ((r.w - 2.0 * ITEM_MARGIN_L)
                        * (1.0 - (1.0 / 7.0) * ((pane_w - COMPACT_W) / (OPEN_W - COMPACT_W)).clamp(0.0, 1.0)))
                    .max(0.0),
                    h: (r.h - 2.0 * ITEM_MARGIN_V).max(0.0),
                };
                ctx.painter.fill_rounded_rect(lr, ITEM_CORNER, c);
            }
            // 前景：rest/over/selected=TextPrimary，pressed=TextSecondary。
            let fg = if pressed { t.text_secondary } else { t.text_primary };
            let icon = Rect { x: r.x + ICON_CX - 8.0, y: r.center_y() - 8.0, w: 16.0, h: 16.0 };
            ctx.painter.draw_glyph(self.items[i].icon, icon, fg);
            if open_amt > 0.05 {
                let label_rect = Rect { x: r.x + LABEL_X, y: r.y, w: (r.w - LABEL_X - 14.0).max(0.0), h: r.h };
                let _ = ctx.painter.draw_text_leading(&self.items[i].label, TextStyle::BODY, label_rect, fg.with_opacity(open_amt));
            }
        }
        self.bg_init = true;

        // 选中指示条：拉伸滑动（源码 PlayIndicatorAnimations：前缘快、后缘缓，中途拉伸）。
        // 居左对齐于 LayoutRoot 左缘（item 左 + 按钮边距 4），3×16、圆角 2、AccentFillColorDefault。
        let cur_c = self.item_rect(self.selected, pane_w).center_y();
        let prev_c = self.item_rect(self.prev_selected, pane_w).center_y();
        let (top, bot) = indicator_edges(self.sel_start, prev_c, cur_c, now);
        let ind = Rect { x: self.rect.x + ITEM_MARGIN_L, y: top, w: IND_W, h: (bot - top).max(1.0) };
        ctx.painter.fill_rounded_rect(ind, IND_RADIUS, t.accent_fill_default());

        // 右侧内容区演示（仅 gallery 演示用；应用外壳由宿主在 content_area 自行绘制）。
        if self.show_demo_content {
            ctx.painter.push_clip(content);
            let pp = if self.sel_start < 0.0 {
                1.0
            } else {
                // EntranceThemeTransition 缓动（KeySpline 0.1,0.9 0.2,1.0）。
                page_ease(((now - self.sel_start) / PAGE_DUR).clamp(0.0, 1.0) as f32)
            };
            let dy = (1.0 - pp) * PAGE_OFFSET;
            // 内容自 .navigation-view-page 的 padding 起：padding-inline 56 / padding-block 44。
            let px = content.x + PAGE_PAD_INLINE;
            let inner_w = (content.w - 2.0 * PAGE_PAD_INLINE).max(0.0);
            let _ = ctx.painter.draw_text_leading(
                &self.items[self.selected].label,
                TextStyle::SUBTITLE,
                Rect { x: px, y: content.y + PAGE_PAD_BLOCK + dy, w: inner_w, h: 32.0 },
                t.text_primary.with_opacity(pp),
            );
            let _ = ctx.painter.draw_text_leading(
                "这是导航内容区。点左侧汉堡 ☰ 可展开/收缩；切换项有指示条滑动 + 页面进入动画。",
                TextStyle::BODY,
                Rect { x: px, y: content.y + PAGE_PAD_BLOCK + 40.0 + dy, w: inner_w, h: 24.0 },
                t.text_secondary.with_opacity(pp),
            );
            ctx.painter.pop_clip();
        }
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        let pane_w = self.pane_w(now);
        let mut redraw = false;
        let mut animating = false;
        match ev {
            InputEvent::PointerMove(p) => {
                let mut h: Option<i32> = None;
                if self.toggle_rect(pane_w).contains(p) {
                    h = Some(-1);
                } else {
                    for i in 0..self.items.len() {
                        if self.item_rect(i, pane_w).contains(p) {
                            h = Some(i as i32);
                            break;
                        }
                    }
                }
                if h != self.hovered {
                    self.hovered = h;
                    redraw = true;
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
                // 记录按下目标（toggle=-1 / item>=0），用于 Pressed 视觉态（即时切换，无动画）。
                let mut pr: Option<i32> = None;
                if self.toggle_rect(pane_w).contains(p) {
                    pr = Some(-1);
                } else {
                    for i in 0..self.items.len() {
                        if self.item_rect(i, pane_w).contains(p) {
                            pr = Some(i as i32);
                            break;
                        }
                    }
                }
                if pr != self.pressed {
                    self.pressed = pr;
                    redraw = true;
                }
            }
            InputEvent::PointerUp(p) => {
                if self.pressed.is_some() {
                    self.pressed = None;
                    redraw = true;
                }
                if self.toggle_rect(pane_w).contains(p) {
                    // 切换展开/收缩
                    self.anim_from = self.progress(now);
                    self.expanded = !self.expanded;
                    self.anim_to = if self.expanded { 1.0 } else { 0.0 };
                    self.anim_start = now;
                    redraw = true;
                    animating = true;
                } else {
                    for i in 0..self.items.len() {
                        if self.item_rect(i, pane_w).contains(p) {
                            if i != self.selected {
                                self.prev_selected = self.selected;
                                self.selected = i;
                                self.sel_start = now; // 触发指示条滑动 + 页面进入动画
                                animating = true;
                            }
                            redraw = true;
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
        EventResult { redraw, animating }
    }

    fn is_animating(&self, now: f64) -> bool {
        (self.anim_start >= 0.0 && (now - self.anim_start) < ANIM_DUR)
            || (self.sel_start >= 0.0 && (now - self.sel_start) < SEL_DUR)
            || self.item_bg.iter().any(|b| b.is_active(now))
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::List
    }
}

/// 选中指示条上下边随时间的位置（拉伸滑动，源码 PlayIndicatorAnimations）。
fn indicator_edges(sel_start: f64, prev_c: f32, cur_c: f32, now: f64) -> (f32, f32) {
    let old_top = prev_c - IND_H / 2.0;
    let old_bot = prev_c + IND_H / 2.0;
    let new_top = cur_c - IND_H / 2.0;
    let new_bot = cur_c + IND_H / 2.0;
    if sel_start < 0.0 {
        return (new_top, new_bot);
    }
    let t = ((now - sel_start) / SEL_DUR).clamp(0.0, 1.0) as f32;
    if t >= 1.0 {
        return (new_top, new_bot);
    }
    // 两段式（对应源码 Offset 在 0.333 步进 + Scale 先涨后落）：
    //   阶段1 (0~0.333)：领先边伸向目标(frame1 缓动)，拖尾边按住不动 → 拉长；
    //   阶段2 (0.333~1)：领先边到位按住，拖尾边收向目标(frame2 缓动) → 收回。
    let p1 = (t / 0.333).min(1.0);
    let p2 = ((t - 0.333) / 0.667).clamp(0.0, 1.0);
    let stretch = cubic_bezier(0.9, 0.1, 1.0, 0.2, p1); // frame1
    let settle = cubic_bezier(0.1, 0.9, 0.2, 1.0, p2); // frame2
    if cur_c >= prev_c {
        // 下移：底边领先、顶边收尾
        let bottom = lerp(old_bot, new_bot, stretch);
        let top = lerp(old_top, new_top, settle);
        (top, bottom)
    } else {
        // 上移：顶边领先、底边收尾
        let top = lerp(old_top, new_top, stretch);
        let bottom = lerp(old_bot, new_bot, settle);
        (top, bottom)
    }
}
