//! cp-help 的 Win32 + Direct2D 宿主：固定尺寸、不可缩放、带应用图标的窗口，
//! 驱动一个 fluentpx [`Widget`] 根控件。负责 DWM 深色标题栏 / Mica / 圆角、
//! Per-Monitor v2 高 DPI、D2D 绘制循环、输入事件分发、动画计时与系统主题跟随。
//!
//! 取代原先的 eframe/egui 窗口与 `dwm.rs`：现在我们自己持有 HWND 与消息循环，
//! 因此可直接处理深浅主题切换，无需额外的消息专用窗口线程。

use std::ffi::c_void;
use std::mem::size_of;
use std::time::Instant;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::Ime::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use fluentpx::gfx::{Gfx, Surface};
use fluentpx::widget::{Cursor, InputEvent, PaintCtx, Point, Size, Widget};
use fluentpx::{Dpi, Theme};

const WM_MOUSELEAVE_LOCAL: u32 = 0x02A3;

/// WM_IME_SETCONTEXT 的 lParam 显示位 `ISC_SHOWUICOMPOSITIONWINDOW`（组字窗由系统 UI 窗口绘制）。
/// 值见 imm.h / MS Learn。我们在 TextBox 内**自绘**内联组字串，故把此位从 lParam 清掉，
/// 系统就不再弹默认组字浮窗（即「搜索框上方一条白底显示拼音」的根因）。
const ISC_SHOWUICOMPOSITIONWINDOW: isize = 0x8000_0000;

pub struct WindowOptions {
    /// 窗口标题。
    pub title: String,
    /// 客户区逻辑宽（设备无关像素）。
    pub width: f32,
    /// 客户区逻辑高。
    pub height: f32,
    /// 是否可缩放（含最大化）。false=固定尺寸（仅标题栏+最小化）。
    pub resizable: bool,
    /// 最小客户区逻辑尺寸（仅 resizable 时生效）。
    pub min_width: f32,
    pub min_height: f32,
}

impl WindowOptions {
    /// 固定尺寸窗口（不可缩放、不可最大化）。
    pub fn fixed(title: impl Into<String>, width: f32, height: f32) -> Self {
        WindowOptions { title: title.into(), width, height, resizable: false, min_width: width, min_height: height }
    }
    /// 可缩放窗口，带最小尺寸。
    pub fn resizable(title: impl Into<String>, width: f32, height: f32, min_width: f32, min_height: f32) -> Self {
        WindowOptions { title: title.into(), width, height, resizable: true, min_width, min_height }
    }
}

struct Host {
    gfx: Gfx,
    surface: Option<Surface>,
    /// 主题解析器：每帧调用以确定当前主题（支持「跟随系统/强制浅色/强制深色」）。
    theme_fn: Box<dyn Fn() -> Theme>,
    applied_dark: Option<bool>,
    dpi: Dpi,
    size_px: (u32, u32),
    /// 最小客户区逻辑尺寸（供 WM_GETMINMAXINFO 换算）。
    min_logical: (f32, f32),
    start: Instant,
    hwnd: HWND,
    tracking_mouse: bool,
    root: Box<dyn Widget>,
}

impl Host {
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
    fn scale(&self) -> f32 {
        self.dpi.scale()
    }
    fn viewport(&self) -> Size {
        Size { w: self.size_px.0 as f32 / self.scale(), h: self.size_px.1 as f32 / self.scale() }
    }

    fn ensure_surface(&mut self) -> bool {
        if self.surface.is_none() {
            match self.gfx.create_surface(self.hwnd, self.size_px.0, self.size_px.1) {
                Ok(s) => self.surface = Some(s),
                Err(_) => return false,
            }
        }
        true
    }

    /// 当前表面是否走组合后端（Present 自带 vblank 节奏 → 宿主**不**应再 DwmFlush）。
    /// 无表面时按回退路径处理（需要 DwmFlush）。
    fn paces_to_vblank(&self) -> bool {
        self.surface.as_ref().map_or(false, |s| s.paces_to_vblank())
    }

    /// 应用 DWM 视觉属性（深色标题栏 + Mica 背景 + 圆角）。仅当深浅态变化时重设。
    fn apply_dwm(&mut self, theme: Theme) {
        let dark = theme.is_dark();
        if self.applied_dark == Some(dark) {
            return;
        }
        self.applied_dark = Some(dark);
        unsafe {
            let b: BOOL = dark.into();
            let _ = DwmSetWindowAttribute(self.hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &b as *const _ as *const c_void, size_of::<BOOL>() as u32);
            let backdrop = DWMSBT_MAINWINDOW;
            let _ = DwmSetWindowAttribute(self.hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop as *const _ as *const c_void, size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32);
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(self.hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as *const c_void, size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32);
        }
    }

    fn paint(&mut self) {
        if self.size_px.0 == 0 || self.size_px.1 == 0 || !self.ensure_surface() {
            return;
        }
        let theme = (self.theme_fn)();
        self.apply_dwm(theme);
        let tokens = theme.tokens();
        let scale = self.scale();
        let now = self.now();
        let viewport = self.viewport();
        let dwrite = self.gfx.dwrite.clone();
        let icon_font = self.gfx.icon_font.clone();
        let dpi = self.dpi;

        // 根控件占满整个客户区。
        self.root.arrange(fluentpx::Rect::new(0.0, 0.0, viewport.w, viewport.h));

        let surface = self.surface.as_mut().unwrap();
        let root = &mut self.root;
        let recreate = (|| -> Result<bool> {
            let mut painter = surface.begin(&dwrite, &icon_font, scale)?;
            painter.clear(tokens.solid_bg_base);
            // 每帧从单位变换开始：D2D 的世界变换在 BeginDraw 间是持久的，
            // 万一上一帧（如对话框缩放/旋转）漏掉复位，这里兜底，杜绝“变换泄漏→满屏放大”。
            painter.set_transform(None);
            let mut ctx = PaintCtx { painter: &mut painter, tokens: &tokens, dpi, now, viewport };
            root.paint(&mut ctx);
            root.paint_overlay(&mut ctx);
            painter.end()
        })();
        if matches!(recreate, Ok(true) | Err(_)) {
            self.surface = None;
            unsafe { let _ = InvalidateRect(self.hwnd, None, false); }
        }
    }

    fn dispatch(&mut self, ev: InputEvent) {
        let now = self.now();
        let r = self.root.on_event(ev, now);
        // 只要事件请求重绘就 InvalidateRect —— 哪怕此刻已在动画中。这样「开始一个动画」的
        // 事件（如下拉框关闭：close_now 把 is_animating 立刻置真）一定能把消息泵踢醒，
        // 让主循环从 close_start 起逐帧播放关闭动画，而不是首帧被吞掉=看着像瞬间关闭。
        // 动画期间 WM_PAINT 有 !is_animating 门、且每帧 ValidateRect 清除失效区，故此处多发无害。
        if r.redraw {
            unsafe { let _ = InvalidateRect(self.hwnd, None, false); }
        }
    }
}

fn lparam_point(lp: LPARAM, scale: f32) -> Point {
    let x = (lp.0 & 0xffff) as i16 as f32;
    let y = ((lp.0 >> 16) & 0xffff) as i16 as f32;
    Point { x: x / scale, y: y / scale }
}

/// 读取 IME 组字串 / 结果串（UTF-16 → String）。
unsafe fn ime_string(himc: HIMC, gcs: IME_COMPOSITION_STRING) -> Option<String> {
    let len = ImmGetCompositionStringW(himc, gcs, None, 0);
    if len <= 0 {
        return None;
    }
    let count = len as usize / 2;
    let mut buf = vec![0u16; count];
    let got = ImmGetCompositionStringW(himc, gcs, Some(buf.as_mut_ptr() as *mut c_void), len as u32);
    if got <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..(got as usize / 2).min(count)]))
}

/// 把 IME 候选窗定位到聚焦搜索框的光标处（设备坐标）。无聚焦文本框则不动。
unsafe fn position_ime(hwnd: HWND, host: &Host) {
    let caret = match host.root.caret_pos() {
        Some(c) => c,
        None => return,
    };
    let scale = host.scale();
    let pt = POINT { x: (caret.x * scale).round() as i32, y: (caret.y * scale).round() as i32 };
    let himc = ImmGetContext(hwnd);
    let cf = COMPOSITIONFORM {
        dwStyle: CFS_POINT,
        ptCurrentPos: pt,
        rcArea: RECT::default(),
    };
    let _ = ImmSetCompositionWindow(himc, &cf);
    // 候选窗放到搜索框**下方**（组字在框内文字行，候选列表在框下，互不遮挡）。
    let canf = CANDIDATEFORM {
        dwIndex: 0,
        dwStyle: CFS_CANDIDATEPOS,
        ptCurrentPos: POINT { x: pt.x, y: pt.y + (24.0 * scale) as i32 },
        rcArea: RECT::default(),
    };
    let _ = ImmSetCandidateWindow(himc, &canf);
    let _ = ImmReleaseContext(hwnd, himc);
}

fn is_touch_or_pen() -> bool {
    const SIGNATURE: usize = 0xFF51_5700;
    const MASK: usize = 0xFFFF_FF00;
    let extra = unsafe { GetMessageExtraInfo() };
    (extra.0 as usize & MASK) == SIGNATURE
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = lparam.0 as *const CREATESTRUCTW;
        let host = (*cs).lpCreateParams as *mut Host;
        (*host).hwnd = hwnd;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, host as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Host;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let host = &mut *ptr;

    match msg {
        WM_CREATE => {
            host.dpi = Dpi::new(GetDpiForWindow(hwnd));
            let theme = (host.theme_fn)();
            host.apply_dwm(theme);
            // 去掉标题栏图标：WS_EX_DLGMODALFRAME + 清空窗口图标 + 刷新非客户区。
            // 必须在 WM_CREATE 里做（后期设置无效）。
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex | WS_EX_DLGMODALFRAME.0) as isize);
            let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(0), LPARAM(0)); // ICON_SMALL
            let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(1), LPARAM(0)); // ICON_BIG
            let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
            LRESULT(0)
        }
        WM_SIZE => {
            let w = (lparam.0 & 0xffff) as u32;
            let h = ((lparam.0 >> 16) & 0xffff) as u32;
            host.size_px = (w, h);
            if let Some(s) = &mut host.surface {
                // 组合后端：ResizeBuffers + 重绑后备缓冲；HWND 后端：rt.Resize。皆需 &mut。
                let _ = s.resize(w, h);
            }
            // 拖拽缩放时 Windows 进入模态循环、不及时派发 WM_PAINT（=拽大露黑边、拽小被裁、松手才刷）。
            // 这里**同步重绘一帧**让内容随拖拽实时跟随；w/h=0（最小化）时 paint 自身会跳过。
            host.paint();
            LRESULT(0)
        }
        WM_DPICHANGED => {
            host.dpi = Dpi::new((wparam.0 & 0xffff) as u32);
            let rc = *(lparam.0 as *const RECT);
            let _ = SetWindowPos(hwnd, None, rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top, SWP_NOZORDER | SWP_NOACTIVATE);
            host.surface = None;
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if !host.tracking_mouse {
                let mut tme = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut tme);
                host.tracking_mouse = true;
            }
            let p = lparam_point(lparam, host.scale());
            host.dispatch(InputEvent::PointerMove(p));
            LRESULT(0)
        }
        WM_MOUSELEAVE_LOCAL => {
            host.tracking_mouse = false;
            host.dispatch(InputEvent::PointerLeave);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let _ = SetCapture(hwnd);
            let p = lparam_point(lparam, host.scale());
            host.dispatch(InputEvent::PointerDown(p));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            let p = lparam_point(lparam, host.scale());
            host.dispatch(InputEvent::PointerUp(p));
            if is_touch_or_pen() {
                host.dispatch(InputEvent::PointerLeave);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            // 右键：上下文菜单（TextBox 剪切/复制/粘贴/全选）。
            let p = lparam_point(lparam, host.scale());
            host.dispatch(InputEvent::ContextMenu(p));
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            // 滚轮：HIWORD(wparam) 为有符号滚动量，每 120 为一格；正=向上。
            let delta = ((wparam.0 >> 16) & 0xffff) as i16 as f32 / 120.0;
            host.dispatch(InputEvent::Wheel(delta));
            LRESULT(0)
        }
        WM_KEYDOWN => {
            host.dispatch(InputEvent::KeyDown(wparam.0 as u32));
            LRESULT(0)
        }
        WM_CHAR => {
            if let Some(c) = char::from_u32(wparam.0 as u32) {
                host.dispatch(InputEvent::Char(c));
            }
            LRESULT(0)
        }
        WM_IME_SETCONTEXT => {
            // 关键修复：我们在 TextBox 内**自绘**内联组字串，必须清除 lParam 的
            // ISC_SHOWUICOMPOSITIONWINDOW，系统才不再弹默认组字浮窗（=原先「框上方白条」根因）。
            // WM_IME_STARTCOMPOSITION 无返回值，靠 return 0 抑制组字窗是无效的——必须在此清位。
            // 候选窗保留（仅清组字位）：交给系统候选列表，由 position_ime 定位到框下方。
            // 注意：本消息在窗口**激活**时发送（此刻搜索框尚未聚焦），故无条件清除、不按 caret_pos 判断。
            let lp = LPARAM(lparam.0 & !ISC_SHOWUICOMPOSITIONWINDOW);
            DefWindowProcW(hwnd, msg, wparam, lp)
        }
        WM_IME_STARTCOMPOSITION => {
            // 有聚焦文本框 → 由 TextBox **内联自绘**组字串（组字窗已在 WM_IME_SETCONTEXT 抑制）。
            // 这里顺手把候选窗定位到光标下方，使首个组字串一出现候选列表就在正确位置。
            if host.root.caret_pos().is_some() {
                position_ime(hwnd, host);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_IME_COMPOSITION => {
            if host.root.caret_pos().is_some() {
                let himc = ImmGetContext(hwnd);
                let flags = lparam.0 as u32;
                // 上屏结果串：逐字符当 Char 派发（插入框）+ 清空组字。
                if flags & GCS_RESULTSTR.0 != 0 {
                    if let Some(s) = ime_string(himc, GCS_RESULTSTR) {
                        host.root.set_composition("");
                        for c in s.chars() {
                            if !c.is_control() {
                                host.dispatch(InputEvent::Char(c));
                            }
                        }
                    }
                }
                // 未上屏组字串：交给聚焦文本框内联自绘。
                if flags & GCS_COMPSTR.0 != 0 {
                    let comp = ime_string(himc, GCS_COMPSTR).unwrap_or_default();
                    host.root.set_composition(&comp);
                    let _ = InvalidateRect(hwnd, None, false);
                }
                let _ = ImmReleaseContext(hwnd, himc);
                position_ime(hwnd, host); // 候选列表定位到框下方
                return LRESULT(0); // 已处理，抑制系统组字窗
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_IME_ENDCOMPOSITION => {
            host.root.set_composition("");
            let _ = InvalidateRect(hwnd, None, false);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SETTINGCHANGE => {
            // 系统设置变化（含深浅色）：直接重绘。theme_fn 下一帧会重新解析主题，
            // apply_dwm 比对 applied_dark，若变化则重设标题栏。
            let _ = InvalidateRect(hwnd, None, false);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_GETMINMAXINFO => {
            // 强制最小客户区尺寸（仅 resizable 窗口有意义）。
            let scale = host.scale();
            let mmi = lparam.0 as *mut MINMAXINFO;
            if !mmi.is_null() {
                let cw = (host.min_logical.0 * scale).round() as i32;
                let ch = (host.min_logical.1 * scale).round() as i32;
                let mut rc = RECT { left: 0, top: 0, right: cw, bottom: ch };
                let style = WINDOW_STYLE(GetWindowLongPtrW(hwnd, GWL_STYLE) as u32);
                let ex = WINDOW_EX_STYLE(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32);
                let _ = AdjustWindowRectExForDpi(&mut rc, style, false, ex, host.dpi.dpi);
                (*mmi).ptMinTrackSize.x = rc.right - rc.left;
                (*mmi).ptMinTrackSize.y = rc.bottom - rc.top;
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            // 动画期间主循环已按 vblank 节奏每帧自绘；此时 WM_PAINT 再画一次会在同一 vblank
            // 内用**不同的 now()** 二次 BLT（卡片位置 t1→t2 跳一帧）+ 二次 DwmFlush（多等一个刷新）
            // —— 正是「连点一闪一闪」的来源。动画中只 Begin/EndPaint 满足 GDI，绘制交给主循环。
            if !host.root.is_animating(host.now()) {
                host.paint();
                // 事件驱动的重绘（滚动 / hover / 缩放）：HWND 回退（BLT）路径用 DwmFlush 对齐刷新、
                // 消除撕裂；组合后端 Present(1,0) 已同步 vblank，不再 DwmFlush。
                if !host.paces_to_vblank() {
                    let _ = DwmFlush();
                }
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_SETCURSOR => {
            // 指针在搜索框（TextBox）内时显示 I 形文本光标；其余交给 DefWindowProc（箭头/缩放边）。
            // 必须在 WM_SETCURSOR 每次重设，否则鼠标一动就被默认箭头覆盖（MS Learn: WM_SETCURSOR）。
            if (lparam.0 & 0xffff) as i32 == HTCLIENT as i32 {
                let mut pt = POINT::default();
                if GetCursorPos(&mut pt).is_ok() {
                    let _ = ScreenToClient(hwnd, &mut pt);
                    let scale = host.scale();
                    let lp = Point { x: pt.x as f32 / scale, y: pt.y as f32 / scale };
                    if host.root.cursor_at(lp) == Cursor::Text {
                        if let Ok(cur) = LoadCursorW(None, IDC_IBEAM) {
                            SetCursor(cur);
                        }
                        return LRESULT(1); // TRUE：已处理，阻止默认箭头覆盖
                    }
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => {
            let _ = Box::from_raw(ptr);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 启动一个 Fluent 外观的原生窗口，驱动给定的根控件直到关闭。
/// `theme_fn` 每帧调用以解析当前主题（跟随系统 / 强制浅色 / 强制深色）。
pub fn run(opts: WindowOptions, root: Box<dyn Widget>, theme_fn: Box<dyn Fn() -> Theme>) -> Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let hinstance: HINSTANCE = instance.into();
        let class_name = w!("CloudMgrFluentWindow");
        // 不设窗口图标：标题栏左上不显示软件图标。
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let gfx = Gfx::new()?;
        let host = Box::new(Host {
            gfx,
            surface: None,
            theme_fn,
            applied_dark: None,
            dpi: Dpi::DEFAULT,
            size_px: (0, 0),
            min_logical: (opts.min_width, opts.min_height),
            start: Instant::now(),
            hwnd: HWND(std::ptr::null_mut()),
            tracking_mouse: false,
            root,
        });
        let host_ptr = Box::into_raw(host);

        // 可缩放：完整窗口样式（缩放边框 + 最大化）。固定：仅标题栏 + 系统菜单 + 最小化。
        let style = if opts.resizable {
            WS_OVERLAPPEDWINDOW
        } else {
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX
        };
        let ex_style = WINDOW_EX_STYLE::default();

        // 以系统 DPI 估算初始客户区像素尺寸，再换算为整窗尺寸并居中。
        let dpi0 = GetDpiForSystem().max(96);
        let scale0 = dpi0 as f32 / 96.0;
        let cw = (opts.width * scale0).round() as i32;
        let ch = (opts.height * scale0).round() as i32;
        let mut rc = RECT { left: 0, top: 0, right: cw, bottom: ch };
        let _ = AdjustWindowRectExForDpi(&mut rc, style, false, ex_style, dpi0);
        let win_w = rc.right - rc.left;
        let win_h = rc.bottom - rc.top;

        // 居中到主显示器工作区。
        let mut work = RECT::default();
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut _ as *mut c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        let x = work.left + ((work.right - work.left) - win_w).max(0) / 2;
        let y = work.top + ((work.bottom - work.top) - win_h).max(0) / 2;

        let title: Vec<u16> = opts.title.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = CreateWindowExW(
            ex_style,
            class_name,
            PCWSTR(title.as_ptr()),
            style,
            x,
            y,
            win_w,
            win_h,
            None,
            None,
            hinstance,
            Some(host_ptr as *const c_void),
        )?;

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        // 与 DwmFlush 对齐刷新的渲染循环：
        // - 动画期间，每帧直接绘制后 DwmFlush（阻塞到下一次 vblank 之后）→ 帧间隔均匀、丝滑；
        // - 空闲时 WaitMessage 阻塞，纯事件驱动、零 CPU 占用（重绘走 InvalidateRect→WM_PAINT）。
        let mut msg = MSG::default();
        // 上一轮是否在动画中：用于「收尾再画一帧」。is_animating 在**画之前**判定，且分段动画用
        // 严格 (now-s)<dur，故 is_animating 转 false 的那一帧（amt 已到 0/1 的定格帧）本来永远不会被画，
        // 导致折叠「露出一点 / 只有移开鼠标才收完」。这里在动画刚结束时强制再画一帧定格状态。
        let mut was_animating = false;
        loop {
            // 处理所有待处理消息（输入 / 尺寸 / 空闲态的 WM_PAINT 等）。
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return Ok(());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // 窗口销毁后 GWLP_USERDATA 被清零，据此安全退出（避免悬垂指针）。
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Host;
            if ptr.is_null() {
                break;
            }
            let host = &mut *ptr;
            if host.root.is_animating(host.now()) {
                host.paint();
                let _ = ValidateRect(hwnd, None);
                was_animating = true;
                // 组合后端 Present(1,0) 已同步 vblank（无撕裂、节奏均匀），不再 DwmFlush；
                // 仅 HWND 回退（BLT）路径才用 DwmFlush 对齐刷新。
                if !host.paces_to_vblank() {
                    let _ = DwmFlush();
                }
            } else if was_animating {
                // 动画刚结束：再画一帧「定格」状态（amt 已 snap 到 0/1），否则会停在上一帧的
                // 中间值上，残留一条细带，直到下一次事件才清。画完即转入空闲等待。
                host.paint();
                let _ = ValidateRect(hwnd, None);
                was_animating = false;
            } else {
                let _ = WaitMessage();
            }
        }
    }
    Ok(())
}
