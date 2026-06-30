//! 插件市场页：搜索框 +（可换行的）分类标签 + 插件卡列表。
//! 卡片操作：安装 / 已安装 / 更新（需启动盘）+ 下载（始终可用，下到用户选的文件夹）。
//! 列表拉取与下载/安装/更新均走 tokio 异步；进行中显示旋转环 + 禁用按钮。

use super::widgets::{btn_width, draw_card, draw_scrollbar, empty_state, generate_plugin_filename, imm_button, loading_button, Scroll};
use crate::config::AppConfig;
use crate::downloader::Downloader;
use crate::mode::PluginMode;
use crate::plugins::{Plugin, PluginManager};
use crate::utils::BootDriveManager;
use fluentpx::controls::{InfoBar, ProgressRing, Severity, TextBox};
use fluentpx::typography::TextStyle;
use fluentpx::widget::*;
use parking_lot::RwLock;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::runtime::Runtime;

#[derive(Clone, Copy, PartialEq)]
enum Status {
    NotInstalled,
    Installed,
    UpdateAvailable,
}

enum Act {
    Install(Plugin),
    Update(Plugin),
    Download(Plugin),
}

pub struct MarketPage {
    plugin_manager: Arc<RwLock<PluginManager>>,
    config: Arc<RwLock<AppConfig>>,
    runtime: Arc<Runtime>,
    boot_drive_manager: Arc<RwLock<BootDriveManager>>,
    mode: PluginMode,

    search: TextBox,
    selected_category: String,
    last_selected_category: String,
    show_search_category: bool,
    downloading: Arc<RwLock<HashSet<String>>>,
    is_loading: bool,
    /// 插件列表拉取失败（网断/取不到数据）→ 不再无限转圈，改显 Critical InfoBar 报错。
    fetch_failed: Arc<AtomicBool>,

    scroll: Scroll,
    content_h: f32,
    view: Rect,
    ring: ProgressRing,
    mouse: Point,
    pressed: Option<usize>,
    hotspots: Vec<(Rect, Act)>,
    tab_rects: Vec<(Rect, String)>,
}

impl MarketPage {
    pub fn new(
        plugin_manager: Arc<RwLock<PluginManager>>,
        config: Arc<RwLock<AppConfig>>,
        runtime: Arc<Runtime>,
        boot_drive_manager: Arc<RwLock<BootDriveManager>>,
        mode: PluginMode,
    ) -> Self {
        // 异步拉取插件列表，写入 plugin_manager.categories；失败置 fetch_failed（停转圈+报错）。
        let fetch_failed = Arc::new(AtomicBool::new(false));
        {
            let pm = plugin_manager.clone();
            let rt = runtime.clone();
            let failed = fetch_failed.clone();
            rt.spawn(async move {
                match PluginManager::fetch_plugins_async(mode).await {
                    Ok(categories) => pm.write().categories = categories,
                    Err(_) => failed.store(true, Ordering::Relaxed),
                }
            });
        }
        Self {
            plugin_manager,
            config,
            runtime,
            boot_drive_manager,
            mode,
            search: TextBox::new("输入关键词，回车搜索插件").with_search_icon(),
            selected_category: "推荐".into(),
            last_selected_category: "推荐".into(),
            show_search_category: false,
            downloading: Arc::new(RwLock::new(HashSet::new())),
            is_loading: true,
            fetch_failed,
            scroll: Scroll::new(),
            content_h: 0.0,
            view: Rect::default(),
            ring: ProgressRing::new(),
            mouse: Point::default(),
            pressed: None,
            hotspots: Vec::new(),
            tab_rects: Vec::new(),
        }
    }

    fn is_module(&self) -> bool {
        self.mode == PluginMode::HotPE
    }

    fn category_plugins(&self) -> Vec<Plugin> {
        let m = self.plugin_manager.read();
        m.get_categories().iter().find(|c| c.class == self.selected_category).map(|c| c.list.clone()).unwrap_or_default()
    }

    fn check_status(&self, plugin: &Plugin) -> Status {
        let m = self.plugin_manager.read();
        if let Some(local) = m.get_enabled_plugin_by_id(&plugin.get_plugin_id()) {
            match m.compare_versions(&local.version, &plugin.version) {
                std::cmp::Ordering::Less => Status::UpdateAvailable,
                _ => Status::Installed,
            }
        } else {
            Status::NotInstalled
        }
    }

    fn on_search_changed(&mut self) {
        if !self.search.text.is_empty() {
            if !self.show_search_category {
                self.show_search_category = true;
                if self.selected_category != "搜索" {
                    self.last_selected_category = self.selected_category.clone();
                }
                self.selected_category = "搜索".into();
            }
        } else if self.show_search_category {
            self.show_search_category = false;
            self.selected_category = self.last_selected_category.clone();
        }
    }

    fn is_hover(&self, r: Rect) -> bool {
        self.pressed.is_none() && r.contains(self.mouse)
    }
    fn is_press(&self, idx: usize) -> bool {
        self.pressed == Some(idx)
    }

    // ——— 异步：安装 / 更新 / 下载 ———

    fn install_plugin(&mut self, plugin: Plugin) {
        let task = format!("{}_install", plugin.get_plugin_id());
        self.downloading.write().insert(task.clone());
        let threads = self.config.read().download_threads;
        let drive = self.boot_drive_manager.read().get_current_drive();
        if let Some(drive) = drive {
            let url = plugin.link.clone();
            let filename = generate_plugin_filename(self.mode, &plugin);
            let (mode, pm, dl_set) = (self.mode, self.plugin_manager.clone(), self.downloading.clone());
            self.runtime.spawn(async move {
                let dir = format!("{}\\{}", drive, mode.get_plugin_folder());
                if tokio::fs::create_dir_all(&dir).await.is_err() {
                    dl_set.write().remove(&task);
                    return;
                }
                let path = PathBuf::from(&dir).join(format!("{}.{}", filename, mode.get_enabled_extension()));
                if Downloader::new(threads).download(&url, path).await.is_ok() {
                    let _ = pm.write().load_local_plugins(&drive);
                }
                dl_set.write().remove(&task);
            });
        } else {
            self.downloading.write().remove(&task);
        }
    }

    fn update_plugin(&mut self, plugin: Plugin) {
        let id = plugin.get_plugin_id();
        let task = format!("{}_update", id);
        self.downloading.write().insert(task.clone());
        let threads = self.config.read().download_threads;
        let drive = self.boot_drive_manager.read().get_current_drive();
        let old_file = self.plugin_manager.read().get_enabled_plugin_by_id(&id).map(|p| p.file.clone());
        if let Some(drive) = drive {
            let url = plugin.link.clone();
            let filename = generate_plugin_filename(self.mode, &plugin);
            let (mode, pm, dl_set) = (self.mode, self.plugin_manager.clone(), self.downloading.clone());
            self.runtime.spawn(async move {
                let dir = format!("{}\\{}", drive, mode.get_plugin_folder());
                if tokio::fs::create_dir_all(&dir).await.is_err() {
                    dl_set.write().remove(&task);
                    return;
                }
                if let Some(old) = old_file {
                    if pm.read().delete_plugin_file(&drive, &old).is_err() {
                        dl_set.write().remove(&task);
                        return;
                    }
                }
                let path = PathBuf::from(&dir).join(format!("{}.{}", filename, mode.get_enabled_extension()));
                if Downloader::new(threads).download(&url, path).await.is_ok() {
                    let _ = pm.write().load_local_plugins(&drive);
                }
                dl_set.write().remove(&task);
            });
        } else {
            self.downloading.write().remove(&task);
        }
    }

    fn download_plugin(&mut self, plugin: Plugin) {
        let task = format!("{}_download", plugin.get_plugin_id());
        self.downloading.write().insert(task.clone());
        let full = format!("{}.{}", generate_plugin_filename(self.mode, &plugin), self.mode.get_enabled_extension());
        let url = plugin.link.clone();
        let config = self.config.clone();
        let threads = self.config.read().download_threads;
        let dl_set = self.downloading.clone();
        let default_path = self.config.read().default_download_path.clone();
        self.runtime.spawn(async move {
            let dir = if let Some(p) = default_path {
                p
            } else {
                match rfd::AsyncFileDialog::new().set_title("选择下载位置").pick_folder().await {
                    Some(h) => {
                        let p = h.path().to_path_buf();
                        {
                            let mut c = config.write();
                            c.default_download_path = Some(p.clone());
                            let _ = c.save();
                        }
                        p
                    }
                    None => {
                        dl_set.write().remove(&task);
                        return;
                    }
                }
            };
            let _ = Downloader::new(threads).download(&url, dir.join(&full)).await;
            dl_set.write().remove(&task);
        });
    }

    /// 画一张插件卡 + 登记操作热区，返回底部 y。
    fn card(&mut self, ctx: &mut PaintCtx, x: f32, y: f32, w: f32, plugin: &Plugin) -> f32 {
        let has_desc = self.mode != PluginMode::Edgeless && !plugin.describe.is_empty();
        let card_h = if has_desc { 86.0 } else { 66.0 };
        let r = Rect::new(x, y, w, card_h);
        draw_card(ctx, r);
        let t = ctx.tokens;
        let ix = r.x + 12.0;
        // 文字列裁剪到按钮左侧，避免长描述/名称压到右侧安装·下载按钮上。
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

        let id = plugin.get_plugin_id();
        let down = self.downloading.read();
        let is_installing = down.contains(&format!("{}_install", id));
        let is_updating = down.contains(&format!("{}_update", id));
        let is_downloading = down.contains(&format!("{}_download", id));
        drop(down);
        let has_drive = self.boot_drive_manager.read().get_current_drive().is_some();

        let bh = 30.0;
        let by = r.center_y() - bh / 2.0;
        let mut bx = r.right() - 12.0;
        let gap = 8.0;

        // 下载（始终）
        {
            let bw = btn_width(ctx, "下载");
            bx -= bw;
            let br = Rect::new(bx, by, bw, bh);
            if is_downloading {
                loading_button(ctx, &mut self.ring, br);
            } else {
                let i = self.hotspots.len();
                imm_button(ctx, br, "下载", false, self.is_hover(br), self.is_press(i), true);
                self.hotspots.push((br, Act::Download(plugin.clone())));
            }
            bx -= gap;
        }

        // 安装 / 更新（需启动盘）
        if has_drive {
            match self.check_status(plugin) {
                Status::NotInstalled => {
                    let bw = btn_width(ctx, "安装");
                    bx -= bw;
                    let br = Rect::new(bx, by, bw, bh);
                    if is_installing {
                        loading_button(ctx, &mut self.ring, br);
                    } else {
                        let i = self.hotspots.len();
                        imm_button(ctx, br, "安装", true, self.is_hover(br), self.is_press(i), true);
                        self.hotspots.push((br, Act::Install(plugin.clone())));
                    }
                }
                Status::Installed => {
                    let bw = btn_width(ctx, "已安装");
                    bx -= bw;
                    imm_button(ctx, Rect::new(bx, by, bw, bh), "已安装", false, false, false, false);
                }
                Status::UpdateAvailable => {
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
                }
            }
        }
        r.bottom() + 8.0
    }

    fn exec(&mut self, i: usize) {
        enum Cmd {
            Install(Plugin),
            Update(Plugin),
            Download(Plugin),
        }
        let cmd = match self.hotspots.get(i) {
            Some((_, Act::Install(p))) => Cmd::Install(p.clone()),
            Some((_, Act::Update(p))) => Cmd::Update(p.clone()),
            Some((_, Act::Download(p))) => Cmd::Download(p.clone()),
            None => return,
        };
        match cmd {
            Cmd::Install(p) => self.install_plugin(p),
            Cmd::Update(p) => self.update_plugin(p),
            Cmd::Download(p) => self.download_plugin(p),
        }
    }
}

impl MarketPage {
    pub fn paint(&mut self, ctx: &mut PaintCtx, area: Rect) {
        self.scroll.tick(ctx.now); // 平滑滚动每帧推进
        // 加载→就绪
        if self.is_loading && !self.plugin_manager.read().get_categories().is_empty() {
            self.is_loading = false;
            let has_rec = self.plugin_manager.read().get_categories().iter().any(|c| c.class == "推荐");
            if !has_rec {
                if let Some(first) = self.plugin_manager.read().get_categories().first() {
                    self.selected_category = first.class.clone();
                    self.last_selected_category = first.class.clone();
                }
            }
        }

        let t = ctx.tokens;
        let title = self.mode.get_plugin_market_name();
        let _ = ctx.painter.draw_text_leading(title, TextStyle::TITLE, Rect::new(area.x, area.y, 260.0, 40.0), t.text_primary);
        // 搜索框：去掉「搜索：」标签，响应式铺满标题右侧到内容区右缘（拉宽变长、变窄缩短）。
        // 宽度取到内容右缘为止（不设最小宽）——否则展开 nav / 窄窗时会被最小宽顶出右缘=「一半显示不出来」。
        let search_x = area.x + 280.0;
        let search_w = (area.right() - search_x).max(0.0);
        self.search.arrange(Rect::new(search_x, area.y + 4.0, search_w, 32.0));
        self.search.paint(ctx);

        let mut y = area.y + 50.0;
        ctx.painter.draw_line(area.x, y, area.right(), y, t.divider_stroke_default, 1.0);
        y += 10.0;

        // 分类标签（换行）
        self.tab_rects.clear();
        if !self.is_loading {
            let categories: Vec<(String, bool)> = {
                let m = self.plugin_manager.read();
                let mut v: Vec<(String, bool)> = Vec::new();
                if self.show_search_category {
                    v.push(("搜索".to_string(), self.selected_category == "搜索"));
                }
                for c in m.get_categories() {
                    v.push((c.class.clone(), self.selected_category == c.class));
                }
                v
            };
            let pad = 14.0;
            let pill_h = 28.0;
            let gap = 8.0;
            let mut px = area.x;
            let mut py = y;
            for (name, selected) in &categories {
                let tw = ctx.painter.measure_text(name, TextStyle::BODY).map(|s| s.w).unwrap_or(name.chars().count() as f32 * 14.0);
                let pw = tw + pad * 2.0;
                if px + pw > area.right() && px > area.x {
                    px = area.x;
                    py += pill_h + gap;
                }
                let pr = Rect::new(px, py, pw, pill_h);
                let hov = pr.contains(self.mouse);
                let (bg, fg) = if *selected {
                    (t.accent_fill_default(), t.text_on_accent_primary)
                } else if hov {
                    (t.subtle_fill_secondary, t.text_primary)
                } else {
                    (t.control_fill_transparent, t.text_secondary)
                };
                ctx.painter.fill_rounded_rect(pr, pill_h / 2.0, bg);
                let _ = ctx.painter.draw_text_centered(name, TextStyle::BODY, pr, fg);
                self.tab_rects.push((pr, name.clone()));
                px += pw + gap;
            }
            y = py + pill_h + 10.0;
            ctx.painter.draw_line(area.x, y, area.right(), y, t.divider_stroke_default, 1.0);
            y += 10.0;
        }

        // 列表（滚动）
        let view = Rect::new(area.x, y, area.w, area.bottom() - y);
        self.view = view;
        ctx.painter.push_clip(view);
        self.hotspots.clear();

        if self.is_loading {
            if self.fetch_failed.load(Ordering::Relaxed) {
                // 拉取失败：不再无限转圈，改显 Critical 报错条（1:1 InfoBar）。
                let what = if self.is_module() { "模块" } else { "插件" };
                let mut ib = InfoBar::new(
                    Severity::Critical,
                    "加载失败",
                    format!("无法获取{}列表，请检查网络连接，稍后重新打开重试。", what),
                );
                ib.closable = false;
                let bh = ib.measure(Size { w: view.w, h: 0.0 }).h;
                ib.arrange(Rect::new(area.x, y + 8.0, view.w, bh));
                ib.paint(ctx);
                ctx.painter.pop_clip();
                self.content_h = 0.0;
                return;
            }
            let rr = 36.0;
            self.ring.arrange(Rect::new(area.x + (area.w - rr) / 2.0, y + 40.0, rr, rr));
            self.ring.paint(ctx);
            let txt = if self.is_module() { "正在加载模块列表..." } else { "正在加载插件列表..." };
            let tt = ctx.tokens;
            let _ = ctx.painter.draw_text_centered(txt, TextStyle::BODY, Rect::new(area.x, y + 84.0, area.w, 22.0), tt.text_secondary);
            ctx.painter.pop_clip();
            self.content_h = 0.0;
            return;
        }

        let plugins: Vec<Plugin> = if self.selected_category == "搜索" && !self.search.text.is_empty() {
            self.plugin_manager.read().search_plugins(&self.search.text)
        } else if self.selected_category != "搜索" {
            self.category_plugins()
        } else {
            Vec::new()
        };

        let x = area.x + 4.0;
        let w = area.w - 16.0;
        let mut cy = y - self.scroll.offset;

        if plugins.is_empty() {
            let searching = self.selected_category == "搜索";
            let txt = if searching {
                if self.is_module() { "未找到相关模块" } else { "未找到相关插件" }
            } else if self.is_module() {
                "该分类暂无模块"
            } else {
                "该分类暂无插件"
            };
            let glyph = if searching { '\u{E721}' } else { '\u{E8B7}' }; // 放大镜 / 文件夹
            empty_state(ctx, view, glyph, txt);
        } else {
            let mut seen = HashSet::new();
            for p in &plugins {
                let key = format!("{}_{}_{}_{}", p.name, p.version, p.author, p.size);
                if !seen.insert(key) {
                    continue;
                }
                let has_desc = self.mode != PluginMode::Edgeless && !p.describe.is_empty();
                let card_h = if has_desc { 86.0 } else { 66.0 };
                // 视口剔除：滚出可见区的卡片只占位、不绘制（滚动时把每帧文本绘制量从「全部」降到「可见的几张」）。
                if cy + card_h >= view.y && cy <= view.bottom() {
                    cy = self.card(ctx, x, cy, w, p);
                } else {
                    cy += card_h + 8.0;
                }
            }
        }

        self.content_h = (cy + self.scroll.offset) - y;
        self.scroll.clamp((self.content_h - view.h).max(0.0));
        ctx.painter.pop_clip();
        draw_scrollbar(ctx, &self.scroll, view, self.content_h);
    }

    pub fn paint_overlay(&mut self, ctx: &mut PaintCtx, _area: Rect) {
        self.search.paint_overlay(ctx);
    }

    pub fn on_event(&mut self, ev: InputEvent, now: f64, _area: Rect) -> EventResult {
        // 滚动条拖拽优先。
        if self.scroll.scrollbar_event(ev, self.view, self.content_h) {
            return EventResult::REDRAW;
        }
        // 搜索框（接收文本输入）。
        let before = self.search.text.clone();
        let rs = self.search.on_event(ev, now);
        if self.search.text != before {
            self.on_search_changed();
            return EventResult::REDRAW;
        }

        match ev {
            InputEvent::PointerMove(p) => {
                self.mouse = p;
                return rs.or(EventResult::REDRAW);
            }
            InputEvent::Wheel(d) => {
                let max = (self.content_h - self.view.h).max(0.0);
                if self.scroll.apply_wheel(d, max) {
                    return EventResult::REDRAW;
                }
            }
            InputEvent::PointerDown(p) => {
                // 分类标签优先
                if let Some((_, name)) = self.tab_rects.iter().find(|(r, _)| r.contains(p)) {
                    let name = name.clone();
                    self.selected_category = name.clone();
                    if !self.show_search_category || name != "搜索" {
                        self.last_selected_category = name;
                    }
                    return EventResult::REDRAW;
                }
                if self.view.contains(p) {
                    self.pressed = self.hotspots.iter().position(|(r, _)| r.contains(p));
                    return EventResult::REDRAW;
                }
            }
            InputEvent::PointerUp(p) => {
                if let Some(i) = self.pressed.take() {
                    let hit = self.hotspots.get(i).map(|(r, _)| r.contains(p)).unwrap_or(false);
                    if hit {
                        self.exec(i);
                    }
                    return EventResult::REDRAW;
                }
            }
            InputEvent::PointerLeave => {
                self.pressed = None;
            }
            _ => {}
        }
        rs
    }

    pub fn is_animating(&self, now: f64) -> bool {
        // 含 search.is_animating：聚焦时光标闪烁需要持续重绘（否则没有焦点反馈，像「点不动」）。
        // 拉取失败后不再算「加载中」——停转圈、转入静态报错条，避免空耗。
        let loading = self.is_loading && !self.fetch_failed.load(Ordering::Relaxed);
        self.scroll.is_settling() || loading || self.search.is_animating(now) || !self.downloading.read().is_empty()
    }

    /// 鼠标在搜索框内时返回 I 形文本光标（供宿主 WM_SETCURSOR）。
    pub fn cursor_at(&self, p: Point) -> Cursor {
        self.search.cursor_at(p)
    }

    /// 搜索框聚焦时的光标坐标（供宿主把 IME 候选窗定位到此处）。
    pub fn caret_pos(&self) -> Option<Point> {
        self.search.caret_pos()
    }

    /// 把 IME 内联组字串转发给搜索框（自绘在框内，不用系统浮窗）。
    pub fn set_composition(&mut self, s: &str) {
        self.search.set_composition(s);
    }
}
