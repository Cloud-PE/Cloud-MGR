use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use crate::mode::PluginMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootDrive {
    pub letter: String,
    pub version: String,
}

pub struct BootDriveManager {
    boot_drives: Vec<BootDrive>,
    current_drive: Option<String>,
    mode: PluginMode,
}

impl BootDriveManager {
    pub fn new(mode: PluginMode) -> Self {
        let mut manager = Self {
            boot_drives: Vec::new(),
            current_drive: None,
            mode,
        };
        manager.boot_drives = manager.scan_boot_drives();
        manager
    }
    
    pub fn scan_boot_drives(&self) -> Vec<BootDrive> {
        let mut drives = Vec::new();
        
        for letter in b'A'..=b'Z' {
            let drive_letter = format!("{}:", letter as char);
            
            match self.mode {
                PluginMode::CloudPE => {
                    let config_path = format!("{}\\cloud-pe\\config.json", drive_letter);
                    let iso_path = format!("{}\\Cloud-PE.iso", drive_letter);
                    
                    if Path::new(&config_path).exists() && Path::new(&iso_path).exists() {
                        if let Ok(version) = self.read_cloudpe_version(&drive_letter) {
                            drives.push(BootDrive {
                                letter: drive_letter,
                                version,
                            });
                        }
                    }
                }
                PluginMode::HotPE => {
                    let hotpe_module_path = format!("{}\\HotPEModule", drive_letter);
                    
                    // 先检查是否有HotPEModule文件夹
                    if Path::new(&hotpe_module_path).exists() {
                        drives.push(BootDrive {
                            letter: drive_letter.clone(),
                            version: "HotPE".to_string(),
                        });
                    } else {
                        // 如果没有，检查是否是Cloud-PE启动盘
                        let config_path = format!("{}\\cloud-pe\\config.json", drive_letter);
                        let iso_path = format!("{}\\Cloud-PE.iso", drive_letter);
                        
                        if Path::new(&config_path).exists() && Path::new(&iso_path).exists() {
                            // 是Cloud-PE启动盘，也算作HotPE启动盘
                            drives.push(BootDrive {
                                letter: drive_letter,
                                version: "Cloud-PE (HotPE兼容)".to_string(),
                            });
                        }
                    }
                }
                PluginMode::Edgeless => {
                    let edgeless_resource_path = format!("{}\\Edgeless\\Resource", drive_letter);
                    
                    // 先检查是否有Edgeless\Resource文件夹
                    if Path::new(&edgeless_resource_path).exists() {
                        drives.push(BootDrive {
                            letter: drive_letter.clone(),
                            version: "Edgeless".to_string(),
                        });
                    } else {
                        // 如果没有，检查是否是Cloud-PE启动盘
                        let config_path = format!("{}\\cloud-pe\\config.json", drive_letter);
                        let iso_path = format!("{}\\Cloud-PE.iso", drive_letter);
                        
                        if Path::new(&config_path).exists() && Path::new(&iso_path).exists() {
                            // 是Cloud-PE启动盘，也算作Edgeless启动盘
                            drives.push(BootDrive {
                                letter: drive_letter,
                                version: "Cloud-PE (Edgeless兼容)".to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        
        drives
    }
    
    fn read_cloudpe_version(&self, drive_letter: &str) -> Result<String> {
        let config_path = format!("{}\\cloud-pe\\config.json", drive_letter);
        let content = fs::read_to_string(config_path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        
        json.get("pe")
            .and_then(|pe| pe.get("version"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("无法读取版本信息"))
    }
    
    pub fn get_all_drives(&self) -> Vec<BootDrive> {
        self.boot_drives.clone()
    }
    
    pub fn get_current_drive(&self) -> Option<String> {
        self.current_drive.clone()
    }
    
    pub fn set_current_drive(&mut self, drive: String) {
        self.current_drive = Some(drive);
    }
    
    pub fn reload(&mut self) {
        self.boot_drives = self.scan_boot_drives();
    }
}