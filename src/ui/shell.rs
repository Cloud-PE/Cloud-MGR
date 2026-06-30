//! 主界面骨架：左侧 NavigationView（插件市场 / 插件管理 / 设置）+ 右侧内容区。
//! 首次启动若检测到多个启动盘且无默认，弹出「选择启动盘」模态。

use super::manage::ManagePage;
use super::market::MarketPage;
use super::settings::SettingsPage;
use super::widgets::Clicky;
use crate::config::AppConfig;
use crate::mode::PluginMode;
use crate::plugins::PluginManager;
use crate::utils::{BootDrive, BootDriveManager};
use fluentpx::controls::{Button, CheckBox, CheckState, ComboBox, NavItem, NavigationView};
use fluentpx::gfx::Icon;
use fluentpx::typography::TextStyle;
use fluentpx::widget::*;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::runtime::Runtime;

pub struct CloudMgrRoot {
    config: Arc<RwLock<AppConfig>>,
    plugin_manager: Arc<RwLock<PluginManager>>,
    boot_drive_manager: Arc<RwLock<BootDriveManager>>,

    nav: NavigationView,
    market: MarketPage,
    manage: ManagePage,
    settings: SettingsPage,

    // 首次启动启动盘选择模态
    show_boot_dialog: bool,
    boot_drives: Vec<BootDrive>,
    boot_combo: ComboBox,
    boot_save: CheckBox,
    boot_ok: Clicky,

    area: Rect,
}

impl CloudMgrRoot {
    pub fn new(runtime: Arc<Runtime>, mode: PluginMode, config: Arc<RwLock<AppConfig>>) -> Self {
        let boot_drive_manager = Arc::new(RwLock::new(BootDriveManager::new(mode)));
        let plugin_manager = Arc::new(RwLock::new(PluginManager::new(mode)));

        let boot_drives = boot_drive_manager.read().get_all_drives();
        let is_first_launch = boot_drives.len() > 1 && config.read().default_boot_drive.is_none();

        if !is_first_launch {
            let default = config.read().default_boot_drive.clone();
            if let Some(default) = default {
                boot_drive_manager.write().set_current_drive(default.clone());
                let _ = plugin_manager.write().load_local_plugins(&default);
            } else if boot_drives.len() == 1 {
                let letter = boot_drives[0].letter.clone();
                boot_drive_manager.write().set_current_drive(letter.clone());
                config.write().default_boot_drive = Some(letter.clone());
                config.write().save().ok();
                let _ = plugin_manager.write().load_local_plugins(&letter);
            }
        }

        let market = MarketPage::new(plugin_manager.clone(), config.clone(), runtime.clone(), boot_drive_manager.clone(), mode);
        let manage = ManagePage::new(plugin_manager.clone(), boot_drive_manager.clone(), mode, runtime.clone(), config.clone());
        let settings = SettingsPage::new(config.clone(), boot_drive_manager.clone(), mode);

        let nav = NavigationView::shell(
            vec![
                NavItem { icon: Icon::Home, label: mode.get_plugin_market_name().to_string() },
                NavItem { icon: Icon::Folder, label: mode.get_plugin_manage_name().to_string() },
                NavItem { icon: Icon::Settings, label: "设置".to_string() },
            ],
            0,
        );

        let drive_letters: Vec<String> = boot_drives.iter().map(|d| d.letter.clone()).collect();

        Self {
            config,
            plugin_manager,
            boot_drive_manager,
            nav,
            market,
            manage,
            settings,
            show_boot_dialog: is_first_launch,
            boot_drives,
            boot_combo: ComboBox::new(drive_letters, 0),
            boot_save: CheckBox::new("把这项选择设为默认值", CheckState::Unchecked),
            boot_ok: Clicky::new(Button::accent("确定")),
            area: Rect::default(),
        }
    }

    /// 开发期：注入样例数据 + 设当前盘，供离屏截图自检（不联网/不读盘）。
    pub fn inject_sample(&mut self) {
        use crate::plugins::{Plugin, PluginCategory};
        let mk = |name: &str, ver: &str, auth: &str, desc: &str, size: &str| Plugin {
            name: name.into(),
            size: size.into(),
            version: ver.into(),
            author: auth.into(),
            describe: desc.into(),
            file: String::new(),
            link: "https://example.com/x".into(),
        };
        let cats = vec![
            PluginCategory {
                class: "推荐".into(),
                icon: None,
                list: vec![
                    mk("DiskGenius", "5.5.0", "CloudPE", "磁盘分区与数据恢复工具", "12.34 MB"),
                    mk("CPU-Z", "2.06", "CPUID", "处理器与主板信息检测", "2.10 MB"),
                    mk("7-Zip", "23.01", "Igor Pavlov", "开源压缩解压工具", "1.50 MB"),
                ],
            },
            PluginCategory {
                class: "系统工具".into(),
                icon: None,
                list: vec![mk("Dism++", "10.1.1", "初雨团队", "系统精简、优化与备份", "8.20 MB")],
            },
        ];
        self.plugin_manager.write().categories = cats;
        self.boot_drive_manager.write().set_current_drive("E:".into());
    }

    pub fn set_page(&mut self, i: usize) {
        self.nav.selected = i;
        self.show_boot_dialog = false;
    }

    fn confirm_boot_drive(&mut self) {
        if let Some(d) = self.boot_drives.get(self.boot_combo.selected) {
            let drive = d.letter.clone();
            self.boot_drive_manager.write().set_current_drive(drive.clone());
            let _ = self.plugin_manager.write().load_local_plugins(&drive);
            if self.boot_save.state == CheckState::Checked {
                self.config.write().default_boot_drive = Some(drive);
                self.config.write().save().ok();
            }
            self.show_boot_dialog = false;
        }
    }

    fn boot_dialog_rect(&self) -> Rect {
        let (dw, dh) = (380.0, 230.0);
        Rect::new(self.area.x + (self.area.w - dw) / 2.0, self.area.y + (self.area.h - dh) / 2.0, dw, dh)
    }

    /// 当前页绘制 / 事件用的内容区矩形（在导航内容区基础上加内边距，避免标题贴住导航栏）。
    fn content(&self, now: f64) -> Rect {
        let r = self.nav.content_area(now);
        Rect::new(r.x + 24.0, r.y + 16.0, (r.w - 44.0).max(0.0), (r.h - 24.0).max(0.0))
    }
}

impl Widget for CloudMgrRoot {
    fn measure(&mut self, available: Size) -> Size {
        available
    }
    fn arrange(&mut self, r: Rect) {
        self.area = r;
        self.nav.arrange(r);
    }
    fn hit_test(&self, p: Point) -> bool {
        self.area.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        self.nav.paint(ctx);
        let c = self.content(ctx.now);
        match self.nav.selected {
            0 => self.market.paint(ctx, c),
            1 => self.manage.paint(ctx, c),
            _ => self.settings.paint(ctx, c),
        }
    }

    fn paint_overlay(&mut self, ctx: &mut PaintCtx) {
        let c = self.content(ctx.now);
        match self.nav.selected {
            0 => self.market.paint_overlay(ctx, c),
            1 => self.manage.paint_overlay(ctx, c),
            _ => self.settings.paint_overlay(ctx, c),
        }

        if self.show_boot_dialog {
            let t = ctx.tokens;
            let vp = ctx.viewport;
            ctx.painter.fill_rect(Rect::new(0.0, 0.0, vp.w, vp.h), t.smoke_fill_default);
            let d = self.boot_dialog_rect();
            // 对话框投影（真·D2D 高斯）：在填充本体之前画。
            let (soff, sblur, scol) = t.dialog_shadow();
            ctx.painter.drop_shadow(d, 8.0, soff, sblur, scol);
            ctx.painter.fill_rounded_rect(d, 8.0, t.solid_bg_base);
            ctx.painter.stroke_inner(d, 8.0, t.surface_stroke_default, 1.0);
            let pad = 20.0;
            let _ = ctx.painter.draw_text_leading("选择启动盘", TextStyle::SUBTITLE, Rect::new(d.x + pad, d.y + pad, d.w - pad * 2.0, 28.0), t.text_primary);
            let _ = ctx.painter.draw_text_wrapped("检测到多个启动盘，请选择要使用的启动盘：", TextStyle::BODY, Rect::new(d.x + pad, d.y + 54.0, d.w - pad * 2.0, 24.0), t.text_secondary);
            self.boot_combo.arrange(Rect::new(d.x + pad, d.y + 86.0, d.w - pad * 2.0, 32.0));
            self.boot_combo.paint(ctx);
            self.boot_save.arrange(Rect::new(d.x + pad, d.y + 128.0, d.w - pad * 2.0, 32.0));
            self.boot_save.paint(ctx);
            let bw = 100.0;
            self.boot_ok.arrange(Rect::new(d.right() - pad - bw, d.bottom() - pad - 32.0, bw, 32.0));
            self.boot_ok.paint(ctx);
            // 组合框下拉浮层最后画（置顶）。
            self.boot_combo.paint_overlay(ctx);
        }
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if self.show_boot_dialog {
            // 下拉打开时独占事件，避免选项点击落到下方的复选框/确定按钮（与 settings 一致）。
            if self.boot_combo.open {
                let r = self.boot_combo.on_event(ev, now);
                return r.or(EventResult::REDRAW);
            }
            let r1 = self.boot_combo.on_event(ev, now);
            let r2 = self.boot_save.on_event(ev, now);
            let (r3, ok) = self.boot_ok.event(ev, now);
            if ok {
                self.confirm_boot_drive();
            }
            return r1.or(r2).or(r3).or(EventResult::REDRAW);
        }

        let prev_sel = self.nav.selected;
        let rn = self.nav.on_event(ev, now);
        if self.nav.selected != prev_sel {
            // 切到管理页时强制刷新本地列表（替代 egui 的 2s 空闲轮询）。
            if self.nav.selected == 1 {
                self.manage.mark_dirty();
            }
            return EventResult::REDRAW;
        }
        let c = self.content(now);
        let rp = match self.nav.selected {
            0 => self.market.on_event(ev, now, c),
            1 => self.manage.on_event(ev, now, c),
            _ => self.settings.on_event(ev, now, c),
        };
        rn.or(rp)
    }

    fn cursor_at(&self, p: Point) -> Cursor {
        // 仅市场页有搜索框；其余页 / 启动盘对话框用默认箭头。
        if self.show_boot_dialog || self.nav.selected != 0 {
            return Cursor::Default;
        }
        self.market.cursor_at(p)
    }

    fn caret_pos(&self) -> Option<Point> {
        if self.show_boot_dialog || self.nav.selected != 0 {
            return None;
        }
        self.market.caret_pos()
    }

    fn set_composition(&mut self, s: &str) {
        if !self.show_boot_dialog && self.nav.selected == 0 {
            self.market.set_composition(s);
        }
    }

    fn is_animating(&self, now: f64) -> bool {
        if self.show_boot_dialog {
            return self.boot_ok.is_animating(now) || self.boot_combo.is_animating(now);
        }
        // 任一页有进行中的任务都要继续泵帧（后台页的下载完成/刷新也能反映）。
        self.nav.is_animating(now)
            || self.market.is_animating(now)
            || self.manage.is_animating(now)
            || self.settings.is_animating(now)
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Dialog
    }
}
