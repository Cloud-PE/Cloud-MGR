//! 选择插件源（--select 模式，400×300）：三个源按钮（Cloud-PE/HotPE/Edgeless，
//! 带 ✓/✗ 可用性前缀）+「检测可用性」。选择某源 → 以对应参数重启自身进程并退出。

use super::widgets::Clicky;
use crate::mode::PluginMode;
use fluentpx::controls::Button;
use fluentpx::gfx::Icon;
use fluentpx::typography::TextStyle;
use fluentpx::widget::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

#[derive(Clone)]
struct SourceStatus {
    available: Option<bool>,
    checking: bool,
}

pub struct SelectorRoot {
    sources: Arc<RwLock<HashMap<PluginMode, SourceStatus>>>,
    is_checking: bool,
    runtime: Arc<Runtime>,
    cloud_btn: Clicky,
    hotpe_btn: Clicky,
    edgeless_btn: Clicky,
    check_btn: Clicky,
    area: Rect,
}

impl SelectorRoot {
    pub fn new(runtime: Arc<Runtime>) -> Self {
        let mut map = HashMap::new();
        map.insert(PluginMode::CloudPE, SourceStatus { available: None, checking: false });
        map.insert(PluginMode::HotPE, SourceStatus { available: None, checking: false });
        map.insert(PluginMode::Edgeless, SourceStatus { available: None, checking: false });
        Self {
            sources: Arc::new(RwLock::new(map)),
            is_checking: false,
            runtime,
            cloud_btn: Clicky::new(Button::standard("Cloud-PE")),
            hotpe_btn: Clicky::new(Button::standard("HotPE")),
            edgeless_btn: Clicky::new(Button::standard("Edgeless")),
            check_btn: Clicky::new(Button::standard("检测可用性")),
            area: Rect::default(),
        }
    }

    fn label(&self, mode: PluginMode, name: &str) -> String {
        match self.sources.read().get(&mode).and_then(|s| s.available) {
            Some(true) => format!("✓  {}", name),
            Some(false) => format!("✗  {}", name),
            None => name.to_string(),
        }
    }

    fn status_icon(&self, mode: PluginMode) -> Option<Icon> {
        match self.sources.read().get(&mode).and_then(|s| s.available) {
            Some(true) => Some(Icon::Success),
            Some(false) => Some(Icon::Error),
            None => None,
        }
    }

    fn paint_status_icon(&self, ctx: &mut PaintCtx, mode: PluginMode, r: Rect) {
        if let Some(icon) = self.status_icon(mode) {
            let size = 16.0;
            let icon_r = Rect::new(r.center_x() - 58.0, r.center_y() - size / 2.0, size, size);
            let color = match self.sources.read().get(&mode).and_then(|s| s.available) {
                Some(false) => ctx.tokens.text_disabled,
                _ => ctx.tokens.text_primary,
            };
            ctx.painter.draw_glyph(icon, icon_r, color);
        }
    }

    fn launch_mode(mode: PluginMode) {
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(_) => return,
        };
        let arg = match mode {
            PluginMode::CloudPE => "",
            PluginMode::HotPE => "--hpm",
            PluginMode::Edgeless => "--edgeless",
            _ => return,
        };
        if arg.is_empty() {
            Command::new(exe).spawn().ok();
        } else {
            Command::new(exe).arg(arg).spawn().ok();
        }
        std::process::exit(0);
    }

    fn check_availability(&mut self) {
        if self.is_checking {
            return;
        }
        self.is_checking = true;
        {
            let mut s = self.sources.write();
            for v in s.values_mut() {
                v.checking = true;
                v.available = None;
            }
        }
        for m in [PluginMode::CloudPE, PluginMode::HotPE, PluginMode::Edgeless] {
            let sc = self.sources.clone();
            self.runtime.spawn(async move {
                let avail = check_source_async(m).await;
                let mut s = sc.write();
                if let Some(st) = s.get_mut(&m) {
                    st.available = Some(avail);
                    st.checking = false;
                }
            });
        }
    }

    /// 检测完成扫描：所有源都 !checking 则清 is_checking。
    fn sweep(&mut self) {
        if self.is_checking {
            let all_done = self.sources.read().values().all(|s| !s.checking);
            if all_done {
                self.is_checking = false;
            }
        }
    }
}

async fn check_source_async(mode: PluginMode) -> bool {
    let url = mode.get_connect_test_url();
    if url.is_empty() {
        return false;
    }
    let mut retry = 0;
    while retry < 3 {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        if let Ok(resp) = client.get(url).send().await {
            if let Ok(text) = resp.text().await {
                if !text.is_empty() {
                    return true;
                }
            }
        }
        retry += 1;
        if retry < 3 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    false
}

impl Widget for SelectorRoot {
    fn measure(&mut self, available: Size) -> Size {
        available
    }
    fn arrange(&mut self, r: Rect) {
        self.area = r;
    }
    fn hit_test(&self, p: Point) -> bool {
        self.area.contains(p)
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        self.sweep();
        let t = ctx.tokens;
        let a = self.area;
        let cx = a.x + a.w / 2.0;

        let _ = ctx.painter.draw_text_centered("选择插件源", TextStyle::SUBTITLE, Rect::new(a.x, a.y + 18.0, a.w, 28.0), t.text_primary);
        ctx.painter.draw_line(a.x + 20.0, a.y + 54.0, a.right() - 20.0, a.y + 54.0, t.divider_stroke_default, 1.0);

        // 刷新按钮文本（带可用性前缀）。
        self.cloud_btn.btn.text = self.label(PluginMode::CloudPE, "Cloud-PE");
        self.cloud_btn.btn.text = "Cloud-PE".into();
        self.hotpe_btn.btn.text = "HotPE".into();
        self.edgeless_btn.btn.text = "Edgeless".into();
        let source_enabled = |mode| {
            !self.is_checking && self.sources.read().get(&mode).and_then(|s| s.available) != Some(false)
        };
        let enabled = !self.is_checking;
        self.cloud_btn.set_enabled(source_enabled(PluginMode::CloudPE));
        self.hotpe_btn.set_enabled(source_enabled(PluginMode::HotPE));
        self.edgeless_btn.set_enabled(source_enabled(PluginMode::Edgeless));
        self.check_btn.btn.text = if self.is_checking { "检测中...".into() } else { "检测可用性".into() };
        self.check_btn.set_enabled(enabled);

        let bw = 200.0;
        let bx = cx - bw / 2.0;
        self.cloud_btn.arrange(Rect::new(bx, a.y + 72.0, bw, 40.0));
        self.hotpe_btn.arrange(Rect::new(bx, a.y + 122.0, bw, 40.0));
        self.edgeless_btn.arrange(Rect::new(bx, a.y + 172.0, bw, 40.0));
        self.cloud_btn.paint(ctx);
        self.hotpe_btn.paint(ctx);
        self.edgeless_btn.paint(ctx);
        self.paint_status_icon(ctx, PluginMode::CloudPE, Rect::new(bx, a.y + 72.0, bw, 40.0));
        self.paint_status_icon(ctx, PluginMode::HotPE, Rect::new(bx, a.y + 122.0, bw, 40.0));
        self.paint_status_icon(ctx, PluginMode::Edgeless, Rect::new(bx, a.y + 172.0, bw, 40.0));

        ctx.painter.draw_line(a.x + 20.0, a.y + 226.0, a.right() - 20.0, a.y + 226.0, t.divider_stroke_default, 1.0);
        let cw = 120.0;
        self.check_btn.arrange(Rect::new(cx - cw / 2.0, a.y + 240.0, cw, 32.0));
        self.check_btn.paint(ctx);
    }

    fn on_event(&mut self, ev: InputEvent, now: f64) -> EventResult {
        let (r1, c1) = self.cloud_btn.event(ev, now);
        let (r2, c2) = self.hotpe_btn.event(ev, now);
        let (r3, c3) = self.edgeless_btn.event(ev, now);
        let (r4, c4) = self.check_btn.event(ev, now);
        if !self.is_checking {
            if c1 {
                Self::launch_mode(PluginMode::CloudPE);
            }
            if c2 {
                Self::launch_mode(PluginMode::HotPE);
            }
            if c3 {
                Self::launch_mode(PluginMode::Edgeless);
            }
            if c4 {
                self.check_availability();
            }
        }
        r1.or(r2).or(r3).or(r4)
    }

    fn is_animating(&self, now: f64) -> bool {
        self.is_checking
            || self.cloud_btn.is_animating(now)
            || self.hotpe_btn.is_animating(now)
            || self.edgeless_btn.is_animating(now)
            || self.check_btn.is_animating(now)
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Dialog
    }
}
