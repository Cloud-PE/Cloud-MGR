//! 设置页：基本设置（颜色模式）/ 启动盘设置 / 下载设置（线程数 + 默认下载路径）/ 关于。
//! 四个可折叠分组。所有更改即时写入配置并保存（无显式保存按钮）。

use super::widgets::{draw_card, draw_scrollbar, page_header, Clicky, Scroll};
use crate::config::{AppConfig, ColorMode};
use crate::mode::PluginMode;
use crate::utils::BootDriveManager;
use fluentpx::anim::cubic_bezier;
use fluentpx::controls::{Button, ComboBox};
use fluentpx::gfx::Icon;
use fluentpx::typography::TextStyle;
use fluentpx::widget::*;
use parking_lot::RwLock;
use std::sync::Arc;

const THREADS: [u32; 3] = [8, 16, 32];
// 卡片折叠/展开动画（同 WinUI Expander：展开 333ms 减速、折叠 167ms）。
const EXPAND_DUR: f64 = 0.333;
const COLLAPSE_DUR: f64 = 0.167;

pub struct SettingsPage {
    config: Arc<RwLock<AppConfig>>,
    boot_drive_manager: Arc<RwLock<BootDriveManager>>,
    mode: PluginMode,

    color_combo: ComboBox,
    drive_combo: ComboBox,
    threads_combo: ComboBox,
    rescan_btn: Clicky,
    browse_btn: Clicky,

    exp: [bool; 4], // basic / boot / download / about（目标展开态）
    anim_from: [f32; 4], // 当前动画起点处的展开度
    anim_start: [f64; 4], // 动画开始时刻（-1=无动画）
    cached_letters: Vec<String>, // 非动画帧刷新；动画期间复用，避免每帧读盘锁 + 重建组合项
    hdr_rects: [Rect; 4],
    scroll: Scroll,
    content_h: f32,
    view: Rect,
}

impl SettingsPage {
    pub fn new(config: Arc<RwLock<AppConfig>>, boot_drive_manager: Arc<RwLock<BootDriveManager>>, mode: PluginMode) -> Self {
        let color_idx = match config.read().color_mode {
            ColorMode::System => 0,
            ColorMode::Light => 1,
            ColorMode::Dark => 2,
        };
        let threads_idx = THREADS.iter().position(|&t| t == config.read().download_threads).unwrap_or(0);

        Self {
            config,
            boot_drive_manager,
            mode,
            color_combo: ComboBox::new(vec!["跟随系统（默认）".into(), "浅色模式".into(), "深色模式".into()], color_idx),
            drive_combo: ComboBox::new(vec![], 0),
            threads_combo: ComboBox::new(vec!["8 线程".into(), "16 线程".into(), "32 线程（最大）".into()], threads_idx),
            rescan_btn: Clicky::new(Button::standard("重新扫描启动盘")),
            browse_btn: Clicky::new(Button::standard("浏览")),
            exp: [true; 4],
            anim_from: [1.0; 4],
            anim_start: [-1.0; 4],
            cached_letters: Vec::new(),
            hdr_rects: [Rect::default(); 4],
            scroll: Scroll::new(),
            content_h: 0.0,
            view: Rect::default(),
        }
    }

    /// 第 i 节当前展开度 0..1（折叠↔展开，动画中为中间值）。
    fn exp_amt(&self, i: usize, now: f64) -> f32 {
        let target = if self.exp[i] { 1.0 } else { 0.0 };
        let start = self.anim_start[i];
        if start < 0.0 {
            return target;
        }
        let dur = if self.exp[i] { EXPAND_DUR } else { COLLAPSE_DUR };
        let t = ((now - start) / dur).clamp(0.0, 1.0) as f32;
        if t >= 1.0 {
            return target;
        }
        // 展开：cubic(0,0,0,1) 减速揭示；折叠：线性收拢（结尾不拖）。
        let e = if self.exp[i] { cubic_bezier(0.0, 0.0, 0.0, 1.0, t) } else { t };
        self.anim_from[i] + (target - self.anim_from[i]) * e
    }

    fn drive_letters(&self) -> Vec<String> {
        self.boot_drive_manager.read().get_all_drives().iter().map(|d| d.letter.clone()).collect()
    }

    fn open_combo(&self) -> Option<u8> {
        if self.color_combo.open {
            Some(0)
        } else if self.drive_combo.open {
            Some(1)
        } else if self.threads_combo.open {
            Some(2)
        } else {
            None
        }
    }

    fn browse_download_path(&mut self) {
        if let Some(path) = rfd::FileDialog::new().set_title("选择默认下载路径").pick_folder() {
            let mut c = self.config.write();
            c.default_download_path = Some(path);
            let _ = c.save();
        }
    }
}

fn label(ctx: &mut PaintCtx, x: f32, y: f32, w: f32, text: &str) {
    let t = ctx.tokens;
    let _ = ctx.painter.draw_text_leading(text, TextStyle::BODY, Rect::new(x, y, w, 24.0), t.text_secondary);
}

impl SettingsPage {
    pub fn paint(&mut self, ctx: &mut PaintCtx, area: Rect) {
        let top = page_header(ctx, area, "设置");
        let view = Rect::new(area.x, top, area.w, area.bottom() - top);
        self.view = view;
        ctx.painter.push_clip(view);

        // 配置→组合框同步 + 启动盘列表读取**仅在非动画帧**做（动画期间配置不会变）。
        // 否则每帧 4 次 RwLock 读 + Vec 重建会拖长帧、超出 vblank 预算 → 掉帧卡顿。
        let now = ctx.now;
        self.scroll.tick(now); // 平滑滚动每帧推进
        let letters: Vec<String> = if self.is_animating(now) {
            self.cached_letters.clone()
        } else {
            let cm = self.config.read().color_mode.clone();
            self.color_combo.selected = match cm { ColorMode::System => 0, ColorMode::Light => 1, ColorMode::Dark => 2 };
            let th = self.config.read().download_threads;
            self.threads_combo.selected = THREADS.iter().position(|&t| t == th).unwrap_or(0);
            let l = self.drive_letters();
            let cur = self.boot_drive_manager.read().get_current_drive().unwrap_or_default();
            self.drive_combo.items = l.clone();
            self.drive_combo.selected = l.iter().position(|x| *x == cur).unwrap_or(0);
            self.cached_letters = l.clone();
            l
        };

        let x = area.x + 4.0;
        // 卡片宽度铺满页面（左 4 + 右留 14 给滚动条），不再固定上限。
        let w = (area.w - 18.0).max(280.0);
        let mut y = top - self.scroll.offset;
        let hdr_h = 48.0;
        let pad = 16.0;

        let about_lines: f32 = if matches!(self.mode, PluginMode::CloudPE | PluginMode::HotPE | PluginMode::Edgeless) { 5.0 } else { 3.0 };
        let bodies = [
            40.0,
            if letters.is_empty() { 70.0 } else { 84.0 },
            84.0,
            26.0 + about_lines * 24.0,
        ];
        let titles = ["基本设置", "启动盘设置", "下载设置", "关于"];

        for i in 0..4 {
            let amt = self.exp_amt(i, now); // 0=折叠 1=展开（动画中为中间值）
            let body_h = (bodies[i] + pad) * amt;
            let card_h = hdr_h + body_h;
            let card = Rect::new(x, y, w, card_h);
            // 每节是一张可折叠卡片：圆角卡 + 头部(标题+chevron) + 内容（按 amt 揭示）。
            draw_card(ctx, card);
            self.hdr_rects[i] = Rect::new(x, y, w, hdr_h);
            let t = ctx.tokens;
            let _ = ctx.painter.draw_text_leading(titles[i], TextStyle::BODY_STRONG, Rect::new(x + pad, y, w - 48.0, hdr_h), t.text_primary);
            // chevron 随展开度旋转 0°→180°（下→上）。
            let chev_c = Point { x: card.right() - 26.0, y: y + hdr_h / 2.0 };
            ctx.painter.set_rotation_about(chev_c, amt * 180.0);
            let _ = ctx.painter.draw_icon(Icon::ChevronDown.codepoint(), 14.0, Rect::new(card.right() - 36.0, y, 20.0, hdr_h), t.text_secondary);
            ctx.painter.set_transform(None);
            // 内容：展开度>0 即绘制；动画中（<1）裁剪到当前高度，实现从头部向下揭示。
            if amt > 0.001 {
                let clipped = amt < 0.999;
                if clipped {
                    // 动画揭示用抗锯齿裁剪：裁剪线逐帧移动时，不让下拉框等高对比边框在像素间跳动（频闪）。
                    ctx.painter.push_clip_aa(Rect::new(x, y + hdr_h, w, body_h));
                }
                self.paint_section(ctx, i, x + pad, y + hdr_h, w - pad * 2.0, &letters);
                if clipped {
                    ctx.painter.pop_clip();
                }
            }
            y += card_h + 10.0;
        }

        self.content_h = (y + self.scroll.offset) - top;
        self.scroll.clamp((self.content_h - view.h).max(0.0));
        ctx.painter.pop_clip();
        draw_scrollbar(ctx, &self.scroll, view, self.content_h);
    }

    /// 绘制第 i 节展开后的内容（卡片内）。
    fn paint_section(&mut self, ctx: &mut PaintCtx, i: usize, cx: f32, cy: f32, cw: f32, letters: &[String]) {
        match i {
            0 => {
                label(ctx, cx, cy + 6.0, 80.0, "颜色模式：");
                self.color_combo.arrange(Rect::new(cx + 92.0, cy, 200.0, 32.0));
                self.color_combo.paint(ctx);
            }
            1 => {
                if letters.is_empty() {
                    label(ctx, cx, cy + 2.0, 200.0, "未检测到启动盘");
                    self.rescan_btn.btn.text = "刷新启动盘".into();
                    self.rescan_btn.arrange(Rect::new(cx, cy + 30.0, 140.0, 32.0));
                    self.rescan_btn.paint(ctx);
                } else {
                    label(ctx, cx, cy + 6.0, 90.0, "当前启动盘：");
                    self.drive_combo.arrange(Rect::new(cx + 100.0, cy, 160.0, 32.0));
                    self.drive_combo.paint(ctx);
                    self.rescan_btn.btn.text = "重新扫描启动盘".into();
                    self.rescan_btn.arrange(Rect::new(cx, cy + 44.0, 160.0, 32.0));
                    self.rescan_btn.paint(ctx);
                }
            }
            2 => {
                label(ctx, cx, cy + 6.0, 90.0, "下载线程数：");
                self.threads_combo.arrange(Rect::new(cx + 100.0, cy, 160.0, 32.0));
                self.threads_combo.paint(ctx);
                label(ctx, cx, cy + 50.0, 110.0, "默认下载路径：");
                let path_str = self.config.read().default_download_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "未设置".into());
                let tt = ctx.tokens;
                let _ = ctx.painter.draw_text_leading(&path_str, TextStyle::BODY, Rect::new(cx + 112.0, cy + 50.0, cw - 200.0, 24.0), tt.text_primary);
                self.browse_btn.arrange(Rect::new(cx + cw - 76.0, cy + 46.0, 72.0, 32.0));
                self.browse_btn.paint(ctx);
            }
            _ => {
                let tt = ctx.tokens;
                let about_title = match self.mode {
                    PluginMode::CloudPE => "Cloud-PE 插件市场",
                    PluginMode::HotPE => "HotPE 模块下载",
                    PluginMode::Edgeless => "Edgeless 插件下载",
                    _ => "",
                };
                let mut yy = cy;
                let _ = ctx.painter.draw_text_leading(about_title, TextStyle::BODY_STRONG, Rect::new(cx, yy, cw, 22.0), tt.text_primary);
                yy += 26.0;
                for line in ["版本：v0.1", "作者：NORMAL-EX（别称：dddffgg）", "版权：© 2025-present Cloud-PE Dev."] {
                    let _ = ctx.painter.draw_text_leading(line, TextStyle::BODY, Rect::new(cx, yy, cw, 22.0), tt.text_secondary);
                    yy += 24.0;
                }
                let desc: &[&str] = match self.mode {
                    PluginMode::CloudPE => &["此软件是 Cloud-PE One 的独立功能模块", "专用于管理和下载 Cloud-PE 插件"],
                    PluginMode::HotPE => &["此软件是 HotPE 模块下载管理工具", "专用于管理和下载 HotPE 模块"],
                    PluginMode::Edgeless => &["此软件是 Edgeless 插件下载管理工具", "专用于管理和下载 Edgeless 插件"],
                    _ => &[],
                };
                let tt = ctx.tokens;
                for line in desc {
                    let _ = ctx.painter.draw_text_leading(line, TextStyle::BODY, Rect::new(cx, yy, cw, 22.0), tt.text_secondary);
                    yy += 24.0;
                }
            }
        }
    }

    pub fn paint_overlay(&mut self, ctx: &mut PaintCtx, _area: Rect) {
        self.color_combo.paint_overlay(ctx);
        self.drive_combo.paint_overlay(ctx);
        self.threads_combo.paint_overlay(ctx);
    }

    pub fn on_event(&mut self, ev: InputEvent, now: f64, _area: Rect) -> EventResult {
        // 滚动条拖拽优先。
        if self.scroll.scrollbar_event(ev, self.view, self.content_h) {
            return EventResult::REDRAW;
        }
        // 组合框打开时独占事件。
        if let Some(open) = self.open_combo() {
            let before = match open { 0 => self.color_combo.selected, 1 => self.drive_combo.selected, _ => self.threads_combo.selected };
            let r = match open { 0 => self.color_combo.on_event(ev, now), 1 => self.drive_combo.on_event(ev, now), _ => self.threads_combo.on_event(ev, now) };
            let after = match open { 0 => self.color_combo.selected, 1 => self.drive_combo.selected, _ => self.threads_combo.selected };
            if before != after {
                self.apply_combo(open);
            }
            return r.or(EventResult::REDRAW);
        }

        // 滚轮滚动
        if let InputEvent::Wheel(d) = ev {
            let max = (self.content_h - self.view.h).max(0.0);
            if self.scroll.apply_wheel(d, max) {
                return EventResult::REDRAW;
            }
        }

        // 分组头点击切换展开
        if let InputEvent::PointerUp(p) = ev {
            for i in 0..4 {
                if self.hdr_rects[i].contains(p) {
                    self.anim_from[i] = self.exp_amt(i, now); // 从当前展开度起算（连点也平滑）
                    self.exp[i] = !self.exp[i];
                    self.anim_start[i] = now;
                    return EventResult { redraw: true, animating: true };
                }
            }
        }

        let mut res = self.color_combo.on_event(ev, now);
        res = res.or(self.drive_combo.on_event(ev, now));
        res = res.or(self.threads_combo.on_event(ev, now));
        // 闭合状态下点击组合框可能将其打开 —— 之后由 open_combo 分支接管。
        let (r1, rescan) = self.rescan_btn.event(ev, now);
        let (r2, browse) = self.browse_btn.event(ev, now);
        if rescan {
            self.boot_drive_manager.write().reload();
            res = EventResult::REDRAW;
        }
        if browse {
            self.browse_download_path();
            res = EventResult::REDRAW;
        }
        res.or(r1).or(r2)
    }

    fn apply_combo(&mut self, which: u8) {
        match which {
            0 => {
                let m = match self.color_combo.selected { 0 => ColorMode::System, 1 => ColorMode::Light, _ => ColorMode::Dark };
                let mut c = self.config.write();
                c.color_mode = m;
                let _ = c.save();
            }
            1 => {
                let letters = self.drive_letters();
                if let Some(letter) = letters.get(self.drive_combo.selected).cloned() {
                    self.boot_drive_manager.write().set_current_drive(letter.clone());
                    let mut c = self.config.write();
                    c.default_boot_drive = Some(letter);
                    let _ = c.save();
                }
            }
            _ => {
                let t = THREADS[self.threads_combo.selected.min(2)];
                let mut c = self.config.write();
                c.download_threads = t;
                let _ = c.save();
            }
        }
    }

    pub fn is_animating(&self, now: f64) -> bool {
        let sections = (0..4).any(|i| {
            let s = self.anim_start[i];
            let dur = if self.exp[i] { EXPAND_DUR } else { COLLAPSE_DUR };
            s >= 0.0 && (now - s) < dur
        });
        sections
            || self.scroll.is_settling()
            || self.rescan_btn.is_animating(now)
            || self.browse_btn.is_animating(now)
            || self.color_combo.is_animating(now)
            || self.drive_combo.is_animating(now)
            || self.threads_combo.is_animating(now)
    }
}
