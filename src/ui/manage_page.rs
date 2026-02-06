use crate::plugins::{Plugin, PluginManager};
use crate::utils::BootDriveManager;
use crate::mode::PluginMode;
use crate::downloader::Downloader;
use crate::config::AppConfig;
use eframe::egui;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use tokio::runtime::Runtime;
use std::time::{Duration, Instant};

#[derive(Clone)]
#[allow(dead_code)]
struct UpdateTask {
    plugin_name: String,
    progress: Arc<RwLock<f32>>,
}

pub struct PluginsManagePage {
    plugin_manager: Arc<RwLock<PluginManager>>,
    boot_drive_manager: Arc<RwLock<BootDriveManager>>,
    mode: PluginMode,
    updating_tasks: Arc<RwLock<HashMap<String, UpdateTask>>>,
    runtime: Arc<Runtime>,
    config: Arc<RwLock<AppConfig>>,
    last_refresh: Option<Instant>,
    need_refresh: bool,
}

impl PluginsManagePage {
    pub fn new(
        plugin_manager: Arc<RwLock<PluginManager>>,
        boot_drive_manager: Arc<RwLock<BootDriveManager>>,
        mode: PluginMode,
        runtime: Arc<Runtime>,
        config: Arc<RwLock<AppConfig>>,
    ) -> Self {
        Self {
            plugin_manager,
            boot_drive_manager,
            mode,
            updating_tasks: Arc::new(RwLock::new(HashMap::new())),
            runtime,
            config,
            last_refresh: None,
            need_refresh: true,
        }
    }
    
    pub fn show(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading(self.mode.get_plugin_manage_name());
        ui.separator();
        
        let current_drive = self.boot_drive_manager.read().get_current_drive();
        
        if let Some(drive) = current_drive {
            let has_updating_tasks = !self.updating_tasks.read().is_empty();
            
            let should_refresh = if has_updating_tasks {
                false
            } else if self.need_refresh {
                true
            } else if let Some(last) = self.last_refresh {
                last.elapsed() > Duration::from_secs(2)
            } else {
                true
            };
            
            if should_refresh {
                let _ = self.plugin_manager.write().load_local_plugins(&drive);
                self.last_refresh = Some(Instant::now());
                self.need_refresh = false;
            }
            
            let enabled_label = match self.mode {
                PluginMode::HotPE => "已启用模块",
                _ => "已启用插件",
            };
            
            let disabled_label = match self.mode {
                PluginMode::HotPE => "已禁用模块",
                _ => "已禁用插件",
            };
            
            egui::ScrollArea::vertical()
                .id_salt("manage_scroll")
                .show(ui, |ui| {
                    ui.collapsing(enabled_label, |ui| {
                        let enabled_plugins = self.plugin_manager.read().get_enabled_plugins().clone();
                        
                        if enabled_plugins.is_empty() {
                            let empty_text = match self.mode {
                                PluginMode::HotPE => "暂无已启用的模块",
                                _ => "暂无已启用的插件",
                            };
                            ui.label(empty_text);
                        } else {
                            for plugin in enabled_plugins {
                                self.show_plugin_item(ui, &plugin, true, &drive);
                            }
                        }
                    });
                    
                    ui.collapsing(disabled_label, |ui| {
                        let disabled_plugins = self.plugin_manager.read().get_disabled_plugins().clone();
                        
                        if disabled_plugins.is_empty() {
                            let empty_text = match self.mode {
                                PluginMode::HotPE => "暂无已禁用的模块",
                                _ => "暂无已禁用的插件",
                            };
                            ui.label(empty_text);
                        } else {
                            for plugin in disabled_plugins {
                                self.show_plugin_item(ui, &plugin, false, &drive);
                            }
                        }
                    });
                });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("请先选择或安装启动盘");
            });
        }
        
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    
    fn show_plugin_item(&mut self, ui: &mut egui::Ui, plugin: &Plugin, is_enabled: bool, drive: &str) {
        let plugin_id = plugin.get_plugin_id();
        let update_task_id = format!("{}_update", plugin_id);
        
        let tasks = self.updating_tasks.read();
        let is_updating = tasks.contains_key(&update_task_id);
        drop(tasks);
        
        egui::Frame::default()
            .fill(ui.style().visuals.window_fill())
            .stroke(ui.style().visuals.widgets.noninteractive.bg_stroke)
            .inner_margin(10.0)
            .outer_margin(5.0)
            .rounding(5.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&plugin.name).strong());
                        
                        if self.mode != PluginMode::Edgeless && !plugin.describe.is_empty() {
                            ui.label(&plugin.describe);
                        }
                        
                        ui.horizontal(|ui| {
                            ui.label(format!("版本: {}", plugin.version));
                            ui.label(format!("大小: {}", plugin.size));
                            ui.label(format!("作者: {}", plugin.author));
                        });
                    });
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if is_enabled {
                            if !is_updating {
                                if ui.button("禁用").clicked() {
                                    let _ = self.plugin_manager.write()
                                        .disable_plugin(drive, &plugin.file);
                                    self.need_refresh = true;
                                }
                            }
                            
                            if self.check_update_available(plugin) {
                                if is_updating {
                                    ui.add_enabled(false, egui::Button::new("更新中..."));
                                    ui.spinner();
                                } else {
                                    if ui.button("更新").clicked() {
                                        self.update_plugin(plugin.clone(), drive);
                                    }
                                }
                            }
                        } else {
                            if ui.button("启用").clicked() {
                                let _ = self.plugin_manager.write()
                                    .enable_plugin(drive, &plugin.file);
                                self.need_refresh = true;
                            }
                        }
                    });
                });
            });
    }
    
    fn check_update_available(&self, local_plugin: &Plugin) -> bool {
        let plugin_id = local_plugin.get_plugin_id();
        let manager = self.plugin_manager.read();
        
        if let Some(market_plugin) = manager.find_market_plugin_by_id(&plugin_id) {
            let comparison = manager.compare_versions(&local_plugin.version, &market_plugin.version);
            matches!(comparison, std::cmp::Ordering::Less)
        } else {
            false
        }
    }
    
    fn update_plugin(&mut self, local_plugin: Plugin, drive: &str) {
        let plugin_id = local_plugin.get_plugin_id();
        let update_task_id = format!("{}_update", plugin_id);
        
        let task = UpdateTask {
            plugin_name: local_plugin.name.clone(),
            progress: Arc::new(RwLock::new(0.0)),
        };
        
        self.updating_tasks.write().insert(update_task_id.clone(), task.clone());
        
        let plugin_manager = self.plugin_manager.clone();
        
        let market_plugin = match plugin_manager.read().find_market_plugin_by_id(&plugin_id) {
            Some(p) => p,
            None => {
                self.updating_tasks.write().remove(&update_task_id);
                return;
            }
        };
        
        let downloader = Arc::new(Downloader::new(self.config.read().download_threads));
        let drive_letter = drive.to_string();
        let updating_tasks = self.updating_tasks.clone();
        let mode = self.mode.clone();
        
        let plugin_url = market_plugin.link.clone();
        let filename = self.generate_plugin_filename(&market_plugin);
        let old_file = local_plugin.file.clone();
        
        self.runtime.spawn(async move {
            let plugin_dir = format!("{}\\{}", drive_letter, mode.get_plugin_folder());
            
            if let Err(_) = tokio::fs::create_dir_all(&plugin_dir).await {
                updating_tasks.write().remove(&update_task_id);
                return;
            }
            
            if let Err(_) = plugin_manager.read().delete_plugin_file(&drive_letter, &old_file) {
                updating_tasks.write().remove(&update_task_id);
                return;
            }
            
            let extension = mode.get_enabled_extension();
            let install_path = std::path::PathBuf::from(plugin_dir).join(format!("{}.{}", filename, extension));
            
            match downloader.download(&plugin_url, install_path.clone()).await {
                Ok(_) => {
                    let _ = plugin_manager.write().load_local_plugins(&drive_letter);
                }
                Err(_) => {
                }
            }
            
            updating_tasks.write().remove(&update_task_id);
        });
    }
    
    fn generate_plugin_filename(&self, plugin: &Plugin) -> String {
        let safe_describe = plugin.describe
            .replace(' ', "_")
            .replace('/', "_")
            .replace('\\', "_")
            .replace(':', "_")
            .replace('*', "_")
            .replace('?', "_")
            .replace('"', "_")
            .replace('<', "_")
            .replace('>', "_")
            .replace('|', "_");
        
        match self.mode {
            PluginMode::CloudPE => {
                format!("{}_{}_{}_{}", plugin.name, plugin.version, plugin.author, safe_describe)
            }
            PluginMode::HotPE => {
                if safe_describe.is_empty() {
                    format!("{}_{}_{}_{}", plugin.name, plugin.author, plugin.version, plugin.name)
                } else {
                    format!("{}_{}_{}_{}", plugin.name, plugin.author, plugin.version, safe_describe)
                }
            }
            PluginMode::Edgeless => {
                format!("{}_{}_{}", plugin.name, plugin.version, plugin.author)
            }
            _ => String::new()
        }
    }
}
