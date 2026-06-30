//! Direct2D / DirectWrite 渲染引擎。
//!
//! 设计要点（对应规格第 5 节「像素对齐隐藏项」）：
//! * **坐标模型**：渲染目标 DPI 固定为 96，所有逻辑坐标在 [`Painter`] 内 × `scale`
//!   换算成设备像素后再绘制，从而对 1px 边框/分隔线做显式设备像素取整。
//! * **文字 AA / gamma**：[`TEXT_AA_MODE`] 与自定义 `IDWriteRenderingParams` 的 gamma
//!   集中可调，对照参考截图把字形边缘灰度调到一致。
//! * **1px 描边内缩**：D2D 描边以路径中心线为准，[`Painter::stroke_inner`] 把矩形内缩
//!   半个描边宽，使边框落在内沿（WinUI 的 InnerBorderEdge 行为）。
//! * **渐变边框**：[`Painter::fill_with_gradient_border`] 用 `ID2D1LinearGradientBrush`
//!   按源码 GradientStops 真值绘制立体高光边。

use core::ffi::c_void;
use std::collections::HashMap;
use windows::core::{IUnknown, Interface, Result};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::{BOOL, HMODULE, HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, IDXGIAdapter, IDXGIDevice, IDXGIFactory2, IDXGIOutput, IDXGISurface,
    IDXGISwapChain1, DXGI_CREATE_FACTORY_FLAGS, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
    DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

use crate::color::{Color, LinearGradient};
use crate::typography::{create_text_format, TextStyle};
use crate::widget::{Point, Rect, Size};

/// 文字抗锯齿模式：用 Direct2D 默认的 **ClearType**——系统级最锐利、与系统/Office/Edge 一致。
/// （旧版用灰度 AA + 自定义 gamma，会让文字发虚、边缘锯齿。）
pub const TEXT_AA_MODE: D2D1_TEXT_ANTIALIAS_MODE = D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE;

/// `D2DERR_RECREATE_TARGET`（`0x8899000C`）：渲染目标失效需重建。windows 0.58 未导出此具名常量，
/// 故按值定义（与改造前 `Painter::end` 用的字面量一致）。
const D2DERR_RECREATE_TARGET: windows::core::HRESULT = windows::core::HRESULT(0x8899_000Cu32 as i32);

/// 内嵌的 **Segoe Fluent Icons 子集**（仅含本工程实际用到的 14 个图标字形，~5KB）：
/// 运行时据此构建一个自定义 DirectWrite 字体集合，使图标渲染**不依赖系统是否安装该字体**
/// —— 在 WinPE 等精简环境也能正常显示，不再出现缺字方块。子集 ttf 在 `assets/` 下，
/// 其 name 表家族名已改为唯一的 [`ICON_FONT_FAMILY`]，与系统同名字体彻底隔离。
const ICON_FONT_SUBSET: &[u8] = include_bytes!("../assets/SegoeFluentIcons-subset.ttf");
/// 内嵌图标字体的家族名（子集已重命名为此唯一名，避免与系统字体冲突，也便于确认确走内嵌字体）。
const ICON_FONT_FAMILY: &str = "Fluentpx Icons";

/// 内置矢量图标（用 D2D 几何绘制，零字体依赖，Fluent 线性风格）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    ChevronDown,
    ChevronUp,
    Close,
    Hamburger,
    Home,
    Folder,
    Star,
    Settings,
    Info,
    Success,
    Warning,
    Error,
    /// 显示器/电脑（设备规格用）——矢量绘制。
    Computer,
    /// Windows 四方块徽标（Windows 规格用）——矢量绘制（Segoe 无品牌字形）。
    WindowsLogo,
}

impl Icon {
    /// 对应 Segoe Fluent Icons / Segoe MDL2 Assets 的码位（两套字体同码位）。
    pub fn codepoint(self) -> char {
        match self {
            Icon::ChevronDown => '\u{E70D}', // ChevronDown
            Icon::ChevronUp => '\u{E70E}',   // ChevronUp
            Icon::Close => '\u{E711}',       // Cancel（关闭 ✕）
            Icon::Hamburger => '\u{E700}',   // GlobalNavButton
            Icon::Home => '\u{E80F}',        // Home
            Icon::Folder => '\u{E8B7}',      // Folder
            Icon::Star => '\u{E734}',        // FavoriteStar
            Icon::Settings => '\u{E713}',    // Setting
            Icon::Info => '\u{E946}',        // Info
            Icon::Success => '\u{E930}',     // Completed
            Icon::Warning => '\u{E7BA}',     // Warning
            Icon::Error => '\u{EA39}',       // ErrorBadge
            // 下面两个无合适字形，统一走矢量（draw_glyph 特判），码位仅占位、不会被使用。
            Icon::Computer => '\u{E7F4}',
            Icon::WindowsLogo => '\u{E782}',
        }
    }
}

/// 进程级 D2D/DWrite 工厂（与窗口无关，可全局复用）。
///
/// `d2d` 升级为 **`ID2D1Factory1`**（它 `Deref` 到 `ID2D1Factory`，旧调用全部不变），
/// 以便组合（composition）后端用 `CreateDevice` 走 D3D 设备路径。
pub struct Gfx {
    pub d2d: ID2D1Factory1,
    pub dwrite: IDWriteFactory,
    /// 实际使用的图标字体族：内嵌子集时为 `ICON_FONT_FAMILY`，回退时为系统 Segoe Fluent Icons / MDL2。
    pub icon_font: String,
    /// 持有内嵌图标字体的内存加载器与集合：必须随进程存活（加载器不可注销），否则图标格式失效。
    _icon_loader: Option<IDWriteInMemoryFontFileLoader>,
    _icon_collection: Option<IDWriteFontCollection>,
}

/// 查询系统字体集中是否存在某字体族。
fn font_family_exists(dwrite: &IDWriteFactory, name: &str) -> bool {
    unsafe {
        let mut collection: Option<IDWriteFontCollection> = None;
        if dwrite.GetSystemFontCollection(&mut collection, false).is_err() {
            return false;
        }
        let Some(collection) = collection else { return false };
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut index = 0u32;
        let mut exists = windows::Win32::Foundation::BOOL(0);
        if collection
            .FindFamilyName(windows::core::PCWSTR(wide.as_ptr()), &mut index, &mut exists)
            .is_err()
        {
            return false;
        }
        exists.as_bool()
    }
}

/// 从内嵌的图标字体子集构建一个**自定义 DirectWrite 字体集合**（DWrite3 内存字体加载器）。
/// 成功返回 (加载器, 集合)：加载器需随集合一同存活（不可注销），故由 [`Gfx`] 长期持有。
/// 任一步失败（极老的 DWrite 无 `IDWriteFactory5`）返回 `None`，调用方回退系统字体集合。
fn build_icon_collection(
    dwrite: &IDWriteFactory,
    data: &'static [u8],
) -> Option<(IDWriteInMemoryFontFileLoader, IDWriteFontCollection)> {
    unsafe {
        let factory5: IDWriteFactory5 = dwrite.cast().ok()?;
        let loader: IDWriteInMemoryFontFileLoader = factory5.CreateInMemoryFontFileLoader().ok()?;
        factory5.RegisterFontFileLoader(&loader).ok()?;
        let font_file = loader
            .CreateInMemoryFontFileReference(dwrite, data.as_ptr() as *const c_void, data.len() as u32, None)
            .ok()?;
        let builder = factory5.CreateFontSetBuilder().ok()?;
        builder.AddFontFile(&font_file).ok()?;
        let font_set = builder.CreateFontSet().ok()?;
        let collection: IDWriteFontCollection1 = factory5.CreateFontCollectionFromFontSet(&font_set).ok()?;
        Some((loader, collection.into()))
    }
}

impl Gfx {
    pub fn new() -> Result<Gfx> {
        unsafe {
            // 注意：要让组合后端能 `factory.CreateDevice(dxgi)`，工厂必须是 `ID2D1Factory1`。
            // 它向下兼容 `ID2D1Factory`（Deref），故所有既有调用（CreatePathGeometry 等）不变。
            let d2d: ID2D1Factory1 =
                D2D1CreateFactory::<ID2D1Factory1>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED)?;
            // 图标字体：优先用**内嵌的 Segoe Fluent Icons 子集**构建自定义集合（不依赖系统字体，
            // WinPE 也能显示）。极少数环境构建失败时，回退系统字体（Win11=Fluent Icons / Win10=MDL2）。
            let (icon_loader, icon_collection, icon_font) =
                match build_icon_collection(&dwrite, ICON_FONT_SUBSET) {
                    Some((loader, coll)) => (Some(loader), Some(coll), ICON_FONT_FAMILY.to_string()),
                    None => {
                        let f = if font_family_exists(&dwrite, "Segoe Fluent Icons") {
                            "Segoe Fluent Icons".to_string()
                        } else {
                            "Segoe MDL2 Assets".to_string()
                        };
                        (None, None, f)
                    }
                };
            // 把集合登记到当前（UI）线程的图标格式工厂，create_icon_format 据此解析图标字体族。
            crate::typography::set_icon_collection(icon_collection.clone());
            Ok(Gfx { d2d, dwrite, icon_font, _icon_loader: icon_loader, _icon_collection: icon_collection })
        }
    }

    /// 为窗口创建/绑定渲染表面。`*_px` 为客户区设备像素尺寸。
    ///
    /// 优先建立 **DirectComposition + DXGI 翻转模型（flip-model）** 后端：vblank 同步 Present，
    /// 全窗动画无撕裂/无闪。任一组合调用失败（如最小化 PE、无 DWM、远程会话）时**回退**到
    /// 既有的 `ID2D1HwndRenderTarget`（BLT）路径——行为与改造前完全一致。
    pub fn create_surface(&self, hwnd: HWND, width_px: u32, height_px: u32) -> Result<Surface> {
        let diag = std::env::temp_dir().join("fluentpx_backend.txt");
        match self.create_composition_backend(hwnd, width_px, height_px) {
            Ok(comp) => {
                let _ = std::fs::write(&diag, "COMPOSITION active (DirectComposition + flip-model swapchain)");
                Ok(Surface {
                    backend: Backend::Composition(comp),
                    brush: None,
                    d2d: self.d2d.clone().into(),
                    images: HashMap::new(),
                })
            }
            Err(e) => {
                let _ = std::fs::write(&diag, format!("FALLBACK to HwndRenderTarget (BLT) — composition setup failed: {e:?}"));
                let hwnd_rt = self.create_hwnd_backend(hwnd, width_px, height_px)?;
                Ok(Surface {
                    backend: Backend::Hwnd(hwnd_rt),
                    brush: None,
                    d2d: self.d2d.clone().into(),
                    images: HashMap::new(),
                })
            }
        }
    }

    /// 回退路径：旧的 HWND 渲染目标（BLT present + DwmFlush 由宿主负责节奏）。
    fn create_hwnd_backend(&self, hwnd: HWND, width_px: u32, height_px: u32) -> Result<ID2D1HwndRenderTarget> {
        let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            // DPI 固定 96：逻辑坐标由 Painter 显式 × scale，便于像素取整。
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U { width: width_px.max(1), height: height_px.max(1) },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        let rt = unsafe { self.d2d.CreateHwndRenderTarget(&rt_props, &hwnd_props)? };
        // ClearType 抗锯齿；渲染参数用系统监视器默认（不覆盖 gamma/对比度），
        // 让文字与系统其它程序完全一致、最锐利。
        unsafe { rt.SetTextAntialiasMode(TEXT_AA_MODE) };
        if let Ok(params) = unsafe { self.dwrite.CreateRenderingParams() } {
            unsafe { rt.SetTextRenderingParams(&params) };
        }
        Ok(rt)
    }

    /// 组合后端：D3D11 设备 → DXGI flip-model 交换链（for composition）→ DComp 设备/目标/视觉
    /// → D2D 设备上下文（ID2D1DeviceContext，渲染进交换链后备缓冲）。
    fn create_composition_backend(&self, hwnd: HWND, width_px: u32, height_px: u32) -> Result<Composition> {
        unsafe {
            // 1) D3D11 设备：先 HARDWARE，失败回退 WARP；务必带 BGRA_SUPPORT 以兼容 D2D。
            let d3d_device = create_d3d11_device()?;
            let dxgi_device: IDXGIDevice = d3d_device.cast()?;

            // 2) D2D 设备 / 设备上下文（DC 即一个 ID2D1RenderTarget，Painter 直接可用）。
            let d2d_device = self.d2d.CreateDevice(&dxgi_device)?;
            let dc = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            dc.SetTextAntialiasMode(TEXT_AA_MODE);
            if let Ok(params) = self.dwrite.CreateRenderingParams() {
                let _ = dc.SetTextRenderingParams(&params);
            }

            // 3) flip-model 交换链（for composition）。不透明窗口 → AlphaMode IGNORE。
            let dxgi_factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width_px.max(1),
                Height: height_px.max(1),
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: BOOL(0),
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                Flags: 0,
            };
            // prestricttooutput: 无限制输出 → 显式 `Option<&IDXGIOutput>`（裸 None 无法推断）。
            let no_output: Option<&IDXGIOutput> = None;
            let swapchain: IDXGISwapChain1 =
                dxgi_factory.CreateSwapChainForComposition(&d3d_device, &desc, no_output)?;

            // 4) DirectComposition：设备 → HWND 目标 → 视觉，视觉内容设为交换链。
            let dcomp_device: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
            let dcomp_target = dcomp_device.CreateTargetForHwnd(hwnd, BOOL(1))?;
            let dcomp_visual = dcomp_device.CreateVisual()?;
            // SetContent 取 &IUnknown：交换链需向上转型。
            let content: IUnknown = swapchain.cast()?;
            dcomp_visual.SetContent(&content)?;
            dcomp_target.SetRoot(&dcomp_visual)?;
            dcomp_device.Commit()?;

            let mut comp = Composition {
                dc,
                swapchain,
                _dcomp_device: dcomp_device,
                _dcomp_target: dcomp_target,
                _dcomp_visual: dcomp_visual,
                target_bitmap: None,
            };
            // 5) 绑定后备缓冲为渲染目标。
            comp.bind_backbuffer()?;
            Ok(comp)
        }
    }
}

/// 组合后端的全部 GPU 资源。`dc` 是 `ID2D1DeviceContext`，它 `Deref` 到 `ID2D1RenderTarget`，
/// 故 [`Painter`] 持 `&ID2D1RenderTarget` 即指向本 DC，绘制 API 完全不变。
struct Composition {
    dc: ID2D1DeviceContext,
    swapchain: IDXGISwapChain1,
    // DComp 资源建好后只需保活（保持引用）；不再每帧触碰。
    _dcomp_device: IDCompositionDevice,
    _dcomp_target: IDCompositionTarget,
    _dcomp_visual: IDCompositionVisual,
    /// 当前绑定为渲染目标的后备缓冲位图；resize 时先解绑、丢弃、重建。
    target_bitmap: Option<ID2D1Bitmap1>,
}

impl Composition {
    /// 取交换链后备缓冲、包成 D2D 位图、设为 DC 的目标。(re)size 后都要重做。
    fn bind_backbuffer(&mut self) -> Result<()> {
        unsafe {
            let surface: IDXGISurface = self.swapchain.GetBuffer(0)?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    // 与交换链 AlphaMode IGNORE 对应：不透明窗口。
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: std::mem::ManuallyDrop::new(None),
            };
            let bitmap = self.dc.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            self.dc.SetTarget(&bitmap);
            self.target_bitmap = Some(bitmap);
            Ok(())
        }
    }

    fn resize(&mut self, width_px: u32, height_px: u32) -> Result<()> {
        unsafe {
            // 解绑并丢弃旧后备缓冲位图（否则 ResizeBuffers 因仍有未释放引用而失败）。
            // 裸 None 无法推断 `Param<ID2D1Image>`，显式标注。
            let clear_target: Option<&ID2D1Image> = None;
            self.dc.SetTarget(clear_target);
            self.target_bitmap = None;
            self.swapchain.ResizeBuffers(
                0, // 保持现有缓冲数
                width_px.max(1),
                height_px.max(1),
                DXGI_FORMAT_UNKNOWN, // 保持现有格式
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
        }
        // ResizeBuffers 不需要重新 Commit（视觉内容仍指向同一交换链）。
        self.bind_backbuffer()
    }
}

/// 渲染后端：优先组合（flip-model），失败回退 HWND BLT。
enum Backend {
    Composition(Composition),
    Hwnd(ID2D1HwndRenderTarget),
}

/// 与单个窗口绑定的渲染表面。
pub struct Surface {
    backend: Backend,
    /// 复用的纯色画刷（每次填充前 SetColor，省去反复创建）。
    brush: Option<ID2D1SolidColorBrush>,
    /// D2D 工厂（创建路径几何用）。`Painter` 持 `&ID2D1Factory`，这里降级存基类型。
    d2d: ID2D1Factory,
    /// 位图缓存（key → 已上传到本渲染目标的 `ID2D1Bitmap`），避免每帧重建。
    /// 随本 `Surface` 存活；目标重建（设备丢失/DPI 变化）时整体丢弃、自动失效。
    images: HashMap<u64, ID2D1Bitmap>,
}

impl Surface {
    /// 本表面是否走组合后端（决定宿主是否需要 DwmFlush 自行对齐 vblank）。
    /// 组合后端用 `Present(1,0)` 自带 vblank 节奏，宿主**不**应再 DwmFlush。
    pub fn paces_to_vblank(&self) -> bool {
        matches!(self.backend, Backend::Composition(_))
    }

    /// 当前的渲染目标（两种后端都 Deref 到 `ID2D1RenderTarget`）。
    fn render_target(&self) -> &ID2D1RenderTarget {
        match &self.backend {
            Backend::Composition(c) => &c.dc, // ID2D1DeviceContext: Deref<Target = ID2D1RenderTarget>
            Backend::Hwnd(rt) => rt,           // ID2D1HwndRenderTarget: Deref<Target = ID2D1RenderTarget>
        }
    }

    /// 客户区尺寸变化时调整后备缓冲。
    pub fn resize(&mut self, width_px: u32, height_px: u32) -> Result<()> {
        match &mut self.backend {
            Backend::Composition(c) => c.resize(width_px, height_px),
            Backend::Hwnd(rt) => unsafe {
                rt.Resize(&D2D_SIZE_U { width: width_px.max(1), height: height_px.max(1) })
            },
        }
    }

    /// 开一帧。返回的 [`Painter`] 需配对 [`Painter::end`]（end 内部做 EndDraw + Present）。
    ///
    /// `Painter::end` 自带 present：组合后端 `Present(1,0)`（vblank 同步、无撕裂），
    /// HWND 后端为隐式 BLT。故所有既有调用点（多个宿主 + 离屏）零改动即可工作。
    pub fn begin<'a>(&'a mut self, dwrite: &'a IDWriteFactory, icon_font: &'a str, scale: f32) -> Result<Painter<'a>> {
        // 先确保画刷存在（不可变借 rt 即可创建；与下面的借用拆分分离开）。
        if self.brush.is_none() {
            let b = unsafe { self.render_target().CreateSolidColorBrush(&Color::TRANSPARENT.d2d(), None)? };
            self.brush = Some(b);
        }
        // 借用拆分：rt / swapchain 取不可变引用，brush 不可变、images 可变——互不冲突。
        let (rt_ref, present): (&ID2D1RenderTarget, Option<&IDXGISwapChain1>) = match &self.backend {
            Backend::Composition(c) => (&c.dc, Some(&c.swapchain)),
            Backend::Hwnd(h) => (h, None),
        };
        unsafe { rt_ref.BeginDraw() };
        Ok(Painter {
            rt: rt_ref,
            d2d: &self.d2d,
            dwrite,
            brush: self.brush.as_ref().unwrap(),
            images: &mut self.images,
            icon_font,
            scale,
            present,
        })
    }
}

/// 创建 D3D11 设备：HARDWARE 优先，失败回退 WARP（软件光栅）。必带 BGRA_SUPPORT（D2D 互操作要求）。
fn create_d3d11_device() -> Result<ID3D11Device> {
    unsafe {
        let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
        // 显式类型：padapter 走 `Param<IDXGIAdapter>` 的 `Option<&T>` 实现（裸 None 无法推断 T）；
        // software 走 `Param<HMODULE>` 的 Copy 实现，传默认空句柄。
        let no_adapter: Option<&IDXGIAdapter> = None;
        let no_software: HMODULE = HMODULE::default();

        let mut device: Option<ID3D11Device> = None;
        let hr = D3D11CreateDevice(
            no_adapter,
            D3D_DRIVER_TYPE_HARDWARE,
            no_software,
            flags,
            None, // 默认特性级别链
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        );
        if hr.is_ok() {
            if let Some(d) = device {
                return Ok(d);
            }
        }
        // 回退 WARP（软件光栅；无 GPU / RDP 会话等）。
        let mut device: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            no_adapter,
            D3D_DRIVER_TYPE_WARP,
            no_software,
            flags,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )?;
        device.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))
    }
}

/// 离屏渲染目标（WIC 位图 + D2D）。用于把控件 headless 渲染成 PNG——
/// 开发期自校验 / 与真·WinUI 逐像素比对（不需要窗口、不需要管理员）。
/// 调用方需先 `CoInitializeEx`。
pub struct Offscreen {
    rt: ID2D1RenderTarget,
    wic: IWICImagingFactory,
    bitmap: IWICBitmap,
    brush: Option<ID2D1SolidColorBrush>,
    d2d: ID2D1Factory,
    images: HashMap<u64, ID2D1Bitmap>,
    width: u32,
    height: u32,
}

impl Gfx {
    /// 创建离屏渲染目标（`width_px`×`height_px` 设备像素）。
    pub fn create_offscreen(&self, width_px: u32, height_px: u32) -> Result<Offscreen> {
        unsafe {
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let bitmap = wic.CreateBitmap(width_px.max(1), height_px.max(1), &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let rt = self.d2d.CreateWicBitmapRenderTarget(&bitmap, &props)?;
            // 离屏用灰度 AA（无显示器子像素，避免文字彩边干扰目检/比对）。
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            Ok(Offscreen {
                rt,
                wic,
                bitmap,
                brush: None,
                // 工厂升级为 ID2D1Factory1，离屏目标仍只需基类型，向下转型。
                d2d: self.d2d.clone().into(),
                images: HashMap::new(),
                width: width_px.max(1),
                height: height_px.max(1),
            })
        }
    }
}

impl Offscreen {
    pub fn begin<'a>(&'a mut self, dwrite: &'a IDWriteFactory, icon_font: &'a str, scale: f32) -> Result<Painter<'a>> {
        if self.brush.is_none() {
            let b = unsafe { self.rt.CreateSolidColorBrush(&Color::TRANSPARENT.d2d(), None)? };
            self.brush = Some(b);
        }
        unsafe { self.rt.BeginDraw() };
        Ok(Painter {
            rt: &self.rt,
            d2d: &self.d2d,
            dwrite,
            brush: self.brush.as_ref().unwrap(),
            images: &mut self.images,
            icon_font,
            scale,
            present: None, // 离屏：EndDraw 后无 present。
        })
    }

    /// 把已渲染的位图编码为 PNG 写到 `path`。
    pub fn save_png(&self, path: &str) -> Result<()> {
        unsafe {
            let stream = self.wic.CreateStream()?;
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            stream.InitializeFromFilename(windows::core::PCWSTR(wide.as_ptr()), 0x4000_0000)?; // GENERIC_WRITE
            let encoder = self.wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
            encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
            let mut frame: Option<IWICBitmapFrameEncode> = None;
            encoder.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
            let frame = frame.unwrap();
            frame.Initialize(None)?;
            frame.SetSize(self.width, self.height)?;
            let mut fmt = GUID_WICPixelFormat32bppPBGRA;
            frame.SetPixelFormat(&mut fmt)?;
            frame.WriteSource(&self.bitmap, std::ptr::null())?;
            frame.Commit()?;
            encoder.Commit()?;
            Ok(())
        }
    }
}

/// 一帧的绘制器。输入坐标一律是逻辑像素，内部 × `scale` 取整到设备像素。
pub struct Painter<'a> {
    rt: &'a ID2D1RenderTarget,
    d2d: &'a ID2D1Factory,
    dwrite: &'a IDWriteFactory,
    brush: &'a ID2D1SolidColorBrush,
    images: &'a mut HashMap<u64, ID2D1Bitmap>,
    icon_font: &'a str,
    scale: f32,
    /// 组合后端的交换链：`end()` 在 EndDraw 后 `Present(1,0)`（vblank 同步）。
    /// HWND 后端 / 离屏为 `None`（EndDraw 即隐式 present / 无 present）。
    present: Option<&'a IDXGISwapChain1>,
}

impl<'a> Painter<'a> {
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// 逻辑→设备像素并取整（边缘对齐）。
    fn px(&self, v: f32) -> f32 {
        (v * self.scale).round()
    }

    /// 不取整的逻辑→设备（用于半像素描边定位）。
    fn dev(&self, v: f32) -> f32 {
        v * self.scale
    }

    fn dev_rect(&self, r: Rect) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.px(r.x),
            top: self.px(r.y),
            right: self.px(r.right()),
            bottom: self.px(r.bottom()),
        }
    }

    fn set_brush(&self, color: Color) {
        unsafe { self.brush.SetColor(&color.d2d()) };
    }

    /// 整屏清成某颜色。
    pub fn clear(&self, color: Color) {
        unsafe { self.rt.Clear(Some(&color.d2d())) };
    }

    /// 实心矩形。
    pub fn fill_rect(&self, r: Rect, color: Color) {
        if color.a == 0 {
            return;
        }
        self.set_brush(color);
        unsafe { self.rt.FillRectangle(&self.dev_rect(r), self.brush) };
    }

    /// 实心圆角矩形（`radius` 为逻辑像素角半径）。
    pub fn fill_rounded_rect(&self, r: Rect, radius: f32, color: Color) {
        if color.a == 0 {
            return;
        }
        self.set_brush(color);
        let rr = D2D1_ROUNDED_RECT {
            rect: self.dev_rect(r),
            radiusX: self.px(radius),
            radiusY: self.px(radius),
        };
        unsafe { self.rt.FillRoundedRectangle(&rr, self.brush) };
    }

    /// 软投影（CSS `box-shadow: 0 offset_y blur color`）。
    ///
    /// **组合后端**（`self.rt` 可转型为 `ID2D1DeviceContext`）走**真·高斯模糊**：把圆角矩形
    /// 形状不透明渲染进一个 `ID2D1CommandList`，喂给 `CLSID_D2D1Shadow` 效果（标准差 = blur/2、
    /// 阴影色 = `color`），再把效果输出按 `(0, offset_y)` 设备像素平移 `DrawImage` 出去——无带状、真柔边。
    ///
    /// **回退后端**（转型失败，HWND/离屏）保持原有**分层半透明圆角矩形**近似。
    /// 两条路径都遵循「**先画阴影，再画元素本体**」。
    pub fn drop_shadow(&self, r: Rect, corner: f32, offset_y: f32, blur: f32, color: Color) {
        if color.a == 0 || blur <= 0.5 {
            return;
        }
        // 组合后端：底层 COM 对象本就是 ID2D1DeviceContext，转型成功即可用效果管线。
        if let Ok(dc) = self.rt.cast::<ID2D1DeviceContext>() {
            if self.gaussian_shadow(&dc, r, corner, offset_y, blur, color).is_ok() {
                return;
            }
            // 效果路径失败（极少见）→ 落到下面的分层近似，保证仍有可见投影。
        }
        self.layered_shadow(r, corner, offset_y, blur, color);
    }

    /// 真·D2D 高斯阴影（仅组合后端）。命令列表录制形状 → Shadow 效果 → 平移 DrawImage。
    fn gaussian_shadow(
        &self,
        dc: &ID2D1DeviceContext,
        r: Rect,
        corner: f32,
        offset_y: f32,
        blur: f32,
        color: Color,
    ) -> Result<()> {
        unsafe {
            // 1) 录制：把形状不透明（a=255，仅作 alpha 掩膜，颜色由效果给定）画进命令列表。
            let cl = dc.CreateCommandList()?;
            let saved: ID2D1Image = dc.GetTarget()?;
            dc.SetTarget(&cl);
            // 形状画在其真实 rect（不在此处加 offset_y；位移由 DrawImage 统一施加）。
            let rr = D2D1_ROUNDED_RECT {
                rect: self.dev_rect(r),
                radiusX: self.px(corner),
                radiusY: self.px(corner),
            };
            self.set_brush(Color { a: 255, ..color });
            dc.FillRoundedRectangle(&rr, self.brush);
            dc.SetTarget(&saved);
            cl.Close()?;

            // 2) Shadow 效果：标准差 = blur/2（CSS 模糊半径 ≈ 2σ），单位设备像素；阴影色 = color。
            let fx = dc.CreateEffect(&CLSID_D2D1Shadow)?;
            fx.SetInput(0, &cl, true);
            let stddev: f32 = self.px(blur) / 2.0;
            fx.SetValue(
                D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32,
                D2D1_PROPERTY_TYPE_FLOAT,
                &stddev.to_ne_bytes(),
            )?;
            // 直通（非预乘）RGBA 0..1，与 Color::d2d() 一致。
            let mut col = [0u8; 16];
            col[0..4].copy_from_slice(&(color.r as f32 / 255.0).to_ne_bytes());
            col[4..8].copy_from_slice(&(color.g as f32 / 255.0).to_ne_bytes());
            col[8..12].copy_from_slice(&(color.b as f32 / 255.0).to_ne_bytes());
            col[12..16].copy_from_slice(&(color.a as f32 / 255.0).to_ne_bytes());
            fx.SetValue(
                D2D1_SHADOW_PROP_COLOR.0 as u32,
                D2D1_PROPERTY_TYPE_VECTOR4,
                &col,
            )?;

            // 3) 取效果输出，按 (0, offset_y 设备像素) 平移合成。
            let out: ID2D1Image = fx.GetOutput()?;
            let off = D2D_POINT_2F { x: 0.0, y: self.px(offset_y) };
            dc.DrawImage(
                &out,
                Some(&off as *const D2D_POINT_2F),
                None,
                D2D1_INTERPOLATION_MODE_LINEAR,
                D2D1_COMPOSITE_MODE_SOURCE_OVER,
            );
            Ok(())
        }
    }

    /// 分层半透明圆角矩形近似（回退后端用；无 D2D 效果依赖）。
    /// 每层不透明度 ≈ color.a/层数，全重叠处≈color.a，向外按层数递减 → 柔和衰减。
    fn layered_shadow(&self, r: Rect, corner: f32, offset_y: f32, blur: f32, color: Color) {
        // 层数随模糊增大（≈每 ~2px 一层），多层 + 低单层不透明度 → 平滑无带状。
        let steps: usize = (blur.round() as i32).clamp(10, 28) as usize;
        let layer_a = (((color.a as f32) / steps as f32).round() as i32).clamp(1, 255) as u8;
        let c = Color { a: layer_a, ..color };
        for i in 0..steps {
            let f = i as f32 / (steps - 1) as f32; // 0=最外(扩张大、falloff 末端) .. 1=贴边
            let grow = blur * (1.0 - f);
            let rect = Rect {
                x: r.x - grow,
                y: r.y + offset_y - grow,
                w: r.w + 2.0 * grow,
                h: r.h + 2.0 * grow,
            };
            self.fill_rounded_rect(rect, (corner + grow).max(0.0), c);
        }
    }

    /// 在矩形**内沿**描 1（或 n）逻辑像素边框（InnerBorderEdge）。
    /// 描边中心线内缩 thickness/2 设备像素，使外缘与矩形外缘对齐、得到清晰边。
    pub fn stroke_inner(&self, r: Rect, radius: f32, color: Color, thickness_logical: f32) {
        if color.a == 0 {
            return;
        }
        self.set_brush(color);
        // 描边厚度取整到**整数设备像素**：1px 逻辑边在 1.5× 下落成 2 个实心像素（清晰、不发虚）。
        let t = self.dev(thickness_logical).round().max(1.0);
        let half = t / 2.0;
        let rect = D2D_RECT_F {
            left: self.px(r.x) + half,
            top: self.px(r.y) + half,
            right: self.px(r.right()) - half,
            bottom: self.px(r.bottom()) - half,
        };
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: (self.px(radius) - half).max(0.0),
            radiusY: (self.px(radius) - half).max(0.0),
        };
        unsafe { self.rt.DrawRoundedRectangle(&rr, self.brush, t, None) };
    }

    /// 用线性渐变在内沿描边（普通/蓝色按钮的立体高光边）。
    pub fn stroke_inner_gradient(
        &self,
        r: Rect,
        radius: f32,
        gradient: &LinearGradient,
        thickness_logical: f32,
    ) -> Result<()> {
        let brush = self.create_gradient_brush(r, gradient)?;
        let t = self.dev(thickness_logical).max(1.0);
        let half = t / 2.0;
        let rect = D2D_RECT_F {
            left: self.px(r.x) + half,
            top: self.px(r.y) + half,
            right: self.px(r.right()) - half,
            bottom: self.px(r.bottom()) - half,
        };
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: (self.px(radius) - half).max(0.0),
            radiusY: (self.px(radius) - half).max(0.0),
        };
        unsafe { self.rt.DrawRoundedRectangle(&rr, &brush, t, None) };
        Ok(())
    }

    /// 构建一个映射到矩形 `r` 的线性渐变画刷。
    /// 处理 XAML 的 Absolute 映射（端点为 DIP，相对矩形左上）与 ScaleY=-1 翻转。
    fn create_gradient_brush(&self, r: Rect, g: &LinearGradient) -> Result<ID2D1LinearGradientBrush> {
        let stops: Vec<D2D1_GRADIENT_STOP> = g
            .stops
            .iter()
            .map(|s| D2D1_GRADIENT_STOP { position: s.offset, color: s.color.d2d() })
            .collect();
        let collection = unsafe {
            self.rt.CreateGradientStopCollection(
                &stops,
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            )?
        };

        // 端点（设备像素）。Absolute：相对矩形左上的 DIP；否则相对包围盒比例。
        let top = self.dev(r.y);
        let bottom = self.dev(r.bottom());
        let (mut p0, mut p1) = if g.absolute {
            (
                D2D_POINT_2F { x: self.dev(r.x) + self.dev(g.start.0), y: top + self.dev(g.start.1) },
                D2D_POINT_2F { x: self.dev(r.x) + self.dev(g.end.0), y: top + self.dev(g.end.1) },
            )
        } else {
            (
                D2D_POINT_2F {
                    x: self.dev(r.x) + self.dev(r.w) * g.start.0,
                    y: top + (bottom - top) * g.start.1,
                },
                D2D_POINT_2F {
                    x: self.dev(r.x) + self.dev(r.w) * g.end.0,
                    y: top + (bottom - top) * g.end.1,
                },
            )
        };
        if g.flip_y {
            // 绕矩形竖直中心反射：y' = top + bottom - y。
            p0.y = top + bottom - p0.y;
            p1.y = top + bottom - p1.y;
        }

        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: p0, endPoint: p1 };
        unsafe { self.rt.CreateLinearGradientBrush(&props, None, &collection) }
    }

    /// 文本测量（逻辑像素）。用于按钮等按内容定尺寸。
    pub fn measure_text(&self, text: &str, style: TextStyle) -> Result<Size> {
        let format = create_text_format(self.dwrite, style, self.scale)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = unsafe {
            self.dwrite.CreateTextLayout(&wide, &format, f32::MAX, f32::MAX)?
        };
        let mut m = DWRITE_TEXT_METRICS::default();
        unsafe { layout.GetMetrics(&mut m)? };
        // metrics 为设备像素（因字号已 × scale），换回逻辑像素。
        Ok(Size { w: m.width / self.scale, h: m.height / self.scale })
    }

    /// 在矩形内绘制单行文本，水平/垂直居中（用于按钮标签等）。
    pub fn draw_text_centered(&self, text: &str, style: TextStyle, r: Rect, color: Color) -> Result<()> {
        self.draw_text(text, style, r, color, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
    }

    /// 左对齐、垂直居中（用于段落标题、列表项文字等）。
    pub fn draw_text_leading(&self, text: &str, style: TextStyle, r: Rect, color: Color) -> Result<()> {
        self.draw_text(text, style, r, color, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
    }

    /// 在矩形内绘制单行文本，指定水平/垂直对齐。
    pub fn draw_text(
        &self,
        text: &str,
        style: TextStyle,
        r: Rect,
        color: Color,
        h_align: DWRITE_TEXT_ALIGNMENT,
        v_align: DWRITE_PARAGRAPH_ALIGNMENT,
    ) -> Result<()> {
        let format = create_text_format(self.dwrite, style, self.scale)?;
        unsafe {
            format.SetTextAlignment(h_align)?;
            format.SetParagraphAlignment(v_align)?;
            // 复位换行（缓存的格式可能被换行变体改成 WRAP）。
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        }
        self.set_brush(color);
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout_rect = self.dev_rect(r);
        unsafe {
            self.rt.DrawText(
                &wide,
                &format,
                &layout_rect,
                self.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        };
        Ok(())
    }

    /// 在矩形内绘制自动换行文本（左对齐、顶部对齐）。用于通知条正文等多行内容。
    pub fn draw_text_wrapped(&self, text: &str, style: TextStyle, r: Rect, color: Color) -> Result<()> {
        let format = create_text_format(self.dwrite, style, self.scale)?;
        unsafe {
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
        }
        self.set_brush(color);
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout_rect = self.dev_rect(r);
        unsafe {
            self.rt.DrawText(
                &wide,
                &format,
                &layout_rect,
                self.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        };
        Ok(())
    }

    /// 自动换行 + **垂直居中**的文本（左对齐）。用于无标题的通知条等需整体居中的多行内容。
    pub fn draw_text_wrapped_centered(&self, text: &str, style: TextStyle, r: Rect, color: Color) -> Result<()> {
        let format = create_text_format(self.dwrite, style, self.scale)?;
        unsafe {
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
        }
        self.set_brush(color);
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout_rect = self.dev_rect(r);
        unsafe {
            self.rt.DrawText(
                &wide,
                &format,
                &layout_rect,
                self.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        };
        Ok(())
    }

    /// 绘制一张**预乘 BGRA**位图（每像素 4 字节，行距 = `w_px*4`），缩放铺入逻辑矩形 `r`。
    /// 按 `key` 缓存已上传的 `ID2D1Bitmap`，避免每帧重建（动画期间的性能关键）。
    /// 缓存随 `Surface` 存活；同一 `key` 在尺寸/内容变化时调用方需换用新 key。
    pub fn draw_image(&mut self, key: u64, r: Rect, bgra_premul: &[u8], w_px: u32, h_px: u32) {
        if w_px == 0 || h_px == 0 || bgra_premul.len() < (w_px as usize) * (h_px as usize) * 4 {
            return;
        }
        let bmp = if let Some(b) = self.images.get(&key) {
            b.clone()
        } else {
            let props = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
            };
            match unsafe {
                self.rt.CreateBitmap(
                    D2D_SIZE_U { width: w_px, height: h_px },
                    Some(bgra_premul.as_ptr() as *const c_void),
                    w_px * 4,
                    &props,
                )
            } {
                Ok(b) => {
                    self.images.insert(key, b.clone());
                    b
                }
                Err(_) => return,
            }
        };
        let dr = self.dev_rect(r);
        unsafe {
            self.rt.DrawBitmap(&bmp, Some(&dr as *const D2D_RECT_F), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None)
        };
    }

    /// 压入统一不透明度图层（对应 XAML 元素 `Opacity` 动画：整组绘制一起淡入/淡出，
    /// 而非逐元素叠加）。需配对 [`Painter::pop_layer`]。
    pub fn push_opacity_layer(&self, opacity: f32) {
        let params = D2D1_LAYER_PARAMETERS {
            contentBounds: D2D_RECT_F { left: f32::MIN, top: f32::MIN, right: f32::MAX, bottom: f32::MAX },
            geometricMask: std::mem::ManuallyDrop::new(None),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity,
            opacityBrush: std::mem::ManuallyDrop::new(None),
            layerOptions: D2D1_LAYER_OPTIONS_NONE,
        };
        unsafe { self.rt.PushLayer(&params, None) };
    }

    pub fn pop_layer(&self) {
        unsafe { self.rt.PopLayer() };
    }

    /// 压入**圆角矩形裁剪层**：层内绘制被 `r`（圆角 `corner`）裁掉外侧
    /// （对应 CSS `overflow:hidden` + `border-radius`）。需配对 [`Painter::pop_layer`]。
    /// 用途：下划线画成**满宽直线**，但底部两角顺着圆角被裁去——线始终直、转角处下缘被切。
    pub fn push_rounded_clip(&self, r: Rect, corner: f32) {
        unsafe {
            let rr = D2D1_ROUNDED_RECT {
                rect: self.dev_rect(r),
                radiusX: self.px(corner),
                radiusY: self.px(corner),
            };
            let mask: Option<ID2D1Geometry> = self.rt.GetFactory().ok().and_then(|factory| {
                factory
                    .CreateRoundedRectangleGeometry(&rr)
                    .ok()
                    .and_then(|g| g.cast::<ID2D1Geometry>().ok())
            });
            let mut params = D2D1_LAYER_PARAMETERS {
                contentBounds: D2D_RECT_F { left: f32::MIN, top: f32::MIN, right: f32::MAX, bottom: f32::MAX },
                geometricMask: std::mem::ManuallyDrop::new(mask),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: Matrix3x2::identity(),
                opacity: 1.0,
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS_NONE,
            };
            self.rt.PushLayer(&params, None);
            // PushLayer 内部已 AddRef 持有几何；释放本地这份引用，避免每帧泄漏。
            std::mem::ManuallyDrop::drop(&mut params.geometricMask);
        }
    }

    /// 绘制一个图标字形（如 ComboBox 的 ChevronDown `\u{E70D}`），居中于 `r`。
    /// 用 `Segoe MDL2 Assets`（Win10/11 均自带；Win11 独有的 `Segoe Fluent Icons`
    /// 在 Win10 缺失会渲染成缺字方块，故统一用 MDL2，码位兼容）。`size` 为逻辑像素字号。
    pub fn draw_icon(&self, glyph: char, size: f32, r: Rect, color: Color) -> Result<()> {
        // 缓存的居中图标格式（避免每次 CreateTextFormat + 两个 HSTRING 分配）。
        let format = crate::typography::create_icon_format(self.dwrite, self.icon_font, size * self.scale)?;
        self.set_brush(color);
        let mut buf = [0u16; 2];
        let wide = glyph.encode_utf16(&mut buf);
        let layout_rect = self.dev_rect(r);
        unsafe {
            self.rt.DrawText(
                wide,
                &format,
                &layout_rect,
                self.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        };
        Ok(())
    }

    /// 直线（逻辑坐标），用于分隔线/简单图形。
    pub fn draw_line(&self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color, width: f32) {
        self.set_brush(color);
        let p0 = D2D_POINT_2F { x: self.dev(x0), y: self.dev(y0) };
        let p1 = D2D_POINT_2F { x: self.dev(x1), y: self.dev(y1) };
        unsafe { self.rt.DrawLine(p0, p1, self.brush, self.dev(width).max(1.0), None) };
    }

    // ———————————————— 矢量图标（零字体依赖，避免缺字方块）————————————————

    fn round_stroke(&self) -> Option<ID2D1StrokeStyle> {
        let props = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: D2D1_CAP_STYLE_ROUND,
            endCap: D2D1_CAP_STYLE_ROUND,
            dashCap: D2D1_CAP_STYLE_ROUND,
            lineJoin: D2D1_LINE_JOIN_ROUND,
            miterLimit: 10.0,
            dashStyle: D2D1_DASH_STYLE_SOLID,
            dashOffset: 0.0,
        };
        unsafe { self.d2d.CreateStrokeStyle(&props, None).ok() }
    }

    fn build_path(&self, pts: &[(f32, f32)], closed: bool, filled: bool) -> Result<ID2D1PathGeometry> {
        let geo = unsafe { self.d2d.CreatePathGeometry()? };
        let sink = unsafe { geo.Open()? };
        let dev: Vec<D2D_POINT_2F> = pts.iter().map(|&(x, y)| D2D_POINT_2F { x: self.dev(x), y: self.dev(y) }).collect();
        unsafe {
            sink.BeginFigure(dev[0], if filled { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW });
            sink.AddLines(&dev[1..]);
            sink.EndFigure(if closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
            sink.Close()?;
        }
        Ok(geo)
    }

    /// 折线描边（逻辑坐标），圆头圆角，用于 chevron / 勾 / 汉堡等。
    pub fn stroke_polyline(&self, pts: &[(f32, f32)], color: Color, width: f32) {
        if pts.len() < 2 {
            return;
        }
        self.set_brush(color);
        if let Ok(geo) = self.build_path(pts, false, false) {
            let ss = self.round_stroke();
            unsafe { self.rt.DrawGeometry(&geo, self.brush, self.dev(width).max(1.0), ss.as_ref()) };
        }
    }

    /// 闭合多边形描边。
    pub fn stroke_polygon(&self, pts: &[(f32, f32)], color: Color, width: f32) {
        if pts.len() < 2 {
            return;
        }
        self.set_brush(color);
        if let Ok(geo) = self.build_path(pts, true, false) {
            let ss = self.round_stroke();
            unsafe { self.rt.DrawGeometry(&geo, self.brush, self.dev(width).max(1.0), ss.as_ref()) };
        }
    }

    /// 闭合多边形填充（用于星形等）。
    pub fn fill_polygon(&self, pts: &[(f32, f32)], color: Color) {
        if pts.len() < 3 {
            return;
        }
        self.set_brush(color);
        if let Ok(geo) = self.build_path(pts, true, true) {
            unsafe { self.rt.FillGeometry(&geo, self.brush, None) };
        }
    }

    pub fn stroke_circle(&self, cx: f32, cy: f32, r: f32, color: Color, width: f32) {
        self.set_brush(color);
        let e = D2D1_ELLIPSE { point: D2D_POINT_2F { x: self.dev(cx), y: self.dev(cy) }, radiusX: self.dev(r), radiusY: self.dev(r) };
        unsafe { self.rt.DrawEllipse(&e, self.brush, self.dev(width).max(1.0), None) };
    }

    pub fn fill_circle(&self, cx: f32, cy: f32, r: f32, color: Color) {
        self.set_brush(color);
        let e = D2D1_ELLIPSE { point: D2D_POINT_2F { x: self.dev(cx), y: self.dev(cy) }, radiusX: self.dev(r), radiusY: self.dev(r) };
        unsafe { self.rt.FillEllipse(&e, self.brush) };
    }

    /// 描一段圆弧（角度制，0°=右、顺时针为正），圆头。用于 ProgressRing 旋转弧。
    pub fn stroke_arc(&self, cx: f32, cy: f32, r: f32, start_deg: f32, sweep_deg: f32, color: Color, width: f32) {
        if sweep_deg.abs() < 0.01 {
            return;
        }
        self.set_brush(color);
        let a0 = start_deg.to_radians();
        let a1 = (start_deg + sweep_deg).to_radians();
        let p0 = D2D_POINT_2F { x: self.dev(cx + r * a0.cos()), y: self.dev(cy + r * a0.sin()) };
        let p1 = D2D_POINT_2F { x: self.dev(cx + r * a1.cos()), y: self.dev(cy + r * a1.sin()) };
        let geo = match unsafe { self.d2d.CreatePathGeometry() } {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Ok(sink) = unsafe { geo.Open() } {
            unsafe {
                sink.BeginFigure(p0, D2D1_FIGURE_BEGIN_HOLLOW);
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: p1,
                    size: D2D_SIZE_F { width: self.dev(r), height: self.dev(r) },
                    rotationAngle: 0.0,
                    sweepDirection: if sweep_deg >= 0.0 { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                    arcSize: if sweep_deg.abs() > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                });
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                let _ = sink.Close();
            }
            let ss = self.round_stroke();
            unsafe { self.rt.DrawGeometry(&geo, self.brush, self.dev(width).max(1.0), ss.as_ref()) };
        }
    }

    /// 绘制一个内置图标，居中于方形区域 `r`，颜色 `color`。
    /// 用**真字体字形**渲染（Segoe Fluent Icons / 回退 Segoe MDL2 Assets，见 `icon_font`），
    /// 而非自绘几何——保证与官方图标一致；字体缺失时才退化为矢量近似。
    pub fn draw_glyph(&self, icon: Icon, r: Rect, color: Color) {
        // 电脑 / Windows 徽标无合适字体字形，走矢量自绘。
        if matches!(icon, Icon::Computer | Icon::WindowsLogo) {
            self.draw_glyph_vector(icon, r, color);
            return;
        }
        let size = r.w.min(r.h);
        let _ = self.draw_icon(icon.codepoint(), size, r, color);
    }

    /// 自绘矢量图标（仅在系统无图标字体时作兜底，正常不走这里）。
    #[allow(dead_code)]
    pub fn draw_glyph_vector(&self, icon: Icon, r: Rect, color: Color) {
        // 单位盒 [0,1]^2 → r 的映射。
        let m = |u: f32, v: f32| (r.x + u * r.w, r.y + v * r.h);
        let sw = (r.w / 16.0 * 1.3).max(1.0); // 16px 基准下约 1.3px 线宽
        match icon {
            Icon::ChevronDown => {
                let (a, b, c) = (m(0.30, 0.42), m(0.50, 0.62), m(0.70, 0.42));
                self.stroke_polyline(&[a, b, c], color, sw);
            }
            Icon::ChevronUp => {
                let (a, b, c) = (m(0.30, 0.58), m(0.50, 0.38), m(0.70, 0.58));
                self.stroke_polyline(&[a, b, c], color, sw);
            }
            Icon::Close => {
                self.stroke_polyline(&[m(0.32, 0.32), m(0.68, 0.68)], color, sw);
                self.stroke_polyline(&[m(0.68, 0.32), m(0.32, 0.68)], color, sw);
            }
            Icon::Hamburger => {
                for v in [0.32, 0.50, 0.68] {
                    let (a, b) = (m(0.20, v), m(0.80, v));
                    self.stroke_polyline(&[a, b], color, sw);
                }
            }
            Icon::Home => {
                let roof = [m(0.18, 0.52), m(0.50, 0.24), m(0.82, 0.52)];
                self.stroke_polyline(&roof, color, sw);
                let body = [m(0.28, 0.48), m(0.28, 0.80), m(0.72, 0.80), m(0.72, 0.48)];
                self.stroke_polyline(&body, color, sw);
            }
            Icon::Folder => {
                let f = [
                    m(0.16, 0.40), m(0.16, 0.74), m(0.84, 0.74), m(0.84, 0.44),
                    m(0.48, 0.44), m(0.40, 0.34), m(0.18, 0.34), m(0.16, 0.40),
                ];
                self.stroke_polygon(&f, color, sw);
            }
            Icon::Star => {
                let cx = r.x + r.w * 0.5;
                let cy = r.y + r.h * 0.52;
                let (ro, ri) = (r.w * 0.40, r.w * 0.17);
                let mut pts = Vec::with_capacity(10);
                for k in 0..10 {
                    let ang = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
                    let rad = if k % 2 == 0 { ro } else { ri };
                    pts.push((cx + rad * ang.cos(), cy + rad * ang.sin()));
                }
                self.fill_polygon(&pts, color);
            }
            Icon::Settings => {
                // 齿轮：8 齿外缘多边形描边 + 中心圆。
                let cx = r.x + r.w * 0.5;
                let cy = r.y + r.h * 0.5;
                let (ro, ri) = (r.w * 0.40, r.w * 0.30);
                let mut pts = Vec::with_capacity(16);
                for k in 0..16 {
                    let ang = k as f32 * std::f32::consts::PI / 8.0;
                    let rad = if k % 2 == 0 { ro } else { ri };
                    pts.push((cx + rad * ang.cos(), cy + rad * ang.sin()));
                }
                self.stroke_polygon(&pts, color, sw);
                self.stroke_circle(cx, cy, r.w * 0.13, color, sw);
            }
            Icon::Info => {
                let cx = r.x + r.w * 0.5;
                let cy = r.y + r.h * 0.5;
                self.stroke_circle(cx, cy, r.w * 0.40, color, sw);
                self.fill_circle(cx, r.y + r.h * 0.32, r.w * 0.045, color);
                let (a, b) = (m(0.50, 0.44), m(0.50, 0.70));
                self.stroke_polyline(&[a, b], color, sw);
            }
            Icon::Success => {
                let cx = r.x + r.w * 0.5;
                let cy = r.y + r.h * 0.5;
                self.stroke_circle(cx, cy, r.w * 0.40, color, sw);
                let chk = [m(0.32, 0.52), m(0.44, 0.64), m(0.68, 0.38)];
                self.stroke_polyline(&chk, color, sw);
            }
            Icon::Warning => {
                let tri = [m(0.50, 0.18), m(0.86, 0.80), m(0.14, 0.80)];
                self.stroke_polygon(&tri, color, sw);
                let (a, b) = (m(0.50, 0.42), m(0.50, 0.60));
                self.stroke_polyline(&[a, b], color, sw);
                self.fill_circle(r.x + r.w * 0.5, r.y + r.h * 0.70, r.w * 0.045, color);
            }
            Icon::Error => {
                let cx = r.x + r.w * 0.5;
                let cy = r.y + r.h * 0.5;
                self.stroke_circle(cx, cy, r.w * 0.40, color, sw);
                let (a, b) = (m(0.38, 0.38), m(0.62, 0.62));
                let (c, d) = (m(0.62, 0.38), m(0.38, 0.62));
                self.stroke_polyline(&[a, b], color, sw);
                self.stroke_polyline(&[c, d], color, sw);
            }
            Icon::Computer => {
                // 显示器：屏幕外框（矩形）+ 底座支架。
                self.stroke_polygon(
                    &[m(0.14, 0.22), m(0.86, 0.22), m(0.86, 0.64), m(0.14, 0.64)],
                    color,
                    sw,
                );
                // 支架（竖杆 + 底座横线）。
                self.stroke_polyline(&[m(0.50, 0.64), m(0.50, 0.78)], color, sw);
                self.stroke_polyline(&[m(0.33, 0.80), m(0.67, 0.80)], color, sw);
            }
            Icon::WindowsLogo => {
                // Windows 四方块（2×2，中间留缝），实心圆角小方块。
                let g = r.w * 0.06; // 半缝
                let cx = r.x + r.w * 0.5;
                let cy = r.y + r.h * 0.5;
                let x0 = r.x + r.w * 0.13;
                let x1 = cx + g;
                let y0 = r.y + r.h * 0.13;
                let y1 = cy + g;
                let sqw = (cx - g) - x0;
                let sqh = (cy - g) - y0;
                let rad = r.w * 0.03;
                for &(qx, qy) in &[(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
                    self.fill_rounded_rect(Rect { x: qx, y: qy, w: sqw, h: sqh }, rad, color);
                }
            }
        }
    }

    /// 压入一个裁剪矩形（用于弹出层/列表内容）。需配对 [`Painter::pop_clip`]。
    pub fn push_clip(&self, r: Rect) {
        unsafe {
            self.rt.PushAxisAlignedClip(&self.dev_rect(r), D2D1_ANTIALIAS_MODE_ALIASED);
        }
    }

    /// **抗锯齿**裁剪：裁边按子像素混合，不会像 ALIASED 那样在整数像素行上硬切。
    /// 专用于**动画揭示**（卡片展开/折叠的体高裁剪）——裁剪线逐帧移动时，
    /// 若用 ALIASED，裁到控件高对比边框上会逐帧在像素间跳动 = 频闪；AA 则平滑无闪。
    pub fn push_clip_aa(&self, r: Rect) {
        unsafe {
            self.rt.PushAxisAlignedClip(&self.dev_rect(r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
    }

    pub fn pop_clip(&self) {
        unsafe { self.rt.PopAxisAlignedClip() };
    }

    /// 设置整体世界变换（设备像素空间），用于动画位移等。传 None 复位。
    pub fn set_transform(&self, m: Option<Matrix3x2>) {
        unsafe { self.rt.SetTransform(&m.unwrap_or(Matrix3x2::identity())) };
    }

    /// 以逻辑坐标点 `center` 为中心，按 `s` 等比缩放整个世界变换（叠加在 DPI 之上）。
    /// 对应 XAML `ScaleTransform`（如 ContentDialog 入场 1.05→1.0）。用 [`Painter::set_transform`]`(None)` 复位。
    pub fn set_scale_about(&self, center: Point, s: f32) {
        let cx = self.dev(center.x);
        let cy = self.dev(center.y);
        let m = Matrix3x2 { M11: s, M12: 0.0, M21: 0.0, M22: s, M31: cx * (1.0 - s), M32: cy * (1.0 - s) };
        unsafe { self.rt.SetTransform(&m) };
    }

    /// 以逻辑坐标点 `center` 为中心，按 `degrees` 顺时针旋转世界变换（叠加在 DPI 之上）。
    /// 对应 XAML `RotateTransform`（如 Expander chevron 折叠↔展开 0°↔180°）。用
    /// [`Painter::set_transform`]`(None)` 复位。
    pub fn set_rotation_about(&self, center: Point, degrees: f32) {
        let rad = degrees.to_radians();
        let (s, c) = (rad.sin(), rad.cos());
        let cx = self.dev(center.x);
        let cy = self.dev(center.y);
        let m = Matrix3x2 {
            M11: c,
            M12: s,
            M21: -s,
            M22: c,
            M31: cx - cx * c + cy * s,
            M32: cy - cx * s - cy * c,
        };
        unsafe { self.rt.SetTransform(&m) };
    }

    /// 命中点是否落在某逻辑矩形内（便捷封装）。
    pub fn hit(&self, r: Rect, p: Point) -> bool {
        r.contains(p)
    }

    /// 结束一帧并 present。返回是否需要重建设备资源（设备丢失 / `D2DERR_RECREATE_TARGET`）。
    ///
    /// * 组合后端：`EndDraw` + `IDXGISwapChain1::Present(1, 0)` —— 同步到 vblank，无撕裂、无闪；
    ///   宿主在此路径下**不**应再 `DwmFlush`（Present 已自带节奏）。用 [`Surface::paces_to_vblank`] 判定。
    /// * HWND / 离屏后端：`EndDraw`（HWND 为隐式 BLT present），节奏由宿主 `DwmFlush` 负责（不变）。
    pub fn end(self) -> Result<bool> {
        let hr = unsafe { self.rt.EndDraw(None, None) };
        match hr {
            Ok(()) => {
                if let Some(swapchain) = self.present {
                    // 1 = 等待一个垂直消隐区间再翻页；vblank 同步，无撕裂。
                    let phr = unsafe { swapchain.Present(1, DXGI_PRESENT(0)) };
                    if phr == DXGI_ERROR_DEVICE_REMOVED || phr == DXGI_ERROR_DEVICE_RESET {
                        return Ok(true); // 设备丢失 → 让宿主丢弃并重建 Surface。
                    }
                }
                Ok(false)
            }
            Err(e) if e.code() == D2DERR_RECREATE_TARGET => Ok(true),
            Err(e) => Err(e),
        }
    }
}

/// Win32 `RECT` → 设备像素宽高。
pub fn rect_size(rc: &RECT) -> (u32, u32) {
    ((rc.right - rc.left).max(0) as u32, (rc.bottom - rc.top).max(0) as u32)
}

// 让上面用到的 Interface trait 被引用（D2DERR_RECREATE_TARGET 经由 .code() 比较）。
const _: fn() = || {
    fn _assert_interface<T: Interface>() {}
    _assert_interface::<ID2D1Factory>();
};
