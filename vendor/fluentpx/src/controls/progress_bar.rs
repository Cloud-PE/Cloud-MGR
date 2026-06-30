//! ProgressBar（进度条，确定 + 不确定）。
//!
//! 真值来源：`controls/dev/ProgressBar/ProgressBar_themeresources.xaml`
//! * 前景 `AccentFillColorDefault`，轨道 `ControlStrongFillColorDefault`
//! * TrackHeight 1、MinHeight 3、确定段圆角 1.5、轨道圆角 0.5
//! * 不确定：两段 accent 在轨道上循环滑动（容器动画近似，周期约 2s）

use crate::widget::*;

const BAR_H: f32 = 3.0;
const CORNER: f32 = 1.5;
const INDET_PERIOD: f64 = 2.0;

pub struct ProgressBar {
    pub value: f32, // 0..1（确定模式）
    pub indeterminate: bool,
    rect: Rect,
}

impl ProgressBar {
    pub fn determinate(value: f32) -> ProgressBar {
        ProgressBar { value: value.clamp(0.0, 1.0), indeterminate: false, rect: Rect::default() }
    }
    pub fn indeterminate() -> ProgressBar {
        ProgressBar { value: 0.0, indeterminate: true, rect: Rect::default() }
    }
}

impl Widget for ProgressBar {
    fn measure(&mut self, available: Size) -> Size {
        Size { w: available.w.clamp(160.0, 320.0), h: BAR_H }
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = Rect { x: rect.x, y: rect.center_y() - BAR_H / 2.0, w: rect.w, h: BAR_H };
    }

    fn hit_test(&self, _p: Point) -> bool {
        false
    }

    fn paint(&mut self, ctx: &mut PaintCtx) {
        let t = ctx.tokens;
        let r = self.rect;
        // 轨道
        ctx.painter.fill_rounded_rect(r, CORNER, t.control_strong_fill_default.with_opacity(0.6));

        let accent = t.accent_fill_default();
        if self.indeterminate {
            // 两段滑动（相位错开），用裁剪限制在轨道内。
            ctx.painter.push_clip(r);
            let phase = ((ctx.now % INDET_PERIOD) / INDET_PERIOD) as f32;
            let seg = |p: f32, wfrac: f32| {
                // p:0..1 段中心从左到右；越界部分被裁剪。
                let w = r.w * wfrac;
                let travel = r.w + w;
                let x = r.x - w + travel * p;
                Rect { x, y: r.y, w, h: r.h }
            };
            // 缓动让两段有快慢变化
            let e = |x: f32| 1.0 - (1.0 - x).powi(3); // ease-out cubic
            ctx.painter.fill_rounded_rect(seg(e(phase), 0.35), CORNER, accent);
            let p2 = (phase + 0.45) % 1.0;
            ctx.painter.fill_rounded_rect(seg(e(p2), 0.2), CORNER, accent);
            ctx.painter.pop_clip();
        } else {
            let fill = Rect { x: r.x, y: r.y, w: r.w * self.value, h: r.h };
            ctx.painter.fill_rounded_rect(fill, CORNER, accent);
        }
    }

    fn is_animating(&self, _now: f64) -> bool {
        self.indeterminate
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Slider
    }
}
