//! 字体排印：WinUI 字号阶梯（type ramp）+ DirectWrite `IDWriteTextFormat`。
//!
//! 控件内容默认字号 `ControlContentThemeFontSize = 14`（Body）。
//! 字体族用可变字体 `Segoe UI Variable Text`（光学尺寸 Text 轴），回退 `Segoe UI`。

use std::cell::RefCell;
use std::collections::HashMap;
use windows::core::{Result, HSTRING};
use windows::Win32::Graphics::DirectWrite::*;

thread_local! {
    /// 进程内（UI 线程）缓存 IDWriteTextFormat：只有 ~8 种字号 × 少数缩放，
    /// 避免每帧、每个文字都重建 DirectWrite 文本格式（昂贵，是滚动/重绘卡顿的主因）。
    static FORMAT_CACHE: RefCell<HashMap<u64, IDWriteTextFormat>> = RefCell::new(HashMap::new());
}

fn format_key(style: TextStyle, scale: f32) -> u64 {
    let size_q = (style.size * scale * 4.0).round() as u64 & 0xFFFF;
    let lh_q = (style.line_height * scale * 4.0).round() as u64 & 0xFFFF;
    let w = style.weight.0 as u64 & 0xFFFF;
    size_q | (lh_q << 16) | (w << 32)
}

/// 主 UI 字体。按用户要求使用「微软雅黑 UI」，中英文均由其渲染（中文字形清晰）。
/// 回退 `Segoe UI`。注：这会让拉丁字母不再是 WinUI 默认的 Segoe UI Variable，
/// 属按需取舍；如需「拉丁=Segoe / 中文=雅黑」混排，可后续接 DirectWrite 自定义字体回退。
pub const FONT_FAMILY: &str = "Microsoft YaHei UI";
pub const FONT_FAMILY_FALLBACK: &str = "Segoe UI";

/// 内容控件默认字号 `ControlContentThemeFontSize`。
pub const CONTROL_CONTENT_FONT_SIZE: f32 = 14.0;

/// WinUI type ramp 的一档：字号 + 字重 + 行高（设备无关像素）。
#[derive(Clone, Copy)]
pub struct TextStyle {
    pub size: f32,
    pub weight: DWRITE_FONT_WEIGHT,
    pub line_height: f32,
}

impl TextStyle {
    pub const CAPTION: TextStyle = TextStyle { size: 12.0, weight: DWRITE_FONT_WEIGHT_NORMAL, line_height: 16.0 };
    pub const BODY: TextStyle = TextStyle { size: 14.0, weight: DWRITE_FONT_WEIGHT_NORMAL, line_height: 20.0 };
    pub const BODY_STRONG: TextStyle = TextStyle { size: 14.0, weight: DWRITE_FONT_WEIGHT_SEMI_BOLD, line_height: 20.0 };
    pub const BODY_LARGE: TextStyle = TextStyle { size: 18.0, weight: DWRITE_FONT_WEIGHT_NORMAL, line_height: 24.0 };
    pub const SUBTITLE: TextStyle = TextStyle { size: 20.0, weight: DWRITE_FONT_WEIGHT_SEMI_BOLD, line_height: 28.0 };
    pub const TITLE: TextStyle = TextStyle { size: 28.0, weight: DWRITE_FONT_WEIGHT_SEMI_BOLD, line_height: 36.0 };
    pub const TITLE_LARGE: TextStyle = TextStyle { size: 40.0, weight: DWRITE_FONT_WEIGHT_SEMI_BOLD, line_height: 52.0 };
    pub const DISPLAY: TextStyle = TextStyle { size: 68.0, weight: DWRITE_FONT_WEIGHT_SEMI_BOLD, line_height: 92.0 };
}

/// 用给定 DWrite 工厂创建一个 `IDWriteTextFormat`。
///
/// `scale` 为 DPI 缩放（dpi/96）：DirectWrite 以设备无关像素工作，但我们把
/// 字号按 scale 预乘，使整个渲染目标统一在设备像素上对齐（见 `dpi`）。
pub fn create_text_format(
    dwrite: &IDWriteFactory,
    style: TextStyle,
    scale: f32,
) -> Result<IDWriteTextFormat> {
    let key = format_key(style, scale);
    if let Some(f) = FORMAT_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Ok(f);
    }
    let family = HSTRING::from(FONT_FAMILY);
    let locale = HSTRING::from("zh-CN");
    let format = unsafe {
        let format = dwrite.CreateTextFormat(
            &family,
            None,
            style.weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            style.size * scale,
            &locale,
        )?;
        // 默认禁用换行（匹配按钮等单行内容）；换行变体会在绘制前显式置 WRAP，
        // 单行变体（draw_text）也会显式置回 NO_WRAP，故共享缓存格式始终被正确配置。
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        // 用 UNIFORM 行高，按字号比例锁定，避免不同字体回退导致行盒抖动。
        format.SetLineSpacing(
            DWRITE_LINE_SPACING_METHOD_UNIFORM,
            style.line_height * scale,
            style.line_height * scale * 0.8,
        )?;
        format
    };
    FORMAT_CACHE.with(|c| c.borrow_mut().insert(key, format.clone()));
    Ok(format)
}

thread_local! {
    /// 图标字形格式缓存（居中对齐）：按设备像素字号量化缓存，
    /// 避免每个图标每帧重建 DirectWrite 文本格式（图标多的页面是动画卡顿主因之一）。
    static ICON_FORMAT_CACHE: RefCell<HashMap<u64, IDWriteTextFormat>> = RefCell::new(HashMap::new());

    /// 自定义图标字体集合（由内嵌的 Segoe Fluent Icons 子集构建）。`None` = 用系统字体集合。
    /// 由 `gfx::Gfx::new` 在 UI 线程设置一次；`create_icon_format` 读取它解析图标字体族。
    static ICON_COLLECTION: RefCell<Option<IDWriteFontCollection>> = RefCell::new(None);
}

/// 设置图标字体集合（`gfx::Gfx::new` 构建内嵌图标集合后调用；传 `None` 表示回退系统字体）。
pub fn set_icon_collection(collection: Option<IDWriteFontCollection>) {
    ICON_COLLECTION.with(|c| *c.borrow_mut() = collection);
}

/// 图标字形的 `IDWriteTextFormat`（水平+垂直居中）。`size_scaled` = 逻辑字号 × scale（设备像素字号）。
/// 图标字体在本工程恒定（Segoe MDL2 Assets），故仅按字号缓存。
pub fn create_icon_format(dwrite: &IDWriteFactory, font: &str, size_scaled: f32) -> Result<IDWriteTextFormat> {
    let key = (size_scaled * 4.0).round() as u64;
    if let Some(f) = ICON_FORMAT_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Ok(f);
    }
    let family = HSTRING::from(font);
    let locale = HSTRING::from("en-US");
    // 有内嵌图标集合时用它解析字体族（与系统字体隔离）；否则传 None 用系统集合。
    let collection = ICON_COLLECTION.with(|c| c.borrow().clone());
    let format = unsafe {
        let f = match collection.as_ref() {
            Some(coll) => dwrite.CreateTextFormat(
                &family,
                coll,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size_scaled,
                &locale,
            )?,
            None => dwrite.CreateTextFormat(
                &family,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size_scaled,
                &locale,
            )?,
        };
        f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        f
    };
    ICON_FORMAT_CACHE.with(|c| c.borrow_mut().insert(key, format.clone()));
    Ok(format)
}
