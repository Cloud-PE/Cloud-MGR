//! 开发期工具：把 Expander 离屏渲染成 PNG，供逐像素自校验 / 与真·WinUI 比对。
//! 运行：cargo run --example render_shots -p fluentpx

use fluentpx::controls::{Button, ComboBox, Expander, InfoBar, Severity};
use fluentpx::gfx::{Gfx, Icon};
use fluentpx::widget::{Interaction, PaintCtx, Rect, Size, Widget};
use fluentpx::{Dpi, Theme};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

const OUT_DIR: &str = r"C:\Users\CJA-OS\AppData\Local\Temp\claude\E---------5-\9e60ccd2-09aa-49c1-8a1f-142d20717383\scratchpad";

fn rows() -> Vec<(String, String)> {
    vec![
        ("设备名称".into(), "LAPTOP-NJQF3BRR".into()),
        ("处理器".into(), "AMD Ryzen 9 7940H w/ Radeon 780M Graphics".into()),
        ("内存".into(), "16 GB（15 GB 可用）".into()),
        ("母盘".into(), "26100.1234".into()),
        ("Cloud-PE 版本".into(), "v10.0".into()),
    ]
}

fn shot(gfx: &Gfx, file: &str, theme: Theme, expanded: bool, now: f64) -> windows::core::Result<()> {
    let scale = 1.5_f32; // 150% DPI
    // 与 C++ 捕获器一致：背景 560 宽、20 内边距 → Expander 520 宽。无图标（裸控件）便于对比 chrome。
    let (lw, lh) = (560.0_f32, 280.0_f32);
    let (pw, ph) = ((lw * scale) as u32, (lh * scale) as u32);

    let mut off = gfx.create_offscreen(pw, ph)?;
    let tokens = theme.tokens();
    let dwrite = gfx.dwrite.clone();
    let icon_font = gfx.icon_font.clone();

    let mut exp = Expander::new("系统信息", None, rows(), expanded).with_label_col(120.0);

    {
        let mut painter = off.begin(&dwrite, &icon_font, scale)?;
        painter.clear(tokens.solid_bg_base);
        let viewport = Size { w: lw, h: lh };
        let mut ctx = PaintCtx { painter: &mut painter, tokens: &tokens, dpi: Dpi::new(144), now, viewport };
        let h = exp.height(now);
        exp.arrange(Rect::new(20.0, 20.0, lw - 40.0, h));
        exp.paint(&mut ctx);
        let _ = painter.end()?;
    }

    let path = format!("{OUT_DIR}\\{file}");
    off.save_png(&path)?;
    println!("wrote {path}");
    Ok(())
}

fn btn_shot(gfx: &Gfx, file: &str, accent: bool, hovered: bool, pressed: bool) -> windows::core::Result<()> {
    let scale = 1.5_f32;
    let (lw, lh) = (200.0_f32, 80.0_f32);
    let mut off = gfx.create_offscreen((lw * scale) as u32, (lh * scale) as u32)?;
    let tokens = Theme::Dark.tokens();
    let dwrite = gfx.dwrite.clone();
    let icon_font = gfx.icon_font.clone();

    let mut b = if accent { Button::accent("官方网站") } else { Button::standard("官方网站") };
    b.interaction = Interaction { hovered, pressed, focused: false, enabled: true };
    {
        let mut p = off.begin(&dwrite, &icon_font, scale)?;
        p.clear(tokens.solid_bg_base);
        let mut ctx = PaintCtx { painter: &mut p, tokens: &tokens, dpi: Dpi::new(144), now: 1000.0, viewport: Size { w: lw, h: lh } };
        b.arrange(Rect::new(20.0, 20.0, 120.0, 32.0));
        b.paint(&mut ctx);
        let _ = p.end()?;
    }
    off.save_png(&format!("{OUT_DIR}\\{file}"))?;
    println!("wrote {file}");
    Ok(())
}

fn bar_shot(gfx: &Gfx, file: &str, theme: Theme, severity: Severity, msg: &str, bar_h: f32) -> windows::core::Result<()> {
    let scale = 1.5_f32;
    let (lw, lh) = (560.0_f32, bar_h + 40.0);
    let mut off = gfx.create_offscreen((lw * scale) as u32, (lh * scale) as u32)?;
    let tokens = theme.tokens();
    let dwrite = gfx.dwrite.clone();
    let icon_font = gfx.icon_font.clone();

    let mut bar = InfoBar::new(severity, "", msg);
    bar.closable = false;
    {
        let mut p = off.begin(&dwrite, &icon_font, scale)?;
        p.clear(tokens.solid_bg_base);
        let mut ctx = PaintCtx { painter: &mut p, tokens: &tokens, dpi: Dpi::new(144), now: 1000.0, viewport: Size { w: lw, h: lh } };
        bar.arrange(Rect::new(20.0, 20.0, 520.0, bar_h));
        bar.paint(&mut ctx);
        let _ = p.end()?;
    }
    off.save_png(&format!("{OUT_DIR}\\{file}"))?;
    println!("wrote {file}");
    Ok(())
}

fn combo_shot(gfx: &Gfx, file: &str, reveal_now: f64) -> windows::core::Result<()> {
    let scale = 1.5_f32;
    let (lw, lh) = (300.0_f32, 230.0_f32);
    let mut off = gfx.create_offscreen((lw * scale) as u32, (lh * scale) as u32)?;
    let tokens = Theme::Dark.tokens();
    let dwrite = gfx.dwrite.clone();
    let icon_font = gfx.icon_font.clone();
    let mut cb = ComboBox::new(vec!["跟随系统（默认）".into(), "浅色模式".into(), "深色模式".into()], 1);
    cb.open = true;
    {
        let mut p = off.begin(&dwrite, &icon_font, scale)?;
        p.clear(tokens.solid_bg_base);
        let mut ctx = PaintCtx { painter: &mut p, tokens: &tokens, dpi: Dpi::new(144), now: reveal_now, viewport: Size { w: lw, h: lh } };
        cb.arrange(Rect::new(50.0, 105.0, 200.0, 32.0));
        cb.paint(&mut ctx);
        cb.paint_overlay(&mut ctx);
        let _ = p.end()?;
    }
    off.save_png(&format!("{OUT_DIR}\\{file}"))?;
    println!("wrote {file}");
    Ok(())
}

fn main() -> windows::core::Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let gfx = Gfx::new()?;
    combo_shot(&gfx, "combo_open.png", 10.0)?;
    combo_shot(&gfx, "combo_mid.png", 0.08)?;
    bar_shot(&gfx, "bar_info.png", Theme::Dark, Severity::Informational, "建议您升级至最新版Cloud-PE，以获得更优体验", 52.0)?;
    bar_shot(&gfx, "bar_error.png", Theme::Dark, Severity::Error, "您当前所使用的版本被标记为存在重大Bug的版本，建议您立即升级至最新版Cloud-PE以确保安全", 72.0)?;
    bar_shot(&gfx, "bar_info_light.png", Theme::Light, Severity::Informational, "建议您升级至最新版Cloud-PE，以获得更优体验", 52.0)?;
    bar_shot(&gfx, "bar_error_light.png", Theme::Light, Severity::Error, "您当前所使用的版本被标记为存在重大Bug的版本，建议您立即升级至最新版Cloud-PE以确保安全", 72.0)?;
    shot(&gfx, "exp_dark_expanded.png", Theme::Dark, true, 1000.0)?;
    shot(&gfx, "exp_dark_collapsed.png", Theme::Dark, false, 1000.0)?;
    shot(&gfx, "exp_light_expanded.png", Theme::Light, true, 1000.0)?;
    btn_shot(&gfx, "btn_accent_normal.png", true, false, false)?;
    btn_shot(&gfx, "btn_accent_hover.png", true, true, false)?;
    btn_shot(&gfx, "btn_accent_pressed.png", true, false, true)?;
    btn_shot(&gfx, "btn_std_normal.png", false, false, false)?;
    Ok(())
}
