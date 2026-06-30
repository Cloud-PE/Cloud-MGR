//! `fluentpx` —— 用 Direct2D 像素级复刻 WinUI 3 Fluent 控件的纯 Rust 库。
//!
//! 全部数值（颜色 token、圆角、内边距、渐变、状态切换）取自官方
//! `microsoft-ui-xaml` 源码真值，详见各模块注释。该 crate 仅 Windows 可用
//! （依赖 D2D/DWrite/Win32），由 `gallery` exe 演示，CI 在 windows-latest 上编译。
//!
//! 模块速览：
//! * [`color`] ARGB 解析 / D2D 颜色 / 半透明叠加 / 线性渐变
//! * [`tokens`] 深浅两套 Fluent token 真值 + 强调色调色板
//! * [`typography`] 字号阶梯 + DirectWrite TextFormat
//! * [`theme`] 深/浅主题与系统跟随
//! * [`dpi`] 高 DPI 缩放与设备像素取整
//! * [`anim`] 缓动曲线与颜色/数值过渡
//! * [`gfx`] D2D/DWrite 引擎、画刷、文字、渐变边框
//! * [`widget`] 控件 trait / 几何 / 状态 / 事件
//! * [`controls`] 九个 Fluent 控件实现

#![cfg(windows)]

pub mod anim;
pub mod clipboard;
pub mod color;
pub mod controls;
pub mod dpi;
pub mod gfx;
pub mod host;
pub mod theme;
pub mod tokens;
pub mod typography;
pub mod widget;

pub use color::Color;
pub use dpi::Dpi;
pub use gfx::{Gfx, Painter, Surface};
pub use theme::Theme;
pub use tokens::Tokens;
pub use widget::{
    AccessibleRole, EventResult, InputEvent, Interaction, PaintCtx, Point, Rect, Size, VisualState,
    Widget,
};
