use eframe::egui;
use crate::mode::PluginMode;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use std::process::Command;

#[derive(Clone)]
struct SourceStatus {
    available: Option<bool>,
    checking: bool,
}

pub struct SourceSelector {
    sources: Arc<RwLock<HashMap<PluginMode, SourceStatus>>>,
    is_checking: bool,
    runtime: tokio::runtime::Runtime,
}

impl SourceSelector {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut sources = HashMap::new();
        sources.insert(PluginMode::CloudPE, SourceStatus { available: None, checking: false });
        sources.insert(PluginMode::HotPE, SourceStatus { available: None, checking: false });
        sources.insert(PluginMode::Edgeless, SourceStatus { available: None, checking: false });
        
        Self {
            sources: Arc::new(RwLock::new(sources)),
            is_checking: false,
            runtime: tokio::runtime::Runtime::new().unwrap(),
        }
    }
    
    fn launch_mode(&self, mode: PluginMode) {
        let exe = std::env::current_exe().unwrap();
        let arg = match mode {
            PluginMode::CloudPE => "",
            PluginMode::HotPE => "--hpm",
            PluginMode::Edgeless => "--edgeless",
            _ => return,
        };
        
        if arg.is_empty() {
            Command::new(exe).spawn().ok();
        } else {
            Command::new(exe).arg(arg).spawn().ok();
        }
        
        std::process::exit(0);
    }
    
    fn check_availability(&mut self) {
        if self.is_checking {
            return;
        }
        
        self.is_checking = true;
        
        // 重置状态
        {
            let mut sources = self.sources.write();
            for (_, status) in sources.iter_mut() {
                status.checking = true;
                status.available = None;
            }
        }
        
        // 检查Cloud-PE
        let sources_clone = self.sources.clone();
        self.runtime.spawn(async move {
            let available = check_source_async(PluginMode::CloudPE).await;
            let mut sources = sources_clone.write();
            if let Some(status) = sources.get_mut(&PluginMode::CloudPE) {
                status.available = Some(available);
                status.checking = false;
            }
        });
        
        // 检查HotPE
        let sources_clone = self.sources.clone();
        self.runtime.spawn(async move {
            let available = check_source_async(PluginMode::HotPE).await;
            let mut sources = sources_clone.write();
            if let Some(status) = sources.get_mut(&PluginMode::HotPE) {
                status.available = Some(available);
                status.checking = false;
            }
        });
        
        // 检查Edgeless
        let sources_clone = self.sources.clone();
        self.runtime.spawn(async move {
            let available = check_source_async(PluginMode::Edgeless).await;
            let mut sources = sources_clone.write();
            if let Some(status) = sources.get_mut(&PluginMode::Edgeless) {
                status.available = Some(available);
                status.checking = false;
            }
        });
    }
}

async fn check_source_async(mode: PluginMode) -> bool {
    let url = mode.get_connect_test_url();
    if url.is_empty() {
        return false;
    }
    
    let mut retry_count = 0;
    let max_retries = 3;
    
    while retry_count < max_retries {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        
        match client.get(url).send().await {
            Ok(response) => {
                if let Ok(text) = response.text().await {
                    if !text.is_empty() {
                        return true;
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
    
    false
}

impl eframe::App for SourceSelector {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading("选择插件源");
                ui.separator();
                ui.add_space(20.0);
                
                let button_enabled = !self.is_checking;
                
                // 创建按钮的函数，包含状态图标
                let create_button = |name: &str, status: Option<bool>| -> String {
                    match status {
                        Some(true) => format!("✓  {}", name),
                        Some(false) => format!("✗  {}", name),
                        None => name.to_string(),
                    }
                };
                
                // Cloud-PE按钮
                {
                    let sources = self.sources.read();
                    let status = sources.get(&PluginMode::CloudPE)
                        .and_then(|s| s.available);
                    let button_text = create_button("Cloud-PE", status);
                    
                    if ui.add_enabled(
                        button_enabled, 
                        egui::Button::new(button_text)
                            .min_size(egui::Vec2::new(200.0, 40.0))
                    ).clicked() {
                        self.launch_mode(PluginMode::CloudPE);
                    }
                }
                
                ui.add_space(10.0);
                
                // HotPE按钮
                {
                    let sources = self.sources.read();
                    let status = sources.get(&PluginMode::HotPE)
                        .and_then(|s| s.available);
                    let button_text = create_button("HotPE", status);
                    
                    if ui.add_enabled(
                        button_enabled,
                        egui::Button::new(button_text)
                            .min_size(egui::Vec2::new(200.0, 40.0))
                    ).clicked() {
                        self.launch_mode(PluginMode::HotPE);
                    }
                }
                
                ui.add_space(10.0);
                
                // Edgeless按钮
                {
                    let sources = self.sources.read();
                    let status = sources.get(&PluginMode::Edgeless)
                        .and_then(|s| s.available);
                    let button_text = create_button("Edgeless", status);
                    
                    if ui.add_enabled(
                        button_enabled,
                        egui::Button::new(button_text)
                            .min_size(egui::Vec2::new(200.0, 40.0))
                    ).clicked() {
                        self.launch_mode(PluginMode::Edgeless);
                    }
                }
                
                ui.add_space(20.0);
                ui.separator();
                ui.add_space(10.0);
                
                // 检测可用性按钮
                if ui.add_enabled(!self.is_checking, egui::Button::new(if self.is_checking { "检测中..." } else { "检测可用性" }))
                    .clicked() {
                    self.check_availability();
                }
                
                // 检查是否所有检测都完成
                let all_done = {
                    let sources = self.sources.read();
                    sources.values().all(|s| !s.checking)
                };
                
                if self.is_checking && all_done {
                    self.is_checking = false;
                }
            });
        });
        
        // 持续刷新以更新检测状态
        if self.is_checking {
            ctx.request_repaint();
        }
    }
}