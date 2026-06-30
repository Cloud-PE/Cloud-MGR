//! 加载页：启动时做网络连通性检测（3 次重试 / 5s 超时 / 1s 间隔），
//! 期间显示「大标题 + 旋转环 + 正在加载...」；失败显示红字 + 关闭按钮；
//! 成功后委托给主界面 CloudMgrRoot（与 egui 版一致：加载页始终持有主界面并转发）。

use super::shell::CloudMgrRoot;
use super::widgets::Clicky;
use crate::config::AppConfig;
use crate::mode::PluginMode;
use fluentpx::controls::{Button, InfoBar, ProgressRing, Severity};
use fluentpx::typography::TextStyle;
use fluentpx::widget::*;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

pub struct LoadingRoot {
    is_loading: Arc<AtomicBool>,
    network_status: Arc<AtomicU8>, // 0=检测中, 1=成功, 2=失败
    mode: PluginMode,
    shell: CloudMgrRoot,
    ring: ProgressRing,
    close_btn: Clicky,
    area: Rect,
}

impl LoadingRoot {
    pub fn new(runtime: Arc<Runtime>, mode: PluginMode, config: Arc<RwLock<AppConfig>>) -> Self {
        let is_loading = Arc::new(AtomicBool::new(true));
        let network_status = Arc::new(AtomicU8::new(0));

        // 后台网络连通性检测。
        {
            let is_loading_c = is_loading.clone();
            let status_c = network_status.clone();
            runtime.spawn(async move {
                let mut retry = 0;
                let mut success = false;
                let url = mode.get_connect_test_url().to_string();
                while retry < 3 {
                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(5))
                        .build()
                        .unwrap_or_else(|_| reqwest::Client::new());
                    if let Ok(resp) = client.get(&url).send().await {
                        if let Ok(text) = resp.text().await {
                            if !text.is_empty() {
                                success = true;
                                break;
                            }
                        }
                    }
                    retry += 1;
                    if retry < 3 {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
                if success {
                    status_c.store(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                } else {
                    status_c.store(2, Ordering::Relaxed);
                }
                is_loading_c.store(false, Ordering::Relaxed);
            });
        }

        // 主界面（内部会立刻发起插件列表拉取，与网络检测并行）。
        let shell = CloudMgrRoot::new(runtime, mode, config);

        Self {
            is_loading,
            network_status,
            mode,
            shell,
            ring: ProgressRing::new(),
            close_btn: Clicky::new(Button::standard("关闭")),
            area: Rect::default(),
        }
    }

    fn done(&self) -> bool {
        !self.is_loading.load(Ordering::Relaxed) && self.network_status.load(Ordering::Relaxed) != 2
    }
    fn failed(&self) -> bool {
        self.network_status.load(Ordering::Relaxed) == 2
    }
}

fn big_title() -> TextStyle {
    TextStyle { size: 48.0, line_height: 60.0, ..TextStyle::TITLE_LARGE }
}

impl Widget for LoadingRoot {
    fn measure(&mut self, available: Size) -> Size {
        available
    }
    fn arrange(&mut self, r: Rect) {
        self.area = r;
        self.shell.arrange(r);
    }
    fn hit_test(&self, p: Point) -> bool {
        self.area.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        if self.done() {
            self.shell.paint(ctx);
            return;
        }
        let t = ctx.tokens;
        let a = self.area;
        // 标题放在上三分之一（不居中下沉）。
        let cy = a.y + a.h * 0.26;
        // Failed state gets its own offsets so the close button can sit lower.
        let title = self.mode.get_server_name();
        let title_y = if self.failed() { a.y + a.h * 0.22 } else { cy };
        let _ = ctx.painter.draw_text_centered(title, big_title(), Rect::new(a.x, title_y, a.w, 64.0), t.text_primary);

        if self.failed() {
            // 连接失败：用 Critical InfoBar 报错（1:1），下方保留「关闭」按钮。
            let msg = format!("无法连接至 {} 服务器，请检查网络连接或联系开发人员。", self.mode.get_server_name());
            let bw_bar = (a.w - 80.0).min(560.0);
            let bx = a.x + (a.w - bw_bar) / 2.0;
            let mut ib = InfoBar::new(Severity::Critical, "连接失败", msg);
            ib.closable = false;
            let bh = ib.measure(Size { w: bw_bar, h: 0.0 }).h;
            let bar_y = title_y + 74.0;
            ib.arrange(Rect::new(bx, bar_y, bw_bar, bh));
            ib.paint(ctx);
            let bw = 120.0;
            self.close_btn.arrange(Rect::new(a.x + (a.w - bw) / 2.0, bar_y + bh + 110.0, bw, 32.0));
            self.close_btn.paint(ctx);
        } else {
            // 加载圈：更小，放在窗口下方（与上方标题拉开大间距）。
            let rr = 24.0;
            let ring_y = a.y + a.h * 0.63;
            self.ring.arrange(Rect::new(a.x + (a.w - rr) / 2.0, ring_y, rr, rr));
            self.ring.paint(ctx);
        }
    }

    fn paint_overlay(&mut self, ctx: &mut PaintCtx) {
        if self.done() {
            self.shell.paint_overlay(ctx);
        }
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        if self.done() {
            return self.shell.on_event(ev, now);
        }
        if self.failed() {
            let (r, clicked) = self.close_btn.event(ev, now);
            if clicked {
                std::process::exit(0);
            }
            return r;
        }
        EventResult::NONE
    }

    fn is_animating(&self, now: f64) -> bool {
        if self.done() {
            self.shell.is_animating(now)
        } else if self.failed() {
            self.close_btn.is_animating(now)
        } else {
            true // 加载中：旋转环 + 轮询网络状态
        }
    }

    fn wants_modal(&self) -> bool {
        self.done() && self.shell.wants_modal()
    }
    fn wants_keyboard(&self) -> bool {
        self.done() && self.shell.wants_keyboard()
    }
    fn cursor_at(&self, p: Point) -> Cursor {
        if self.done() {
            self.shell.cursor_at(p)
        } else {
            Cursor::Default
        }
    }
    fn caret_pos(&self) -> Option<Point> {
        if self.done() {
            self.shell.caret_pos()
        } else {
            None
        }
    }
    fn set_composition(&mut self, s: &str) {
        if self.done() {
            self.shell.set_composition(s);
        }
    }
    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Dialog
    }
}
