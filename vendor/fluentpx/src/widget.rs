//! 控件抽象：几何类型、`Widget` trait（measure/arrange/hit_test/paint/on_event）、
//! 视觉状态枚举、输入事件、以及为将来无障碍（UIA）预留的接口。

use crate::dpi::Dpi;
use crate::gfx::Painter;
use crate::tokens::Tokens;

/// 逻辑像素点。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// 逻辑像素尺寸。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

/// 逻辑像素矩形（左上 + 宽高）。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f32 { self.x + self.w }
    pub fn bottom(&self) -> f32 { self.y + self.h }
    pub fn center_y(&self) -> f32 { self.y + self.h / 2.0 }
    pub fn center_x(&self) -> f32 { self.x + self.w / 2.0 }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// 四周内缩 `d` 逻辑像素（用于 1px 边框落在内沿）。
    pub fn inset(&self, d: f32) -> Rect {
        Rect { x: self.x + d, y: self.y + d, w: (self.w - 2.0 * d).max(0.0), h: (self.h - 2.0 * d).max(0.0) }
    }
}

/// WinUI CommonStates 视觉状态。Focused 作为独立标志叠加（可与其它态并存）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VisualState {
    Normal,
    PointerOver,
    Pressed,
    Disabled,
}

/// 控件交互状态的通用记录。具体控件在此基础上扩展自有状态（如开关 on/off）。
#[derive(Clone, Copy, Debug)]
pub struct Interaction {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub enabled: bool,
}

impl Default for Interaction {
    fn default() -> Self {
        Interaction { hovered: false, pressed: false, focused: false, enabled: true }
    }
}

impl Interaction {
    /// 按 WinUI 优先级映射到视觉状态：Disabled > Pressed > PointerOver > Normal。
    pub fn visual_state(&self) -> VisualState {
        if !self.enabled {
            VisualState::Disabled
        } else if self.pressed && self.hovered {
            // 仅在指针仍**压在控件上**时显示 Pressed；按住后移开 → 回落 Normal（与 WinUI 一致）。
            VisualState::Pressed
        } else if self.hovered {
            VisualState::PointerOver
        } else {
            VisualState::Normal
        }
    }
}

/// 输入事件（坐标为逻辑像素，相对窗口客户区）。
#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    PointerMove(Point),
    PointerDown(Point),
    PointerUp(Point),
    PointerLeave,
    KeyDown(u32),
    KeyUp(u32),
    /// 已翻译的字符输入（WM_CHAR），供 TextBox 等编辑控件使用。
    Char(char),
    /// 鼠标滚轮，正值向上。
    Wheel(f32),
    /// 右键 / 上下文菜单请求（坐标为请求位置）。
    ContextMenu(Point),
}

/// 事件处理结果：是否需要重绘，以及是否仍有动画在进行（需持续刷新）。
#[derive(Clone, Copy, Debug, Default)]
pub struct EventResult {
    pub redraw: bool,
    pub animating: bool,
}

impl EventResult {
    pub const NONE: EventResult = EventResult { redraw: false, animating: false };
    pub const REDRAW: EventResult = EventResult { redraw: true, animating: false };
    pub fn or(self, o: EventResult) -> EventResult {
        EventResult { redraw: self.redraw || o.redraw, animating: self.animating || o.animating }
    }
}

/// 绘制上下文：携带 painter、当前主题 token、DPI、动画时间基准。
pub struct PaintCtx<'a, 'b> {
    pub painter: &'a mut Painter<'b>,
    pub tokens: &'a Tokens,
    pub dpi: Dpi,
    /// 单调时间（秒），用于动画曲线求值。
    pub now: f64,
    /// 客户区逻辑尺寸，供覆盖层（如 ContentDialog 遮罩）铺满全窗。
    pub viewport: Size,
}

/// 为将来 UIA 预留的无障碍角色（暂不实装，但控件需声明，避免以后无处挂接）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AccessibleRole {
    Button,
    CheckBox,
    Slider,
    ComboBox,
    List,
    ListItem,
    Tab,
    ToolTip,
    Dialog,
}

/// 控件统一接口。
/// 鼠标指针形状：宿主在 WM_SETCURSOR 据此设系统光标（默认箭头；文本框内 I 形）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cursor {
    /// 箭头（IDC_ARROW）
    Default,
    /// I 形文本输入光标（IDC_IBEAM）
    Text,
}

pub trait Widget {
    /// 在给定可用尺寸下测量期望尺寸（逻辑像素）。
    fn measure(&mut self, available: Size) -> Size;

    /// 安排最终布局矩形。
    fn arrange(&mut self, rect: Rect);

    /// 命中测试（逻辑坐标）。
    fn hit_test(&self, p: Point) -> bool;

    /// 绘制（主层）。
    fn paint(&mut self, ctx: &mut PaintCtx);

    /// 绘制覆盖层（弹出/遮罩），在所有控件主层绘制完成后统一再画一遍，
    /// 用于 ComboBox 下拉、ToolTip、ContentDialog 等需置顶的内容。默认无。
    fn paint_overlay(&mut self, _ctx: &mut PaintCtx) {}

    /// 处理输入事件。`now` 为单调时间（秒），供控件记录动画起点。
    fn on_event(&mut self, _ev: InputEvent, _now: f64) -> EventResult {
        EventResult::NONE
    }

    /// 是否有动画正在进行（决定是否继续高频重绘）。
    fn is_animating(&self, _now: f64) -> bool {
        false
    }

    /// 是否正处于模态状态（如打开的 ComboBox 下拉 / ContentDialog）：
    /// 为 true 时宿主只把事件派发给本控件，实现焦点捕获。
    fn wants_modal(&self) -> bool {
        false
    }

    /// 是否需要独占键盘/字符输入（如聚焦的 TextBox）：
    /// 为 true 时宿主只把 KeyDown/Char 事件派发给本控件，避免空格等键误触其它控件。
    fn wants_keyboard(&self) -> bool {
        false
    }

    /// 指针位于 `p`（逻辑坐标）时期望的光标形状。默认箭头；TextBox 在其矩形内返回 Text，
    /// 容器把查询转发给命中的子控件。宿主在 WM_SETCURSOR 调用以决定系统光标。
    fn cursor_at(&self, _p: Point) -> Cursor {
        Cursor::Default
    }

    /// 当前文本插入点（caret）的逻辑坐标——聚焦的 TextBox 返回 Some(光标处)，否则 None。
    /// 宿主在 WM_IME_STARTCOMPOSITION 用它把输入法候选窗定位到光标处。
    fn caret_pos(&self) -> Option<Point> {
        None
    }

    /// 设置 IME **内联组字串**（未上屏的拼音），由聚焦 TextBox 自绘在框内（带下划线），
    /// 不再用系统组字浮窗（那是一块白底，盖在框上很丑）。空串=清除。容器转发给聚焦子控件。
    fn set_composition(&mut self, _s: &str) {}

    // —— UIA 预留 —— //
    fn accessible_role(&self) -> AccessibleRole;
    fn accessible_name(&self) -> String {
        String::new()
    }
}
