//! 可复用应用宿主：把 Win32 窗口 + DWM + 高DPI + D2D 绘制循环 + 事件分发 + 动画计时
//! 封装成「驱动一个根 [`Widget`]」。应用只需实现一个根控件（自行布局/绘制/处理事件），
//! 调用 [`run`] 即可得到一个 Fluent 外观的原生窗口程序。
//!
//! gallery 仍用自己的内联宿主；本模块供 cloud-mgr 等独立应用复用。

use std::cell::Cell;
use std::ffi::c_void;
use std::mem::size_of;
use std::rc::Rc;
use std::time::Instant;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::gfx::{Gfx, Surface};
use crate::theme::Theme;
use crate::widget::{InputEvent, PaintCtx, Point, Size, Widget};
use crate::Dpi;

const WM_MOUSELEAVE_LOCAL: u32 = 0x02A3;
const ANIM_TIMER: usize = 1;

/// 主题偏好（Auto = 跟随系统）。应用通过 [`ThemeControl`] 设置。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeMode {
    Auto,
    Light,
    Dark,
}

/// 主题控制句柄（可克隆，交给应用根控件以便在设置中切换主题）。
#[derive(Clone)]
pub struct ThemeControl(Rc<Cell<ThemeMode>>);

impl ThemeControl {
    pub fn get(&self) -> ThemeMode {
        self.0.get()
    }
    pub fn set(&self, m: ThemeMode) {
        self.0.set(m);
    }
    fn resolve(&self) -> Theme {
        match self.0.get() {
            ThemeMode::Auto => Theme::system(),
            ThemeMode::Light => Theme::Light,
            ThemeMode::Dark => Theme::Dark,
        }
    }
}

pub struct WindowOptions {
    pub title: String,
    pub width: i32,
    pub height: i32,
    pub min_width: i32,
    pub min_height: i32,
}

impl Default for WindowOptions {
    fn default() -> Self {
        WindowOptions { title: "fluentpx".into(), width: 1024, height: 640, min_width: 640, min_height: 480 }
    }
}

struct Host {
    gfx: Gfx,
    surface: Option<Surface>,
    theme_ctl: ThemeControl,
    applied_dark: Option<bool>,
    dpi: Dpi,
    size_px: (u32, u32),
    start: Instant,
    hwnd: HWND,
    tracking_mouse: bool,
    timer_on: bool,
    root: Box<dyn Widget>,
    min_size: (i32, i32),
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
        let theme = self.theme_ctl.resolve();
        self.apply_dwm(theme);
        let tokens = theme.tokens();
        let scale = self.scale();
        let now = self.now();
        let viewport = self.viewport();
        let dwrite = self.gfx.dwrite.clone();
        let icon_font = self.gfx.icon_font.clone();
        let dpi = self.dpi;

        // 根控件占满整个客户区。
        self.root.arrange(crate::Rect::new(0.0, 0.0, viewport.w, viewport.h));

        let surface = self.surface.as_mut().unwrap();
        let root = &mut self.root;
        let recreate = (|| -> Result<bool> {
            let mut painter = surface.begin(&dwrite, &icon_font, scale)?;
            painter.clear(tokens.solid_bg_base);
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
        if r.redraw {
            unsafe { let _ = InvalidateRect(self.hwnd, None, false); }
        }
        self.update_timer();
    }

    fn update_timer(&mut self) {
        let want = self.root.is_animating(self.now());
        if want && !self.timer_on {
            unsafe { SetTimer(self.hwnd, ANIM_TIMER, 16, None) };
            self.timer_on = true;
        } else if !want && self.timer_on {
            unsafe { let _ = KillTimer(self.hwnd, ANIM_TIMER); }
            self.timer_on = false;
        }
    }
}

fn lparam_point(lp: LPARAM, scale: f32) -> Point {
    let x = (lp.0 & 0xffff) as i16 as f32;
    let y = ((lp.0 >> 16) & 0xffff) as i16 as f32;
    Point { x: x / scale, y: y / scale }
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
            let theme = host.theme_ctl.resolve();
            host.apply_dwm(theme);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let mmi = lparam.0 as *mut MINMAXINFO;
            let s = host.scale();
            (*mmi).ptMinTrackSize.x = (host.min_size.0 as f32 * s) as i32;
            (*mmi).ptMinTrackSize.y = (host.min_size.1 as f32 * s) as i32;
            LRESULT(0)
        }
        WM_SIZE => {
            let w = (lparam.0 & 0xffff) as u32;
            let h = ((lparam.0 >> 16) & 0xffff) as u32;
            host.size_px = (w, h);
            if let Some(s) = &mut host.surface {
                let _ = s.resize(w, h);
            }
            let _ = InvalidateRect(hwnd, None, false);
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
            let p = lparam_point(lparam, host.scale());
            host.dispatch(InputEvent::ContextMenu(p));
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16 as f32 / 120.0;
            host.dispatch(InputEvent::Wheel(delta));
            LRESULT(0)
        }
        WM_CHAR => {
            if let Some(c) = char::from_u32(wparam.0 as u32) {
                host.dispatch(InputEvent::Char(c));
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            host.dispatch(InputEvent::KeyDown(wparam.0 as u32));
            LRESULT(0)
        }
        WM_TIMER => {
            let _ = InvalidateRect(hwnd, None, false);
            host.update_timer();
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            // Auto 模式下跟随系统主题变化。
            if host.theme_ctl.get() == ThemeMode::Auto {
                let _ = InvalidateRect(hwnd, None, false);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            host.paint();
            let _ = EndPaint(hwnd, &ps);
            host.update_timer();
            LRESULT(0)
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

/// 启动一个由根 Widget 驱动的 Fluent 窗口程序。`build` 收到主题控制句柄并返回根控件。
pub fn run(opts: WindowOptions, build: impl FnOnce(ThemeControl) -> Box<dyn Widget>) -> Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let hinstance: HINSTANCE = instance.into();
        let class_name = w!("FluentPxHostWindow");
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

        let theme_ctl = ThemeControl(Rc::new(Cell::new(ThemeMode::Auto)));
        let gfx = Gfx::new()?;
        let root = build(theme_ctl.clone());
        let host = Box::new(Host {
            gfx,
            surface: None,
            theme_ctl,
            applied_dark: None,
            dpi: Dpi::DEFAULT,
            size_px: (0, 0),
            start: Instant::now(),
            hwnd: HWND(std::ptr::null_mut()),
            tracking_mouse: false,
            timer_on: false,
            root,
            min_size: (opts.min_width, opts.min_height),
        });
        let host_ptr = Box::into_raw(host);

        let title: Vec<u16> = opts.title.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            opts.width,
            opts.height,
            None,
            None,
            hinstance,
            Some(host_ptr as *const c_void),
        )?;

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
