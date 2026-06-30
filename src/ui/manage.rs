//! 插件管理页：已启用 / 已禁用 两个可折叠分组；每项可启用/禁用，有新版时可更新。
//! 需先选择启动盘。更新走 tokio 异步（删旧文件→下载新文件→刷新本地列表）。

use super::widgets::{btn_width, draw_card, draw_scrollbar, empty_state, generate_plugin_filename, imm_button, loading_button, page_header, section_head, Scroll};
use crate::config::AppConfig;
use crate::downloader::Downloader;
use crate::mode::PluginMode;
use crate::plugins::{Plugin, PluginManager};
use crate::utils::BootDriveManager;
use fluentpx::anim::cubic_bezier;
use fluentpx::controls::ProgressRing;
use fluentpx::typography::TextStyle;
use fluentpx::widget::*;
use parking_lot::RwLock;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

// 分组折叠/展开动画（同 WinUI Expander：展开 333ms 减速、折叠 167ms）。
const EXPAND_DUR: f64 = 0.333;
const COLLAPSE_DUR: f64 = 0.167;

enum Act {
    ToggleSec(usize),
    Enable(String),
    Disable(String),
    Update(Plugin),
}

pub struct ManagePage {
    plugin_manager: Arc<RwLock<PluginManager>>,
    boot_drive_manager: Arc<RwLock<BootDriveManager>>,
    mode: PluginMode,
    runtime: Arc<Runtime>,
    config: Arc<RwLock<AppConfig>>,
    updating: Arc<RwLock<HashSet<String>>>,
    last_refresh: Option<Instant>,
    need_refresh: bool,

    exp: [bool; 2],
    anim_from: [f32; 2],  // 折叠动画起点处的展开度
    anim_start: [f64; 2], // 动画开始时刻（-1=无动画）
    cached_enabled: Vec<Plugin>,  // 动画期间复用，避免每帧锁 + 深拷贝列表
    cached_disabled: Vec<Plugin>,
    scroll: Scroll,
    content_h: f32,
    view: Rect,
    ring: ProgressRing,
    mouse: Point,
    pressed: Option<usize>,
    hotspots: Vec<(Rect, Act)>,
}

impl ManagePage {
    pub fn new(
        plugin_manager: Arc<RwLock<PluginManager>>,
        boot_drive_manager: Arc<RwLock<BootDriveManager>>,
        mode: PluginMode,
        runtime: Arc<Runtime>,
        config: Arc<RwLock<AppConfig>>,
    ) -> Self {
        Self {
            plugin_manager,
            boot_drive_manager,
            mode,
            runtime,
            config,
            updating: Arc::new(RwLock::new(HashSet::new())),
            last_refresh: None,
            need_refresh: true,
            exp: [true, true],
            anim_from: [1.0, 1.0],
            anim_start: [-1.0, -1.0],
            cached_enabled: Vec::new(),
            cached_disabled: Vec::new(),
            scroll: Scroll::new(),
            content_h: 0.0,
            view: Rect::default(),
            ring: ProgressRing::new(),
            mouse: Point::default(),
            pressed: None,
            hotspots: Vec::new(),
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

    /// 一节内容的完整高度（用于按展开度裁剪揭示）。
    fn section_full_h(&self, list: &[Plugin]) -> f32 {
        if list.is_empty() {
            return 30.0; // 空态一行
        }
        list.iter()
            .map(|p| {
                let has_desc = self.mode != PluginMode::Edgeless && !p.describe.is_empty();
                (if has_desc { 86.0 } else { 66.0 }) + 8.0
            })
            .sum()
    }

    /// 标记需要刷新（切到本页时调用，替代空闲 2s 轮询）。
    pub fn mark_dirty(&mut self) {
        self.need_refresh = true;
    }

    fn is_module(&self) -> bool {
        self.mode == PluginMode::HotPE
    }

    fn check_update(&self, local: &Plugin) -> bool {
        let m = self.plugin_manager.read();
        if let Some(mkt) = m.find_market_plugin_by_id(&local.get_plugin_id()) {
            matches!(m.compare_versions(&local.version, &mkt.version), std::cmp::Ordering::Less)
        } else {
            false
        }
    }

    fn update_plugin(&mut self, local: Plugin, drive: String) {
        let plugin_id = local.get_plugin_id();
        let task_id = format!("{}_update", plugin_id);
        self.updating.write().insert(task_id.clone());

        let market = match self.plugin_manager.read().find_market_plugin_by_id(&plugin_id) {
            Some(m) => m,
            None => {
                self.updating.write().remove(&task_id);
                return;
            }
        };

        let threads = self.config.read().download_threads;
        let url = market.link.clone();
        let filename = generate_plugin_filename(self.mode, &market);
        let old_file = local.file.clone();
        let mode = self.mode;
        let pm = self.plugin_manager.clone();
        let updating = self.updating.clone();

        self.runtime.spawn(async move {
            let plugin_dir = format!("{}\\{}", drive, mode.get_plugin_folder());
            if tokio::fs::create_dir_all(&plugin_dir).await.is_err() {
                updating.write().remove(&task_id);
                return;
            }
            if pm.read().delete_plugin_file(&drive, &old_file).is_err() {
                updating.write().remove(&task_id);
                return;
            }
            let ext = mode.get_enabled_extension();
            let path = PathBuf::from(&plugin_dir).join(format!("{}.{}", filename, ext));
            let downloader = Downloader::new(threads);
            if downloader.download(&url, path).await.is_ok() {
                let _ = pm.write().load_local_plugins(&drive);
            }
            updating.write().remove(&task_id);
        });
    }

    /// 画一张插件卡 + 登记操作热区。返回卡片底部 y。
    fn card(&mut self, ctx: &mut PaintCtx, x: f32, y: f32, w: f32, plugin: &Plugin, enabled: bool, drive: &str) -> f32 {
        let has_desc = self.mode != PluginMode::Edgeless && !plugin.describe.is_empty();
        let card_h = if has_desc { 86.0 } else { 66.0 };
        let r = Rect::new(x, y, w, card_h);
        draw_card(ctx, r);
        let t = ctx.tokens;
        let ix = r.x + 12.0;
        // 文字列裁剪到按钮左侧，避免长描述/名称压到右侧操作按钮上。
        let text_right = r.right() - 156.0;
        let tw = (text_right - ix).max(0.0);
        ctx.painter.push_clip(Rect::new(r.x, r.y, (text_right - r.x).max(0.0), card_h));
        let mut ty = r.y + 10.0;
        let _ = ctx.painter.draw_text_leading(&plugin.name, TextStyle::BODY_STRONG, Rect::new(ix, ty, tw, 22.0), t.text_primary);
        ty += 24.0;
        if has_desc {
            let _ = ctx.painter.draw_text_leading(&plugin.describe, TextStyle::BODY, Rect::new(ix, ty, tw, 20.0), t.text_secondary);
            ty += 22.0;
        }
        let meta = format!("版本: {}    大小: {}    作者: {}", plugin.version, plugin.size, plugin.author);
        let _ = ctx.painter.draw_text_leading(&meta, TextStyle::CAPTION, Rect::new(ix, ty, tw, 18.0), t.text_tertiary);
        ctx.painter.pop_clip();

        // 右侧操作（从右往左放）。
        let plugin_id = plugin.get_plugin_id();
        let is_updating = self.updating.read().contains(&format!("{}_update", plugin_id));
        let bh = 30.0;
        let by = r.center_y() - bh / 2.0;
        let mut bx = r.right() - 12.0;
        let gap = 8.0;

        if enabled {
            // 更新（若有新版）
            if self.check_update(plugin) {
                let bw = btn_width(ctx, "更新");
                bx -= bw;
                let br = Rect::new(bx, by, bw, bh);
                if is_updating {
                    loading_button(ctx, &mut self.ring, br);
                } else {
                    let i = self.hotspots.len();
                    imm_button(ctx, br, "更新", true, self.is_hover(br), self.is_press(i), true);
                    self.hotspots.push((br, Act::Update(plugin.clone())));
                }
                bx -= gap;
            }
            // 禁用（更新进行中时隐藏，匹配原行为）
            if !is_updating {
                let bw = btn_width(ctx, "禁用");
                bx -= bw;
                let br = Rect::new(bx, by, bw, bh);
                let i = self.hotspots.len();
                imm_button(ctx, br, "禁用", false, self.is_hover(br), self.is_press(i), true);
                self.hotspots.push((br, Act::Disable(plugin.file.clone())));
            }
        } else {
            let bw = btn_width(ctx, "启用");
            bx -= bw;
            let br = Rect::new(bx, by, bw, bh);
            let i = self.hotspots.len();
            imm_button(ctx, br, "启用", false, self.is_hover(br), self.is_press(i), true);
            self.hotspots.push((br, Act::Enable(plugin.file.clone())));
        }
        let _ = drive;
        r.bottom() + 8.0
    }

    fn is_hover(&self, r: Rect) -> bool {
        self.pressed.is_none() && r.contains(self.mouse)
    }
    fn is_press(&self, idx: usize) -> bool {
        self.pressed == Some(idx)
    }
}

impl ManagePage {
    /// 画一节：头部（标题 + 旋转 chevron + 分隔线）+ 按展开度 `amt` 裁剪揭示的卡片列表。
    /// 返回该节内容底部 y。
    #[allow(clippy::too_many_arguments)]
    fn draw_section(&mut self, ctx: &mut PaintCtx, idx: usize, list: &[Plugin], x: f32, y: f32, w: f32, drive: &str, now: f64, view: Rect) -> f32 {
        let m = self.is_module();
        let enabled = idx == 0;
        let title = match (enabled, m) {
            (true, true) => "已启用模块",
            (true, false) => "已启用插件",
            (false, true) => "已禁用模块",
            (false, false) => "已禁用插件",
        };
        let empty_msg = match (enabled, m) {
            (true, true) => "暂无已启用的模块",
            (true, false) => "暂无已启用的插件",
            (false, true) => "暂无已禁用的模块",
            (false, false) => "暂无已禁用的插件",
        };
        let hdr_h = 34.0;
        let amt = self.exp_amt(idx, now);
        section_head(ctx, Rect::new(x, y, w, hdr_h), title, amt);
        self.hotspots.push((Rect::new(x, y, w, hdr_h), Act::ToggleSec(idx)));

        let cards_top = y + hdr_h + 8.0;
        let vis_h = self.section_full_h(list) * amt;
        if amt > 0.001 {
            let clipped = amt < 0.999;
            if clipped {
                // 动画揭示用抗锯齿裁剪（同 settings）：裁剪线逐帧移动不在像素间跳动。
                ctx.painter.push_clip_aa(Rect::new(view.x, cards_top, view.w, vis_h));
            }
            let mut cy = cards_top;
            if list.is_empty() {
                let tt = ctx.tokens;
                let _ = ctx.painter.draw_text_leading(empty_msg, TextStyle::BODY, Rect::new(x + 4.0, cy, w, 22.0), tt.text_tertiary);
            } else {
                for p in list {
                    let has_desc = self.mode != PluginMode::Edgeless && !p.describe.is_empty();
                    let card_h = if has_desc { 86.0 } else { 66.0 };
                    if cy + card_h >= view.y && cy <= view.bottom() {
                        cy = self.card(ctx, x, cy, w, p, enabled, drive);
                    } else {
                        cy += card_h + 8.0;
                    }
                }
            }
            if clipped {
                ctx.painter.pop_clip();
            }
        }
        cards_top + vis_h
    }

    pub fn paint(&mut self, ctx: &mut PaintCtx, area: Rect) {
        let title = self.mode.get_plugin_manage_name();
        let top = page_header(ctx, area, title);

        let drive = self.boot_drive_manager.read().get_current_drive();
        let drive = match drive {
            Some(d) => d,
            None => {
                self.hotspots.clear();
                empty_state(ctx, Rect::new(area.x, top, area.w, area.bottom() - top), '\u{EDA2}', "请先选择或安装启动盘");
                return;
            }
        };

        // 刷新逻辑
        let has_updating = !self.updating.read().is_empty();
        let should_refresh = if has_updating {
            false
        } else if self.need_refresh {
            true
        } else if let Some(last) = self.last_refresh {
            last.elapsed() > Duration::from_secs(2)
        } else {
            true
        };
        if should_refresh {
            let _ = self.plugin_manager.write().load_local_plugins(&drive);
            self.last_refresh = Some(Instant::now());
            self.need_refresh = false;
        }

        // 列表缓存：动画期间不重读/克隆（避免每帧锁 + 深拷贝）；非动画帧 / 需刷新时更新。
        let now = ctx.now;
        self.scroll.tick(now); // 平滑滚动每帧推进
        if should_refresh || !self.is_animating(now) {
            self.cached_enabled = self.plugin_manager.read().get_enabled_plugins().clone();
            self.cached_disabled = self.plugin_manager.read().get_disabled_plugins().clone();
        }
        let enabled_list = std::mem::take(&mut self.cached_enabled);
        let disabled_list = std::mem::take(&mut self.cached_disabled);

        let view = Rect::new(area.x, top, area.w, area.bottom() - top);
        self.view = view;
        ctx.painter.push_clip(view);
        self.hotspots.clear();

        let x = area.x + 8.0;
        let w = area.w - 24.0;
        let mut y = top - self.scroll.offset;

        y = self.draw_section(ctx, 0, &enabled_list, x, y, w, &drive, now, view);
        y += 8.0;
        y = self.draw_section(ctx, 1, &disabled_list, x, y, w, &drive, now, view);
        y += 16.0;

        self.content_h = (y + self.scroll.offset) - top;
        self.scroll.clamp((self.content_h - view.h).max(0.0));
        ctx.painter.pop_clip();
        draw_scrollbar(ctx, &self.scroll, view, self.content_h);

        // 归还缓存列表（mem::take 借出后放回，避免克隆）。
        self.cached_enabled = enabled_list;
        self.cached_disabled = disabled_list;
    }

    pub fn paint_overlay(&mut self, _ctx: &mut PaintCtx, _area: Rect) {}

    pub fn on_event(&mut self, ev: InputEvent, now: f64, area: Rect) -> EventResult {
        if self.scroll.scrollbar_event(ev, self.view, self.content_h) {
            return EventResult::REDRAW;
        }
        match ev {
            InputEvent::PointerMove(p) => {
                self.mouse = p;
                return EventResult::REDRAW;
            }
            InputEvent::Wheel(d) => {
                let top = area.y + 60.0;
                let max = (self.content_h - (area.bottom() - top)).max(0.0);
                if self.scroll.apply_wheel(d, max) {
                    return EventResult::REDRAW;
                }
            }
            InputEvent::PointerDown(p) => {
                if self.view.contains(p) {
                    self.pressed = self.hotspots.iter().position(|(r, _)| r.contains(p));
                    return EventResult::REDRAW;
                }
            }
            InputEvent::PointerUp(p) => {
                if let Some(i) = self.pressed.take() {
                    let hit = self.hotspots.get(i).map(|(r, _)| r.contains(p)).unwrap_or(false);
                    if hit {
                        self.exec(i, now);
                    }
                    return EventResult { redraw: true, animating: true };
                }
            }
            InputEvent::PointerLeave => {
                self.pressed = None;
            }
            _ => {}
        }
        EventResult::NONE
    }

    fn exec(&mut self, i: usize, now: f64) {
        let drive = self.boot_drive_manager.read().get_current_drive();
        // 把动作克隆成自有值，结束对 hotspots 的借用，再可变借用 self。
        enum Cmd {
            Toggle(usize),
            Enable(String),
            Disable(String),
            Update(Plugin),
        }
        let cmd = match self.hotspots.get(i) {
            Some((_, Act::ToggleSec(s))) => Cmd::Toggle(*s),
            Some((_, Act::Enable(f))) => Cmd::Enable(f.clone()),
            Some((_, Act::Disable(f))) => Cmd::Disable(f.clone()),
            Some((_, Act::Update(p))) => Cmd::Update(p.clone()),
            None => return,
        };
        match cmd {
            Cmd::Toggle(s) => {
                self.anim_from[s] = self.exp_amt(s, now); // 从当前展开度起算（连点也平滑）
                self.exp[s] = !self.exp[s];
                self.anim_start[s] = now;
            }
            Cmd::Enable(file) => {
                if let Some(d) = drive {
                    let _ = self.plugin_manager.write().enable_plugin(&d, &file);
                    self.need_refresh = true;
                }
            }
            Cmd::Disable(file) => {
                if let Some(d) = drive {
                    let _ = self.plugin_manager.write().disable_plugin(&d, &file);
                    self.need_refresh = true;
                }
            }
            Cmd::Update(plugin) => {
                if let Some(d) = drive {
                    self.update_plugin(plugin, d);
                }
            }
        }
    }

    pub fn is_animating(&self, now: f64) -> bool {
        let sections = (0..2).any(|i| {
            let s = self.anim_start[i];
            let dur = if self.exp[i] { EXPAND_DUR } else { COLLAPSE_DUR };
            s >= 0.0 && (now - s) < dur
        });
        sections || self.scroll.is_settling() || !self.updating.read().is_empty()
    }
}
