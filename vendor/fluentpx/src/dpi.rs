//! 高 DPI：缩放因子 + 设备像素布局取整（layout rounding）。
//!
//! 逻辑像素 × scale 得到设备像素后，必须取整到整数设备像素，否则 1px 边框/分隔线
//! 会落在像素边界中间而发虚。这里复刻 XAML 的「四舍五入到最近设备像素」规则。

/// 一个窗口当前的 DPI 状态。scale = dpi / 96。
#[derive(Clone, Copy, Debug)]
pub struct Dpi {
    pub dpi: u32,
}

impl Dpi {
    pub const DEFAULT: Dpi = Dpi { dpi: 96 };

    pub fn new(dpi: u32) -> Dpi {
        Dpi { dpi: dpi.max(1) }
    }

    /// 缩放因子（100%→1.0, 125%→1.25, 150%→1.5, 175%→1.75）。
    pub fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }

    /// 把逻辑坐标四舍五入到最近的设备像素，再换算回逻辑坐标。
    /// D2D 以逻辑（DIP）坐标绘制但渲染目标 DPI=缩放后，这样可保证边缘落在整设备像素上。
    pub fn round_to_pixel(&self, logical: f32) -> f32 {
        let s = self.scale();
        (logical * s).round() / s
    }

    /// 半像素对齐：用于 1px 描边，使描边中心线落在设备像素中心，得到清晰 1px。
    /// 返回的是「整设备像素 + 0.5 设备像素」对应的逻辑坐标。
    pub fn align_stroke_center(&self, logical_edge: f32, stroke_px: f32) -> f32 {
        let s = self.scale();
        let dev = (logical_edge * s).round() + stroke_px * 0.5;
        dev / s
    }
}
