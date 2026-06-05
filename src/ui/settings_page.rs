use crate::config::{AppConfig, ColorMode};
use crate::utils::BootDriveManager;
use crate::mode::PluginMode;
use eframe::egui;
use std::sync::Arc;
use parking_lot::RwLock;

#[cfg(target_os = "windows")]
use winapi::um::dwmapi::DwmSetWindowAttribute;
#[cfg(target_os = "windows")]
use winapi::um::winuser::GetActiveWindow;
#[cfg(target_os = "windows")]
use std::mem;

pub struct SettingsPage {
    config: Arc<RwLock<AppConfig>>,
    boot_drive_manager: Arc<RwLock<BootDriveManager>>,
    mode: PluginMode,
}

impl SettingsPage {
    pub fn new(
        config: Arc<RwLock<AppConfig>>,
        boot_drive_manager: Arc<RwLock<BootDriveManager>>,
        mode: PluginMode,
    ) -> Self {
        Self {
            config,
            boot_drive_manager,
            mode,
        }
    }
    
    pub fn show(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.heading("设置");
        ui.separator();
        
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.collapsing("基本设置", |ui| {
                self.show_basic_settings(ui);
            });
            
            ui.collapsing("启动盘设置", |ui| {
                self.show_boot_drive_settings(ui);
            });
            
            ui.collapsing("下载设置", |ui| {
                self.show_download_settings(ui);
            });
            
            ui.collapsing("关于", |ui| {
                self.show_about(ui);
            });
        });
    }
    
    fn show_basic_settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("颜色模式：");
            
            let mut config = self.config.write();
            let mut current_mode = config.color_mode.clone();
            
            egui::ComboBox::from_label("")
                .selected_text(match &current_mode {
                    ColorMode::System => "跟随系统",
                    ColorMode::Light => "浅色模式",
                    ColorMode::Dark => "深色模式",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut current_mode, ColorMode::System, "跟随系统（默认）");
                    ui.selectable_value(&mut current_mode, ColorMode::Light, "浅色模式");
                    ui.selectable_value(&mut current_mode, ColorMode::Dark, "深色模式");
                });
            
            if current_mode != config.color_mode {
                config.color_mode = current_mode.clone();
                let _ = config.save();
                
                // 更新窗口标题栏主题
                #[cfg(target_os = "windows")]
                unsafe {
                    set_dwm_theme(&current_mode);
                }
            }
        });
    }
    
    fn show_boot_drive_settings(&mut self, ui: &mut egui::Ui) {
        let boot_drives = self.boot_drive_manager.read().get_all_drives();
        
        if boot_drives.is_empty() {
            ui.label("未检测到启动盘");
            ui.add_space(10.0);
            if ui.button("刷新启动盘").clicked() {
                self.boot_drive_manager.write().reload();
            }
        } else {
            ui.horizontal(|ui| {
                ui.label("当前启动盘：");
                
                let current_drive = self.boot_drive_manager.read().get_current_drive();
                let mut selected_drive = current_drive.clone().unwrap_or_default();
                
                egui::ComboBox::from_label("")
                    .selected_text(&selected_drive)
                    .show_ui(ui, |ui| {
                        for drive in &boot_drives {
                            // 只显示盘符，不显示版本
                            ui.selectable_value(
                                &mut selected_drive,
                                drive.letter.clone(),
                                &drive.letter,
                            );
                        }
                    });
                
                if Some(&selected_drive) != current_drive.as_ref() && !selected_drive.is_empty() {
                    self.boot_drive_manager.write().set_current_drive(selected_drive.clone());
                    
                    let mut config = self.config.write();
                    config.default_boot_drive = Some(selected_drive);
                    let _ = config.save();
                }
            });
            
            if ui.button("重新扫描启动盘").clicked() {
                self.boot_drive_manager.write().reload();
            }
        }
    }
    
    fn show_download_settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("下载线程数：");
            
            let mut config = self.config.write();
            let mut threads = config.download_threads;
            
            egui::ComboBox::from_label("")
                .selected_text(format!("{} 线程", threads))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut threads, 8, "8 线程");
                    ui.selectable_value(&mut threads, 16, "16 线程");
                    ui.selectable_value(&mut threads, 32, "32 线程（最大）");
                });
            
            if threads != config.download_threads {
                config.download_threads = threads;
                let _ = config.save();
            }
        });
        
        ui.horizontal(|ui| {
            ui.label("默认下载路径：");
            
            let config = self.config.read();
            if let Some(path) = &config.default_download_path {
                ui.label(path.display().to_string());
            } else {
                ui.label("未设置");
            }
            
            if ui.button("浏览").clicked() {
                use rfd::FileDialog;
                
                if let Some(path) = FileDialog::new()
                    .set_title("选择默认下载路径")
                    .pick_folder()
                {
                    drop(config);
                    let mut config = self.config.write();
                    config.default_download_path = Some(path);
                    let _ = config.save();
                }
            }
        });
    }
    
    fn show_about(&mut self, ui: &mut egui::Ui) {
        let title = match self.mode {
            PluginMode::CloudPE => "Cloud-PE 插件市场",
            PluginMode::HotPE => "HotPE 模块下载",
            PluginMode::Edgeless => "Edgeless 插件下载",
            _ => "",
        };
        
        ui.label(egui::RichText::new(title).strong());
        ui.label("版本：v0.1");
        ui.label("作者：NORMAL-EX（别称：dddffgg）");
        ui.label("版权：© 2025-present Cloud-PE Dev.");
        
        ui.separator();
        
        match self.mode {
            PluginMode::CloudPE => {
                ui.label("此软件是 Cloud-PE One 的独立功能模块");
                ui.label("专用于管理和下载 Cloud-PE 插件");
            }
            PluginMode::HotPE => {
                ui.label("此软件是 HotPE 模块下载管理工具");
                ui.label("专用于管理和下载 HotPE 模块");
            }
            PluginMode::Edgeless => {
                ui.label("此软件是 Edgeless 插件下载管理工具");
                ui.label("专用于管理和下载 Edgeless 插件");
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn set_dwm_theme(mode: &ColorMode) {
    let hwnd = GetActiveWindow();
    if !hwnd.is_null() {
        let dark_mode = match mode {
            ColorMode::System => {
                // 跟随系统模式
                if dark_light::detect() == dark_light::Mode::Dark { 1i32 } else { 0i32 }
            }
            ColorMode::Light => 0i32,
            ColorMode::Dark => 1i32,
        };
        
        DwmSetWindowAttribute(
            hwnd as _,
            20, // DWMWA_USE_IMMERSIVE_DARK_MODE
            &dark_mode as *const _ as *mut _,
            mem::size_of::<i32>() as u32,
        );
    }
}