//! 主题：深/浅切换，启动跟随系统（注册表 `AppsUseLightTheme`），
//! 运行时由 gallery 监听 `WM_SETTINGCHANGE` 调用 [`Theme::system`] 重新检测。

use crate::tokens::Tokens;
use windows::core::w;
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    /// 当前主题对应的全套 token 真值。
    pub fn tokens(self) -> Tokens {
        match self {
            Theme::Dark => Tokens::dark(),
            Theme::Light => Tokens::light(),
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(self, Theme::Dark)
    }

    pub fn toggled(self) -> Theme {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }

    /// 读取系统应用主题：HKCU\...\Themes\Personalize : AppsUseLightTheme (DWORD)。
    /// 1 = 浅色，0 = 深色；读不到时默认深色（gallery 主目标）。
    pub fn system() -> Theme {
        let mut value: u32 = 0;
        let mut size: u32 = std::mem::size_of::<u32>() as u32;
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                w!("AppsUseLightTheme"),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut value as *mut u32 as *mut _),
                Some(&mut size),
            )
        };
        if result.is_ok() && value == 1 {
            Theme::Light
        } else {
            Theme::Dark
        }
    }
}
