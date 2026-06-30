//! ProgressRing（环形进度，不确定 spinner）。
//!
//! 真值来源 = fluent-svelte（逐项核对，非凭记忆）：
//! * 模板/几何：`ProgressRing/ProgressRing.svelte`
//!   - `<svg viewBox="0 0 16 16" width={size} height={size}>`，默认 size=32。
//!     即 16×16 视口等比缩放到 size → 比例因子 size/16。
//!   - `<circle cx="50%" cy="50%" r="7">`：圆心 (8,8)，半径 7（视口单位）。
//!   - `class:indeterminate`（value 为 undefined/null/NaN 时）→ 走不确定动画。
//!     determinate 时 `stroke-dashoffset = (100-value)/100 * 周长`（本控件实现不确定态）。
//! * 描边/颜色/最小尺寸：`ProgressRing/ProgressRing.scss`
//!   - `.progress-ring { min-inline-size:16px; min-block-size:16px; outline:none }`。
//!   - `circle { transform: rotate(-90deg); transform-origin: 50% 50%;`
//!     `stroke: var(--accent-default); stroke-width: 1.5; stroke-linecap: round;`
//!     `stroke-dasharray: 43.97 }`。
//!     · `rotate(-90deg)` → SVG 路径起点（3 点钟 = 0°）转到 12 点钟（顶端）。
//!     · `--accent-default` = `--fds-accent-default`（深=accent-light-2，浅=accent-dark-1）
//!       → fluentpx `accent_fill_default()`。背景轨道无（只画前景弧）。
//!     · 周长 = 2π·7 = 43.98（scss 注为 43.97）。
//! * 不确定动画：`@keyframes progress-ring-indeterminate`，`animation: 2s linear infinite`：
//!     - 0%   `stroke-dasharray: 0.01px 43.97px; transform: rotate(0)`
//!     - 50%  `stroke-dasharray: 21.99px 21.99px; transform: rotate(450deg)`
//!     - 100% `stroke-dasharray: 0.01px 43.97px; transform: rotate(1080deg)`
//!   关键点（与官方 LottieGen XAML 版本不同，此处以 fluent-svelte 为准）：
//!     · **linear** 计时——关键帧间按原始进度线性插值，无缓动。
//!     · dasharray `[dash, gap]` + dashoffset=0（indeterminate 时 value 为 NaN → 0）：
//!       可见弧**恒从路径起点起**，仅 `dash` 长度变化（0.01→21.99→0.01 视口单位）。
//!       故 phase A 弧由「点」长到半圈（180°），phase B 由半圈缩回「点」，
//!       起点始终是旋转中的路径起点（非两端各自 trim）。
//!     · 旋转 0°→450°→1080°（整两圈半），叠加 -90° 基准。
//!     · `stroke-linecap: round` → dash≈0 时圆头帽渲染成一个圆点。

use crate::anim::lerp;
use crate::widget::*;

/// 默认尺寸（svelte:10 `size = 32`）。
const SIZE: f32 = 32.0;
/// 最小尺寸（scss:18-19 min-inline/block-size = 16px）。
const MIN_SIZE: f32 = 16.0;
/// 一次完整循环时长（scss:35 `animation: ... 2s ...`）。
const CYCLE: f64 = 2.0;
/// 半径占尺寸比：r=7 / viewBox 16（svelte:65 r="7"）。
const RADIUS_RATIO: f32 = 7.0 / 16.0; // = 0.4375
/// 描边占尺寸比：stroke-width 1.5 / viewBox 16（scss:28 width: 1.5）。
const STROKE_RATIO: f32 = 1.5 / 16.0; // = 0.09375
/// 周长（视口单位）：2π·7 ≈ 43.98（scss 用 43.97）。
const CIRCUMFERENCE: f32 = 2.0 * std::f32::consts::PI * 7.0;

/// 关键帧的 stroke-dasharray dash 值（视口单位，scss:3/7/11）。
const DASH_MIN: f32 = 0.01;
const DASH_HALF: f32 = 21.99;
/// 关键帧旋转角（度，scss:4/8/12）。
const ROT_MID: f32 = 450.0;
const ROT_END: f32 = 1080.0;

pub struct ProgressRing {
    rect: Rect,
    /// 对应 `class:indeterminate`：true=不确定（旋转动画），false=隐藏/停止。
    /// 本控件复刻不确定态 spinner；非动画态整体不绘制。
    active: bool,
}

impl ProgressRing {
    pub fn new() -> ProgressRing {
        ProgressRing { rect: Rect::default(), active: true }
    }

    /// 设置是否激活（运行不确定旋转动画）；false 时停止并隐藏。
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }
}

impl Default for ProgressRing {
    fn default() -> Self {
        ProgressRing::new()
    }
}

impl Widget for ProgressRing {
    fn measure(&mut self, _available: Size) -> Size {
        Size { w: SIZE, h: SIZE }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn hit_test(&self, _p: Point) -> bool {
        false
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        // 非激活：不画（停止动画）。
        if !self.active {
            return;
        }

        let t = ctx.tokens;
        // 按排布矩形自适应（可放进按钮里做小尺寸 spinner）；不小于 MinSize，居中绘制。
        let sz = self.rect.w.min(self.rect.h).max(MIN_SIZE);
        let cx = self.rect.center_x();
        let cy = self.rect.center_y();
        let thickness = sz * STROKE_RATIO;
        let r = sz * RADIUS_RATIO;

        // 单一 2000ms 时间线，linear：p∈[0,1) 同时驱动 dash 长度与旋转（scss keyframes）。
        let p = (ctx.now.rem_euclid(CYCLE) / CYCLE) as f32;

        // —— linear 插值关键帧（无缓动）——
        // dash：0%→50% 0.01→21.99；50%→100% 21.99→0.01（视口单位）。
        // rot ：0%→50% 0→450°；50%→100% 450→1080°，叠加 -90° 基准。
        let (dash, rotation_deg) = if p < 0.5 {
            let f = p / 0.5;
            (lerp(DASH_MIN, DASH_HALF, f), lerp(0.0, ROT_MID, f))
        } else {
            let f = (p - 0.5) / 0.5;
            (lerp(DASH_HALF, DASH_MIN, f), lerp(ROT_MID, ROT_END, f))
        };

        // dasharray 模型：可见弧恒从路径起点起（dashoffset=0），长度 = dash。
        // SVG 路径起点在 3 点钟（0°），顺时针扫；circle 上 `rotate(-90deg)` 把起点移到顶端，
        // 动画 rotate 再叠加。gfx::stroke_arc：0°=右、顺时针为正——与 SVG 同向。
        // 故起点角 = 旋转 - 90°；扫角 = dash/周长×360°（始终从起点向后扫）。
        let start_deg = rotation_deg - 90.0;
        let sweep_deg = dash / CIRCUMFERENCE * 360.0;

        // 前景色：--accent-default = accent_fill_default()（深 accent-light-2 / 浅 accent-dark-1）。
        // 无背景轨道。stroke_arc 用圆头描边（round cap），对应 scss `stroke-linecap: round`，
        // 使 dash≈0 时渲染成一个圆点。
        ctx.painter.stroke_arc(cx, cy, r, start_deg, sweep_deg, t.accent_fill_default(), thickness);
    }

    fn is_animating(&self, _now: f64) -> bool {
        // 仅激活时持续旋转。
        self.active
    }

    fn accessible_role(&self) -> AccessibleRole {
        // svelte:55 role = value ? "progressbar" : "status"；不确定态为 "status"。
        // 现有角色枚举无进度/状态项，沿用最接近的 Slider（RangeValue 语义），与原实现一致。
        AccessibleRole::Slider
    }
}
