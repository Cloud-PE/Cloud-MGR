// 隐藏控制台窗口（release）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod downloader;
mod fluent_host;
mod mode;
mod network;
mod plugins;
mod ui;
mod utils;

use config::{AppConfig, ColorMode};
use fluent_host::WindowOptions;
use mode::PluginMode;
use std::env;
use std::sync::Arc;
use parking_lot::RwLock;
use fluentpx::Theme;

#[cfg(all(target_os = "windows", not(cm_noadmin)))]
fn request_admin() -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use winapi::um::processthreadsapi::GetCurrentProcess;
    use winapi::um::processthreadsapi::OpenProcessToken;
    use winapi::um::securitybaseapi::GetTokenInformation;
    use winapi::um::winnt::{TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
    use winapi::um::handleapi::CloseHandle;
    use std::ptr;
    use std::mem;

    unsafe {
        let mut is_elevated = false;
        let process = GetCurrentProcess();
        let mut token = ptr::null_mut();

        if OpenProcessToken(process, TOKEN_QUERY, &mut token) != 0 {
            let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut size = 0;

            if GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut _ as *mut _,
                mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut size,
            ) != 0 {
                is_elevated = elevation.TokenIsElevated != 0;
            }

            CloseHandle(token);
        }

        if !is_elevated {
            let exe = env::current_exe().unwrap();
            let args: Vec<String> = env::args().skip(1).collect();

            let result = Command::new("cmd")
                .arg("/c")
                .arg("start")
                .raw_arg(format!("runas /user:Administrator \"{}\" {}", exe.display(), args.join(" ")))
                .spawn();

            if result.is_ok() {
                std::process::exit(0);
            }
        }

        is_elevated
    }
}

/// 检测是否在 PE 环境
fn is_pe_environment() -> bool {
    std::env::var("X:").is_ok()
        || std::env::var("WINPE").is_ok()
        || std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.starts_with("X:")))
            .unwrap_or(false)
}

pub fn show_error_message(title: &str, message: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::ptr;
        use winapi::um::winuser::{MessageBoxW, MB_OK, MB_ICONERROR};

        let title_wide: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();
        let message_wide: Vec<u16> = OsStr::new(message).encode_wide().chain(Some(0)).collect();

        unsafe {
            MessageBoxW(ptr::null_mut(), message_wide.as_ptr(), title_wide.as_ptr(), MB_OK | MB_ICONERROR);
        }
    }
}

/// 把配置里的 ColorMode 解析为 fluentpx 的 Theme（供宿主每帧调用）。
pub fn resolve_theme(config: &Arc<RwLock<AppConfig>>) -> Theme {
    match config.read().color_mode {
        ColorMode::System => Theme::system(),
        ColorMode::Light => Theme::Light,
        ColorMode::Dark => Theme::Dark,
    }
}

fn main() -> windows::core::Result<()> {
    let argv: Vec<String> = env::args().collect();

    // 开发期离屏截图：`--shot <png> [page 0..2] [light]`，不提权、不联网。
    if let Some(pos) = argv.iter().position(|a| a == "--shot") {
        let path = argv.get(pos + 1).cloned().unwrap_or_else(|| "shot.png".into());
        let page: usize = argv.get(pos + 2).and_then(|s| s.parse().ok()).unwrap_or(0);
        let light = argv.iter().any(|a| a == "light");
        let m = if argv.iter().any(|a| a == "--hpm") {
            PluginMode::HotPE
        } else if argv.iter().any(|a| a == "--edgeless") {
            PluginMode::Edgeless
        } else {
            PluginMode::CloudPE
        };
        return ui::shot(&path, m, page, light);
    }

    let in_pe = is_pe_environment();
    let _ = in_pe; // 开发期(cm_noadmin)不自我提权；in_pe 仅用于下方提权判断

    // 开发期(CM_NOADMIN)：跳过 runas 自我提权，配合 asInvoker 清单 → 直接以当前用户运行、不弹 UAC。
    #[cfg(all(target_os = "windows", not(cm_noadmin)))]
    {
        if !in_pe {
            request_admin();
        }
    }

    let args: Vec<String> = env::args().collect();
    let mode = if args.len() > 1 {
        match args[1].as_str() {
            "--hpm" => PluginMode::HotPE,
            "--edgeless" => PluginMode::Edgeless,
            "--select" => PluginMode::Select,
            _ => PluginMode::CloudPE,
        }
    } else {
        PluginMode::CloudPE
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            show_error_message("启动失败", &format!("无法创建 Tokio 运行时: {}", e));
            std::process::exit(1);
        }
    };

    if mode == PluginMode::Select {
        // 选择插件源：固定小窗，跟随系统主题。
        let root = Box::new(ui::SelectorRoot::new(rt));
        return fluent_host::run(
            WindowOptions::fixed(mode.get_title(), 400.0, 300.0),
            root,
            Box::new(|| Theme::system()),
        );
    }

    // 主程序：先加载配置（决定主题），构造加载页（内部持有主界面）。
    let config = Arc::new(RwLock::new(AppConfig::load().unwrap_or_default()));
    let theme_config = config.clone();

    // 开发期离线直达市场页（注入样例数据），用于无网测试搜索框等交互：CM_DEV_OFFLINE=1
    if std::env::var("CM_DEV_OFFLINE").is_ok() {
        let mut shell = ui::CloudMgrRoot::new(rt, mode, config);
        shell.inject_sample();
        return fluent_host::run(
            WindowOptions::resizable(mode.get_title(), 1024.0, 630.0, 800.0, 600.0),
            Box::new(shell),
            Box::new(move || resolve_theme(&theme_config)),
        );
    }

    let root = Box::new(ui::LoadingRoot::new(rt, mode, config));
    fluent_host::run(
        WindowOptions::resizable(mode.get_title(), 1024.0, 630.0, 800.0, 600.0),
        root,
        Box::new(move || resolve_theme(&theme_config)),
    )
}
