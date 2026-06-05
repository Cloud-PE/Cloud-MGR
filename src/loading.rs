use crate::app::CloudPEApp;
use crate::mode::PluginMode;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Instant;
use tokio::runtime::Runtime;

pub struct LoadingScreen {
    is_loading: Arc<AtomicBool>,
    network_check_status: Arc<AtomicU8>, // 0=checking, 1=success, 2=failed
    _network_error_message: Option<String>,
    _start_time: Instant,
    _runtime: Arc<Runtime>,
    app: Option<Box<CloudPEApp>>,
    _init_complete: bool,
    mode: PluginMode,
}

impl LoadingScreen {
    pub fn new(cc: &eframe::CreationContext<'_>, runtime: Runtime, mode: PluginMode) -> Self {
        let runtime = Arc::new(runtime);
        let is_loading = Arc::new(AtomicBool::new(true));
        let network_check_status = Arc::new(AtomicU8::new(0));
        
        let is_loading_clone = is_loading.clone();
        let network_status_clone = network_check_status.clone();
        let runtime_clone = runtime.clone();
        let mode_clone = mode.clone();
        
        // 网络检测
        runtime_clone.spawn(async move {
            let mut retry_count = 0;
            let max_retries = 3;
            let mut success = false;
            
            let url = mode_clone.get_connect_test_url();
            
            while retry_count < max_retries {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(5))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new());
                
                match client.get(url).send().await {
                    Ok(response) => {
                        if let Ok(text) = response.text().await {
                            if !text.is_empty() {
                                success = true;
                                break;
                            }
                        }
                    }
                    Err(_) => {}
                }
                
                retry_count += 1;
                if retry_count < max_retries {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
            
            if success {
                network_status_clone.store(1, Ordering::Relaxed);
                // 网络连接成功，等待一会儿显示加载动画
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            } else {
                network_status_clone.store(2, Ordering::Relaxed);
            }
            
            is_loading_clone.store(false, Ordering::Relaxed);
        });
        
        // 初始化应用（在后台）
        let app = CloudPEApp::new(cc, runtime.clone(), mode);
        
        Self {
            is_loading,
            network_check_status,
            _network_error_message: None,
            _start_time: Instant::now(),
            _runtime: runtime,
            app: Some(Box::new(app)),
            _init_complete: false,
            mode,
        }
    }
}

impl eframe::App for LoadingScreen {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let network_status = self.network_check_status.load(Ordering::Relaxed);
        
        if network_status == 2 {
            // 网络连接失败
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let available_height = ui.available_height();
                    ui.add_space(available_height * 0.3);
                    
                    let title = self.mode.get_server_name();
                    ui.heading(egui::RichText::new(title).size(48.0).strong());
                    
                    ui.add_space(40.0);
                    
                    let error_msg = format!("无法连接至 {} 服务器，请检查网络连接或联系开发人员", self.mode.get_server_name());
                    ui.label(egui::RichText::new(error_msg)
                        .color(egui::Color32::from_rgb(255, 100, 100)));
                    
                    ui.add_space(20.0);
                    
                    if ui.button("关闭").clicked() {
                        std::process::exit(0);
                    }
                });
            });
        } else if self.is_loading.load(Ordering::Relaxed) {
            // 显示加载界面
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let available_height = ui.available_height();
                    ui.add_space(available_height * 0.3);
                    
                    // 显示对应模式的文字
                    let title = self.mode.get_server_name();
                    ui.heading(egui::RichText::new(title).size(48.0).strong());
                    
                    ui.add_space(40.0);
                    
                    // 加载动画圈
                    ui.spinner();
                    
                    ui.add_space(20.0);
                    ui.label("正在加载...");
                });
            });
            
            // 持续刷新
            ctx.request_repaint();
        } else {
            // 加载完成，运行主应用
            if let Some(app) = &mut self.app {
                app.update(ctx, frame);
            }
        }
    }
}