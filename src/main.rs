// 隐藏控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod downloader;
mod network;
mod plugins;
mod ui;
mod utils;
mod loading;
mod mode;
mod source_selector;

use eframe::egui;
use std::env;
use mode::PluginMode;

#[cfg(target_os = "windows")]
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

// 检测是否在 PE 环境
fn is_pe_environment() -> bool {
    // PE 环境通常有这些特征
    std::env::var("X:").is_ok() || 
    std::env::var("WINPE").is_ok() ||
    std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.starts_with("X:")))
        .unwrap_or(false)
}

fn show_error_message(title: &str, message: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::ptr;
        use winapi::um::winuser::{MessageBoxW, MB_OK, MB_ICONERROR};

        let title_wide: Vec<u16> = OsStr::new(title)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let message_wide: Vec<u16> = OsStr::new(message)
            .encode_wide()
            .chain(Some(0))
            .collect();

        unsafe {
            MessageBoxW(
                ptr::null_mut(),
                message_wide.as_ptr(),
                title_wide.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

fn main() -> eframe::Result<()> {
    // 检测 PE 环境
    let in_pe = is_pe_environment();
    
    // 在 PE 环境中跳过管理员权限检查
    #[cfg(target_os = "windows")]
    {
        if !in_pe {
            request_admin();
        }
    }
    
    // 解析命令行参数
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
    
    // 初始化运行时
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            let error_msg = format!("无法创建 Tokio 运行时: {}", e);
            show_error_message("启动失败", &error_msg);
            std::process::exit(1);
        }
    };
    
    // 设置图标
    let icon_bytes = include_bytes!("../assets/icon.png");
    let icon = match eframe::icon_data::from_png_bytes(icon_bytes) {
        Ok(icon) => icon,
        Err(_e) => egui::IconData::default()
    };
    
    // 根据模式设置窗口标题
    let window_title = match mode {
        PluginMode::CloudPE => "Cloud-PE 插件市场",
        PluginMode::HotPE => "HotPE 模块下载",
        PluginMode::Edgeless => "Edgeless 插件下载",
        PluginMode::Select => "选择插件源",
    };
    
    // 根据模式设置窗口大小
    let window_size = if mode == PluginMode::Select {
        [400.0, 300.0]
    } else {
        [1024.0, 630.0]
    };
    
    // 配置窗口选项
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(window_size)
            .with_min_inner_size(if mode == PluginMode::Select { [400.0, 300.0] } else { [800.0, 600.0] })
            .with_icon(icon)
            .with_resizable(mode != PluginMode::Select),
        centered: true,
        ..Default::default()
    };
    
    // 启动应用
    let result = eframe::run_native(
        window_title,
        native_options,
        Box::new(move |cc| {
            // 加载自定义字体
            setup_custom_fonts(&cc.egui_ctx);
            
            if mode == PluginMode::Select {
                Ok(Box::new(source_selector::SourceSelector::new(cc)))
            } else {
                Ok(Box::new(loading::LoadingScreen::new(cc, rt, mode)))
            }
        }),
    );
    
    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            let error_msg = format!("应用启动失败: {}", e);
            
            // 显示用户友好的错误信息
            let user_msg = if error_msg.contains("OpenGL") || error_msg.contains("GL") {
                "OpenGL 初始化失败！\n\n可能的原因：\n\
                 1. WinPE 缺少 OpenGL 支持\n\
                 2. 显卡驱动未安装\n\
                 3. 虚拟机未启用 3D 加速\n\n\
                 解决方案：\n\
                 - 创建 WinPE 时勾选 OpenGL 支持\n\
                 - 安装显卡驱动到 PE\n\
                 - 在虚拟机中启用 3D 加速"
            } else {
                &error_msg
            };
            
            show_error_message("启动失败", user_msg);
            
            Err(e)
        }
    }
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // 尝试加载Windows系统微软雅黑字体
    let font_loaded = load_microsoft_yahei_font(&mut fonts);
    
    if font_loaded {
        // 设置微软雅黑为主要字体
        fonts.families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "microsoft_yahei".to_owned());
        
        fonts.families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "microsoft_yahei".to_owned());
    }
    
    ctx.set_fonts(fonts);
}

fn load_microsoft_yahei_font(fonts: &mut egui::FontDefinitions) -> bool {
    #[cfg(target_os = "windows")]
    {
        // 获取Windows字体目录
        let font_paths = get_windows_font_paths();
        
        // 尝试加载微软雅黑字体文件
        for font_path in font_paths {
            if let Ok(font_data) = std::fs::read(&font_path) {
                // 成功读取字体文件
                fonts.font_data.insert(
                    "microsoft_yahei".to_owned(),
                    egui::FontData::from_owned(font_data)
                );
                return true;
            }
        }
        
        // 如果所有路径都失败，返回false
        false
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        // 非Windows系统，不加载微软雅黑
        false
    }
}

#[cfg(target_os = "windows")]
fn get_windows_font_paths() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    
    let mut paths = Vec::new();
    
    // 方法1: 从环境变量获取Windows目录
    if let Ok(windir) = std::env::var("WINDIR") {
        paths.push(PathBuf::from(&windir).join("Fonts").join("msyh.ttc"));
        paths.push(PathBuf::from(&windir).join("Fonts").join("msyh.ttf"));
    }
    
    // 方法2: 从SystemRoot环境变量获取
    if let Ok(systemroot) = std::env::var("SystemRoot") {
        paths.push(PathBuf::from(&systemroot).join("Fonts").join("msyh.ttc"));
        paths.push(PathBuf::from(&systemroot).join("Fonts").join("msyh.ttf"));
    }
    
    // 方法3: 使用默认路径（适用于大多数Windows系统）
    paths.push(PathBuf::from("C:\\Windows\\Fonts\\msyh.ttc"));
    paths.push(PathBuf::from("C:\\Windows\\Fonts\\msyh.ttf"));
    
    // 方法4: 使用注册表获取字体目录
    if let Some(fonts_dir) = get_fonts_dir_from_registry() {
        paths.push(fonts_dir.join("msyh.ttc"));
        paths.push(fonts_dir.join("msyh.ttf"));
    }
    
    paths
}

#[cfg(target_os = "windows")]
fn get_fonts_dir_from_registry() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    use winapi::um::winreg::{RegOpenKeyExW, RegQueryValueExW, RegCloseKey, HKEY_LOCAL_MACHINE};
    use winapi::um::winnt::{KEY_READ, REG_SZ};
    use winapi::shared::minwindef::HKEY;
    use std::ptr;
    
    unsafe {
        let subkey = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts\0"
            .encode_utf16()
            .collect::<Vec<u16>>();
        
        let mut hkey: HKEY = ptr::null_mut();
        let result = RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        );
        
        if result != 0 {
            return None;
        }
        
        let value_name = "微软雅黑 & Microsoft YaHei UI (TrueType)\0"
            .encode_utf16()
            .collect::<Vec<u16>>();
        
        let mut buffer: [u16; 260] = [0; 260];
        let mut buffer_size: u32 = (buffer.len() * 2) as u32;
        let mut value_type: u32 = 0;
        
        let result = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            ptr::null_mut(),
            &mut value_type,
            buffer.as_mut_ptr() as *mut u8,
            &mut buffer_size,
        );
        
        RegCloseKey(hkey);
        
        if result == 0 && value_type == REG_SZ {
            let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            let font_file = String::from_utf16_lossy(&buffer[..len]);
            
            // 如果是相对路径，需要加上Windows\Fonts目录
            if !font_file.contains(':') && !font_file.starts_with('\\') {
                if let Ok(windir) = std::env::var("WINDIR") {
                    return Some(PathBuf::from(windir).join("Fonts").join(font_file));
                }
            }
            
            Some(PathBuf::from(font_file))
        } else {
            None
        }
    }
}
