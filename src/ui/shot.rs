//! 开发期离屏截图：用样例数据把主界面渲染成 PNG，供逐像素自检（不联网/不读盘/不需管理员窗口）。

use super::shell::CloudMgrRoot;
use crate::config::AppConfig;
use crate::mode::PluginMode;
use fluentpx::widget::{PaintCtx, Rect, Size, Widget};
use fluentpx::{Dpi, Theme};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::runtime::Runtime;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

pub fn shot(path: &str, mode: PluginMode, page: usize, light: bool) -> windows::core::Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let gfx = fluentpx::gfx::Gfx::new()?;
    let scale = 1.5_f32;
    // page 3 = 选择源（小窗），page 4 = 加载页；其余 = 主界面三页。
    let (lw, lh) = if page == 3 { (400.0_f32, 300.0_f32) } else { (1024.0_f32, 630.0_f32) };
    let mut off = gfx.create_offscreen((lw * scale) as u32, (lh * scale) as u32)?;
    let tokens = if light { Theme::Light.tokens() } else { Theme::Dark.tokens() };
    let dwrite = gfx.dwrite.clone();
    let icon_font = gfx.icon_font.clone();

    let rt = Arc::new(Runtime::new().unwrap());
    let config = Arc::new(RwLock::new(AppConfig::default()));

    let mut root: Box<dyn Widget> = if page == 3 {
        Box::new(super::SelectorRoot::new(rt))
    } else if page == 4 {
        Box::new(super::LoadingRoot::new(rt, mode, config))
    } else {
        let mut s = CloudMgrRoot::new(rt, mode, config);
        s.inject_sample();
        s.set_page(page);
        Box::new(s)
    };
    root.arrange(Rect::new(0.0, 0.0, lw, lh));
    {
        let mut p = off.begin(&dwrite, &icon_font, scale)?;
        p.clear(tokens.solid_bg_base);
        let mut ctx = PaintCtx { painter: &mut p, tokens: &tokens, dpi: Dpi::new(144), now: 1.0, viewport: Size { w: lw, h: lh } };
        root.paint(&mut ctx);
        root.paint_overlay(&mut ctx);
        let _ = p.end()?;
    }
    off.save_png(path)?;
    println!("wrote {path}");
    Ok(())
}
