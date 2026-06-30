//! Fluent 设计 token 的深/浅两套真值，全部从 microsoft-ui-xaml 源码
//! `controls/dev/CommonStyles/Common_themeresources_any.xaml` 抽取（非凭记忆）。
//!
//! 半透明 token 保留原始 alpha（如 `ControlFillColorDefault = #0FFFFFFF`），
//! 由 `gfx`/控件在真实背景上做 alpha 混合，详见 `color::Color::over`。

use crate::color::{Color, GradientStop, LinearGradient};

/// 默认 Windows 强调色调色板（基色 `SystemAccentColor = #0078D4`）。
///
/// Light2/Dark2/Dark3 已被源码 test 字典确认；其余为该基色的标准派生 shade。
/// 「跟随系统强调色」(UISettings.GetColorValue) 列为 TODO，先用可复现的默认值保证比对稳定。
#[derive(Clone, Copy)]
pub struct AccentPalette {
    pub light3: Color,
    pub light2: Color,
    pub light1: Color,
    pub base: Color,
    pub dark1: Color,
    pub dark2: Color,
    pub dark3: Color,
}

impl AccentPalette {
    /// Windows 默认强调色调色板真值（取自系统 `Explorer\Accent\AccentPalette`，
    /// 与真·WinUI AccentButton 逐像素一致：深色 AccentButton = Light2 = #4CC2FF）。
    /// 旧版按基色「派生」出的 shade 不准（light2 曾误为 #76B9ED）。
    pub const DEFAULT: AccentPalette = AccentPalette {
        light3: Color::hex("#99EBFF"),
        light2: Color::hex("#4CC2FF"),
        light1: Color::hex("#0091F8"),
        base: Color::hex("#0078D4"),
        dark1: Color::hex("#0067C0"),
        dark2: Color::hex("#003E92"),
        dark3: Color::hex("#001A68"),
    };
}

/// 一套主题（深或浅）下控件需要的全部颜色 token。
#[derive(Clone, Copy)]
pub struct Tokens {
    pub is_dark: bool,
    pub accent: AccentPalette,

    // —— 文字 ——
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_tertiary: Color,
    pub text_disabled: Color,
    pub text_on_accent_primary: Color,
    pub text_on_accent_secondary: Color,
    pub text_on_accent_disabled: Color,
    /// TextFillColorInverse：用于强调/语义色底上的符号（如 InfoBar 图标内的 i/✕）。
    pub text_inverse: Color,

    // —— 控件填充（半透明叠加）——
    pub control_fill_default: Color,
    pub control_fill_secondary: Color,
    pub control_fill_tertiary: Color,
    pub control_fill_disabled: Color,
    pub control_fill_transparent: Color,
    pub control_fill_input_active: Color,
    pub control_strong_fill_default: Color,
    pub control_strong_fill_disabled: Color,
    pub control_solid_fill_default: Color,

    // —— Subtle（按钮悬停/按下）——
    pub subtle_fill_secondary: Color,
    pub subtle_fill_tertiary: Color,

    // —— ControlAlt（ComboBox 等输入背景）——
    pub control_alt_fill_secondary: Color,
    pub control_alt_fill_tertiary: Color,
    pub control_alt_fill_quarternary: Color,
    pub control_alt_fill_disabled: Color,

    // —— 描边 ——
    pub stroke_default: Color,
    pub stroke_secondary: Color,
    pub stroke_on_accent_default: Color,
    pub stroke_on_accent_secondary: Color,
    pub strong_stroke_default: Color,
    pub strong_stroke_disabled: Color,
    pub surface_stroke_flyout: Color,
    pub surface_stroke_default: Color,
    pub divider_stroke_default: Color,
    pub focus_stroke_outer: Color,
    pub focus_stroke_inner: Color,

    // —— 卡片/层/背景 ——
    pub card_bg_default: Color,
    pub card_bg_secondary: Color,
    pub card_stroke_default: Color,
    pub smoke_fill_default: Color,
    pub solid_bg_base: Color,
    pub solid_bg_secondary: Color,
    pub solid_bg_tertiary: Color,
    pub solid_bg_quarternary: Color,

    // —— 强调填充 ——
    pub accent_fill_disabled: Color,

    // —— 系统语义色（前景 + InfoBar 背景）——
    pub system_success: Color,
    pub system_caution: Color,
    pub system_critical: Color,
    pub system_neutral: Color,
    pub system_success_bg: Color,
    pub system_caution_bg: Color,
    pub system_critical_bg: Color,
    pub system_attention_bg: Color,
}

impl Tokens {
    /// 深色（Default 字典）真值。
    pub const fn dark() -> Tokens {
        let accent = AccentPalette::DEFAULT;
        Tokens {
            is_dark: true,
            accent,
            text_primary: Color::hex("#FFFFFFFF"),
            text_secondary: Color::hex("#C5FFFFFF"),
            text_tertiary: Color::hex("#87FFFFFF"),
            text_disabled: Color::hex("#5DFFFFFF"),
            text_on_accent_primary: Color::hex("#FF000000"),
            text_on_accent_secondary: Color::hex("#80000000"),
            text_on_accent_disabled: Color::hex("#87FFFFFF"),
            text_inverse: Color::hex("#E4000000"),

            control_fill_default: Color::hex("#0FFFFFFF"),
            control_fill_secondary: Color::hex("#15FFFFFF"),
            control_fill_tertiary: Color::hex("#08FFFFFF"),
            control_fill_disabled: Color::hex("#0BFFFFFF"),
            control_fill_transparent: Color::hex("#00FFFFFF"),
            control_fill_input_active: Color::hex("#B31E1E1E"),
            control_strong_fill_default: Color::hex("#8BFFFFFF"),
            control_strong_fill_disabled: Color::hex("#3FFFFFFF"),
            control_solid_fill_default: Color::hex("#454545"),

            subtle_fill_secondary: Color::hex("#0FFFFFFF"),
            subtle_fill_tertiary: Color::hex("#0AFFFFFF"),

            control_alt_fill_secondary: Color::hex("#19000000"),
            control_alt_fill_tertiary: Color::hex("#0BFFFFFF"),
            control_alt_fill_quarternary: Color::hex("#12FFFFFF"),
            control_alt_fill_disabled: Color::hex("#00FFFFFF"),

            stroke_default: Color::hex("#12FFFFFF"),
            stroke_secondary: Color::hex("#18FFFFFF"),
            stroke_on_accent_default: Color::hex("#14FFFFFF"),
            stroke_on_accent_secondary: Color::hex("#23000000"),
            strong_stroke_default: Color::hex("#8BFFFFFF"),
            strong_stroke_disabled: Color::hex("#28FFFFFF"),
            surface_stroke_flyout: Color::hex("#33000000"),
            surface_stroke_default: Color::hex("#66757575"),
            divider_stroke_default: Color::hex("#15FFFFFF"),
            focus_stroke_outer: Color::hex("#FFFFFFFF"),
            focus_stroke_inner: Color::hex("#B3000000"),

            card_bg_default: Color::hex("#0DFFFFFF"),
            card_bg_secondary: Color::hex("#08FFFFFF"),
            card_stroke_default: Color::hex("#19000000"),
            smoke_fill_default: Color::hex("#4D000000"),
            solid_bg_base: Color::hex("#202020"),
            solid_bg_secondary: Color::hex("#1C1C1C"),
            solid_bg_tertiary: Color::hex("#282828"),
            solid_bg_quarternary: Color::hex("#2C2C2C"),

            accent_fill_disabled: Color::hex("#28FFFFFF"),

            system_success: Color::hex("#6CCB5F"),
            system_caution: Color::hex("#FCE100"),
            system_critical: Color::hex("#FF99A4"),
            system_neutral: Color::hex("#8BFFFFFF"),
            system_success_bg: Color::hex("#393D1B"),
            system_caution_bg: Color::hex("#433519"),
            system_critical_bg: Color::hex("#442726"),
            system_attention_bg: Color::hex("#08FFFFFF"),
        }
    }

    /// 浅色（Light 字典）真值。
    pub const fn light() -> Tokens {
        let accent = AccentPalette::DEFAULT;
        Tokens {
            is_dark: false,
            accent,
            text_primary: Color::hex("#E4000000"),
            text_secondary: Color::hex("#9E000000"),
            text_tertiary: Color::hex("#72000000"),
            text_disabled: Color::hex("#5C000000"),
            text_on_accent_primary: Color::hex("#FFFFFFFF"),
            text_on_accent_secondary: Color::hex("#B3FFFFFF"),
            text_on_accent_disabled: Color::hex("#FFFFFFFF"),
            text_inverse: Color::hex("#FFFFFFFF"),

            control_fill_default: Color::hex("#B3FFFFFF"),
            control_fill_secondary: Color::hex("#80F9F9F9"),
            control_fill_tertiary: Color::hex("#4DF9F9F9"),
            control_fill_disabled: Color::hex("#4DF9F9F9"),
            control_fill_transparent: Color::hex("#00FFFFFF"),
            control_fill_input_active: Color::hex("#FFFFFFFF"),
            control_strong_fill_default: Color::hex("#72000000"),
            control_strong_fill_disabled: Color::hex("#51000000"),
            control_solid_fill_default: Color::hex("#FFFFFF"),

            subtle_fill_secondary: Color::hex("#09000000"),
            subtle_fill_tertiary: Color::hex("#06000000"),

            control_alt_fill_secondary: Color::hex("#06000000"),
            control_alt_fill_tertiary: Color::hex("#0F000000"),
            control_alt_fill_quarternary: Color::hex("#18000000"),
            control_alt_fill_disabled: Color::hex("#00FFFFFF"),

            stroke_default: Color::hex("#0F000000"),
            stroke_secondary: Color::hex("#29000000"),
            stroke_on_accent_default: Color::hex("#14FFFFFF"),
            stroke_on_accent_secondary: Color::hex("#66000000"),
            strong_stroke_default: Color::hex("#72000000"),
            strong_stroke_disabled: Color::hex("#37000000"),
            surface_stroke_flyout: Color::hex("#0F000000"),
            surface_stroke_default: Color::hex("#66757575"),
            divider_stroke_default: Color::hex("#0F000000"),
            focus_stroke_outer: Color::hex("#E4000000"),
            focus_stroke_inner: Color::hex("#B3FFFFFF"),

            card_bg_default: Color::hex("#B3FFFFFF"),
            card_bg_secondary: Color::hex("#80F6F6F6"),
            card_stroke_default: Color::hex("#0F000000"),
            smoke_fill_default: Color::hex("#4D000000"),
            solid_bg_base: Color::hex("#F3F3F3"),
            solid_bg_secondary: Color::hex("#EEEEEE"),
            solid_bg_tertiary: Color::hex("#F9F9F9"),
            solid_bg_quarternary: Color::hex("#FFFFFF"),

            accent_fill_disabled: Color::hex("#37000000"),

            system_success: Color::hex("#0F7B0F"),
            system_caution: Color::hex("#9D5D00"),
            system_critical: Color::hex("#C42B1C"),
            system_neutral: Color::hex("#72000000"),
            system_success_bg: Color::hex("#DFF6DD"),
            system_caution_bg: Color::hex("#FFF4CE"),
            system_critical_bg: Color::hex("#FDE7E9"),
            system_attention_bg: Color::hex("#80F6F6F6"),
        }
    }

    // —— 强调填充画刷（由调色板 + 不透明度派生，见源码 AccentFillColor*Brush）——

    /// `AccentFillColorDefaultBrush`：深色=Light2，浅色=Dark1。
    pub fn accent_fill_default(&self) -> Color {
        if self.is_dark { self.accent.light2 } else { self.accent.dark1 }
    }
    /// `AccentFillColorSecondaryBrush`：同基色 @ Opacity 0.9。
    pub fn accent_fill_secondary(&self) -> Color {
        self.accent_fill_default().with_opacity(0.9)
    }
    /// `AccentFillColorTertiaryBrush`：同基色 @ Opacity 0.8。
    pub fn accent_fill_tertiary(&self) -> Color {
        self.accent_fill_default().with_opacity(0.8)
    }

    // —— 用户指定调色（覆盖 WinUI 默认）：ComboBox / 标准按钮 / 搜索框 —————————————————
    // 深色 / 浅色两套实测值，按 is_dark 取。hover/pressed 在 rest 底上叠加细微深浅。

    /// ComboBox 与「白/黑」标准按钮的底色（rest）：深 #373737 / 浅 #FEFEFE。
    pub fn input_solid_bg(&self) -> Color {
        if self.is_dark { Color::hex("#373737") } else { Color::hex("#FEFEFE") }
    }
    /// 悬停（用户指定）：深 #434343 / 浅 #FAFAFA。
    pub fn input_bg_hover(&self) -> Color {
        if self.is_dark { Color::hex("#434343") } else { Color::hex("#FAFAFA") }
    }
    /// 按下 / 打开（用户指定）：深 #323232 / 浅 #FAFAFA。
    pub fn input_bg_pressed(&self) -> Color {
        if self.is_dark { Color::hex("#323232") } else { Color::hex("#FAFAFA") }
    }
    /// ComboBox / 标准按钮 边框（整圈）：深 #414141 / 浅 #ECECEC。
    pub fn input_border(&self) -> Color {
        if self.is_dark { Color::hex("#414141") } else { Color::hex("#ECECEC") }
    }
    /// ComboBox / 标准按钮 底边描边：深 #3A3A3A / 浅 #D3D3D3。
    pub fn input_border_bottom(&self) -> Color {
        // 底边只比四周描边「微深一点」做轻微立体感；浅色原 #D3D3D3 太深→底边「露出来」，调淡。
        if self.is_dark { Color::hex("#3A3A3A") } else { Color::hex("#E0E0E0") }
    }
    /// 搜索框（TextBox）边框（整圈）：深 #353535 / 浅 #E5E5E5。
    pub fn search_border(&self) -> Color {
        if self.is_dark { Color::hex("#353535") } else { Color::hex("#E5E5E5") }
    }
    /// 搜索框 底边描边：深 #303030 / 浅 #DBDBDB（原 #CCCCCC 太深，底边「露出来」，调淡）。
    pub fn search_border_bottom(&self) -> Color {
        if self.is_dark { Color::hex("#303030") } else { Color::hex("#DBDBDB") }
    }

    // —— 投影（CSS box-shadow → (offset_y, blur, color)，取自 fluent-svelte theme.css）——
    /// Flyout/下拉弹窗投影：0 8 16，黑 14%。
    pub fn flyout_shadow(&self) -> (f32, f32, Color) {
        (8.0, 16.0, Color::hex("#24000000"))
    }
    /// 对话框投影：0 32 64，黑 ~19%（深）/ 37%（浅）。
    pub fn dialog_shadow(&self) -> (f32, f32, Color) {
        (26.0, 52.0, if self.is_dark { Color::hex("#36000000") } else { Color::hex("#5E000000") })
    }
    /// 卡片投影：0 2 4，黑 4%（深，很轻）/ 13%（浅）。
    pub fn card_shadow(&self) -> (f32, f32, Color) {
        (2.0, 6.0, if self.is_dark { Color::hex("#12000000") } else { Color::hex("#21000000") })
    }

    /// `ControlElevationBorderBrush`：立体高光边（普通按钮/ComboBox 边框）。
    /// 源码 MappingMode=Absolute，0,0 -> 0,3；stop0.33=StrokeSecondary，stop1.0=StrokeDefault。
    /// **按主题区分**（与源码两套字典一致）：深色字典无 ScaleY 翻转→高光在顶部；
    /// 浅色字典带 `ScaleY=-1`→翻到底部。故 flip_y = !is_dark。
    pub fn control_elevation_border(&self) -> LinearGradient {
        LinearGradient {
            start: (0.0, 0.0),
            end: (0.0, 3.0),
            absolute: true,
            flip_y: !self.is_dark,
            stops: vec![
                GradientStop { offset: 0.33, color: self.stroke_secondary },
                GradientStop { offset: 1.0, color: self.stroke_default },
            ],
        }
    }

    /// `AccentControlElevationBorderBrush`：蓝色按钮边框（OnAccent 渐变）。
    /// 源码：Absolute 0,0 -> 0,3，且带 ScaleY=-1 翻转；
    /// stop0.33=OnAccentSecondary，stop1.0=OnAccentDefault。
    pub fn accent_control_elevation_border(&self) -> LinearGradient {
        LinearGradient {
            start: (0.0, 0.0),
            end: (0.0, 3.0),
            absolute: true,
            flip_y: true,
            stops: vec![
                GradientStop { offset: 0.33, color: self.stroke_on_accent_secondary },
                GradientStop { offset: 1.0, color: self.stroke_on_accent_default },
            ],
        }
    }
}
