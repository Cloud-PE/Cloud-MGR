//! 颜色：`#AARRGGBB` 解析、D2D `D2D1_COLOR_F` 转换、半透明 sRGB 直通 alpha 叠加。
//!
//! WinUI 的半透明填充 token（如 `#0FFFFFFF`）最终颜色取决于其下方的实际背景。
//! D2D 在渲染目标的颜色空间（默认 B8G8R8A8_UNORM，sRGB 编码、直通 alpha）中混合，
//! 因此既可以直接用半透明画刷让 D2D 合成，也可以用 [`Color::over`] 预先算出合成色。
//! 关于「sRGB vs 线性」混合空间的偏色调试见 `gfx.rs`（集中控制像素格式）。

use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

/// 直通（非预乘）alpha 的 8bit/通道颜色，内部按 ARGB 存储。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color { a: 0, r: 0, g: 0, b: 0 };

    pub const fn argb(a: u8, r: u8, g: u8, b: u8) -> Color {
        Color { a, r, g, b }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { a: 0xFF, r, g, b }
    }

    /// 解析 `#AARRGGBB` 或 `#RRGGBB`（与 XAML 的 `Color` 字面量一致）。
    /// 编译期/启动期使用，遇到非法串直接 panic，避免错误数值悄悄进入比对。
    pub const fn hex(s: &str) -> Color {
        let bytes = s.as_bytes();
        match bytes.len() {
            // #RRGGBB
            7 => Color {
                a: 0xFF,
                r: hex2(bytes[1], bytes[2]),
                g: hex2(bytes[3], bytes[4]),
                b: hex2(bytes[5], bytes[6]),
            },
            // #AARRGGBB
            9 => Color {
                a: hex2(bytes[1], bytes[2]),
                r: hex2(bytes[3], bytes[4]),
                g: hex2(bytes[5], bytes[6]),
                b: hex2(bytes[7], bytes[8]),
            },
            _ => panic!("color hex must be #RRGGBB or #AARRGGBB"),
        }
    }

    /// 整体不透明度乘子（对应 XAML `Opacity="0.9"` 这类写法，只缩放 alpha）。
    pub fn with_opacity(self, opacity: f32) -> Color {
        let a = (self.a as f32 * opacity).round().clamp(0.0, 255.0) as u8;
        Color { a, ..self }
    }

    /// 直通 alpha、在 sRGB（gamma 编码）空间中，把 `self` 叠加到不透明背景 `bg` 上。
    /// 这是 D2D 默认渲染目标的混合行为，用于预算半透明 token 的最终色以便逐像素比对。
    pub fn over(self, bg: Color) -> Color {
        let sa = self.a as f32 / 255.0;
        let blend = |fg: u8, bg: u8| -> u8 {
            ((fg as f32 * sa) + (bg as f32 * (1.0 - sa))).round().clamp(0.0, 255.0) as u8
        };
        Color {
            a: 0xFF,
            r: blend(self.r, bg.r),
            g: blend(self.g, bg.g),
            b: blend(self.b, bg.b),
        }
    }

    /// 转为 D2D 颜色（直通 alpha，分量 0..1）。
    pub fn d2d(self) -> D2D1_COLOR_F {
        D2D1_COLOR_F {
            r: self.r as f32 / 255.0,
            g: self.g as f32 / 255.0,
            b: self.b as f32 / 255.0,
            a: self.a as f32 / 255.0,
        }
    }
}

const fn hex2(hi: u8, lo: u8) -> u8 {
    nibble(hi) * 16 + nibble(lo)
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("invalid hex digit in color literal"),
    }
}

/// 线性渐变的一个停靠点（offset 0..1 + 颜色），对应 XAML `<GradientStop>`。
#[derive(Clone, Copy, Debug)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Color,
}

/// 线性渐变画刷描述（对应 XAML `LinearGradientBrush`）。
///
/// `absolute` 为 true 时 `start`/`end` 为设备无关的绝对坐标（XAML
/// `MappingMode="Absolute"`，如 ElevationBorderBrush 的 `0,0 -> 0,3`）；
/// 为 false 时相对于绘制矩形（`RelativeToBoundingBox`）。
/// `flip_y` 对应 AccentControlElevationBorderBrush 的 `ScaleTransform ScaleY=-1`。
#[derive(Clone, Debug)]
pub struct LinearGradient {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub absolute: bool,
    pub flip_y: bool,
    pub stops: Vec<GradientStop>,
}

impl LinearGradient {
    /// 计算把单位渐变映射到目标矩形所需的 brush 变换矩阵（用于 RelativeToBoundingBox /
    /// flip_y）。Absolute 模式由调用方按缩放后的逻辑坐标直接给点。
    pub fn brush_transform(&self, rect_top: f32, rect_height: f32) -> Matrix3x2 {
        if self.flip_y {
            // 绕矩形竖直中心翻转：y' = top + height - (y - top) = 2*center - y
            let center = rect_top + rect_height / 2.0;
            Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: -1.0, M31: 0.0, M32: 2.0 * center }
        } else {
            Matrix3x2::identity()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_argb() {
        let c = Color::hex("#0FFFFFFF");
        assert_eq!((c.a, c.r, c.g, c.b), (0x0F, 0xFF, 0xFF, 0xFF));
        let c = Color::hex("#76B9ED");
        assert_eq!((c.a, c.r, c.g, c.b), (0xFF, 0x76, 0xB9, 0xED));
    }

    #[test]
    fn srgb_composite_matches_straight_alpha() {
        // #0FFFFFFF (alpha 15/255≈0.0588) 白色叠加到 #202020 基底。
        let base = Color::hex("#202020");
        let out = Color::hex("#0FFFFFFF").over(base);
        // 0x20=32; 32*(1-0.0588)+255*0.0588 = 30.12+15.0 = 45.12 -> 45 = 0x2D
        assert_eq!(out.r, 45);
        assert_eq!(out.a, 0xFF);
    }
}
