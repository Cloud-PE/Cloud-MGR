//! 动画工具：线性/三次贝塞尔缓动、颜色与数值过渡。
//!
//! WinUI 的 `BrushTransition`（如按钮背景 0.083s）是线性插值；
//! 控件位移（ToggleSwitch knob、Slider thumb）用缓动曲线，曲线参数从源码取真值。

use crate::color::Color;

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// 在直通 sRGB 空间线性插值颜色（与 D2D 默认 BrushTransition 行为一致）。
pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    Color { a: m(a.a, b.a), r: m(a.r, b.r), g: m(a.g, b.g), b: m(a.b, b.b) }
}

/// 三次贝塞尔缓动 y(x)，控制点 (p1x,p1y)、(p2x,p2y)，端点固定 (0,0)、(1,1)。
/// 用牛顿迭代由进度 x 反解参数 t 再求 y，与 CSS/Composition cubic-bezier 一致。
pub fn cubic_bezier(p1x: f32, p1y: f32, p2x: f32, p2y: f32, x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let cx = 3.0 * p1x;
    let bx = 3.0 * (p2x - p1x) - cx;
    let ax = 1.0 - cx - bx;
    let cy = 3.0 * p1y;
    let by = 3.0 * (p2y - p1y) - cy;
    let ay = 1.0 - cy - by;
    let sample_x = |t: f32| ((ax * t + bx) * t + cx) * t;
    let sample_dx = |t: f32| (3.0 * ax * t + 2.0 * bx) * t + cx;

    let mut t = x;
    for _ in 0..8 {
        let x2 = sample_x(t) - x;
        if x2.abs() < 1e-4 {
            break;
        }
        let d = sample_dx(t);
        if d.abs() < 1e-6 {
            break;
        }
        t -= x2 / d;
    }
    ((ay * t + by) * t + cy) * t
}

/// WinUI 标准缓动 `ControlFastOutSlowInKeySpline = "0,0,0,1"`（cubic-bezier(0,0,0,1)）。
/// 控件动画（ToggleSwitch/Slider/ListView/ComboBox/Menu/NavigationView 等）统一用它。
pub fn ease_out(x: f32) -> f32 {
    cubic_bezier(0.0, 0.0, 0.0, 1.0, x)
}

/// 颜色过渡：从 `from` 到 `to`，时长 `dur` 秒，线性。
#[derive(Clone, Copy)]
pub struct ColorTransition {
    pub from: Color,
    pub to: Color,
    pub start: f64,
    pub dur: f64,
}

impl ColorTransition {
    pub fn instant(c: Color) -> ColorTransition {
        ColorTransition { from: c, to: c, start: 0.0, dur: 0.0 }
    }

    /// 朝新目标过渡：以当前显示色为起点，保证连续。
    pub fn retarget(&mut self, target: Color, now: f64, dur: f64) {
        if self.to == target {
            return;
        }
        let cur = self.value(now);
        self.from = cur;
        self.to = target;
        self.start = now;
        self.dur = dur;
    }

    pub fn value(&self, now: f64) -> Color {
        if self.dur <= 0.0 {
            return self.to;
        }
        let t = ((now - self.start) / self.dur).clamp(0.0, 1.0) as f32;
        lerp_color(self.from, self.to, t)
    }

    pub fn is_active(&self, now: f64) -> bool {
        self.dur > 0.0 && (now - self.start) < self.dur
    }
}
