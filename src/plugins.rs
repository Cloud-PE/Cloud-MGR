use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use anyhow::Result;
use std::collections::{HashSet, HashMap};
use crate::mode::PluginMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub size: String,
    pub version: String,
    pub author: String,
    #[serde(default)]
    pub describe: String,
    #[serde(default)]
    pub file: String,
    pub link: String,
}

impl Plugin {
    fn get_unique_key(&self) -> String {
        format!("{}_{}_{}_{}", self.name, self.version, self.author, self.size)
    }
    
    pub fn get_plugin_id(&self) -> String {
        format!("{}_{}", self.name, self.author)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCategory {
    pub class: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub list: Vec<Plugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudPEResponse {
    pub code: i32,
    pub message: String,
    pub data: Vec<PluginCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotPEResponse {
    pub state: String,
    pub data: Vec<HotPECategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotPECategory {
    pub class: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub list: Vec<HotPEPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotPEPlugin {
    pub name: String,
    pub size: serde_json::Value,
    #[serde(deserialize_with = "deserialize_modified")]
    pub modified: String,
    pub link: String,
}

fn deserialize_modified<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    
    match value {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => {
            if let Some(timestamp) = n.as_i64() {
                Ok(format_timestamp(timestamp))
            } else {
                Ok(n.to_string())
            }
        }
        _ => Err(D::Error::custom("expected string or number for modified field")),
    }
}

fn format_timestamp(timestamp: i64) -> String {
    use chrono::DateTime;
    if let Some(dt) = DateTime::from_timestamp(timestamp, 0) {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        timestamp.to_string()
    }
}

pub struct PluginManager {
    pub categories: Vec<PluginCategory>,
    enabled_plugins: Vec<Plugin>,
    disabled_plugins: Vec<Plugin>,
    enabled_plugin_map: HashMap<String, Plugin>,
    mode: PluginMode,
}

impl PluginManager {
    pub fn new(mode: PluginMode) -> Self {
        Self {
            categories: Vec::new(),
            enabled_plugins: Vec::new(),
            disabled_plugins: Vec::new(),
            enabled_plugin_map: HashMap::new(),
            mode,
        }
    }
    
    pub async fn fetch_plugins_async(mode: PluginMode) -> Result<Vec<PluginCategory>> {
        let client = reqwest::Client::new();
        let response = client
            .get(mode.get_api_url())
            .send()
            .await?;
        
        let text = response.text().await?;
        
        match mode {
            PluginMode::CloudPE | PluginMode::Edgeless => {
                let mut plugins_response: CloudPEResponse = serde_json::from_str(&text)?;
                
                if plugins_response.code == 200 {
                    for category in &mut plugins_response.data {
                        let mut seen = HashSet::new();
                        let mut unique_plugins = Vec::new();
                        
                        for plugin in &category.list {
                            let key = plugin.get_unique_key();
                            if seen.insert(key) {
                                unique_plugins.push(plugin.clone());
                            }
                        }
                        
                        category.list = unique_plugins;
                    }
                    
                    Ok(plugins_response.data)
                } else {
                    anyhow::bail!("获取插件列表失败: {}", plugins_response.message)
                }
            }
            PluginMode::HotPE => {
                let hotpe_response: HotPEResponse = match serde_json::from_str(&text) {
                    Ok(resp) => resp,
                    Err(e) => {
                        return Err(anyhow::anyhow!("解析HotPE响应失败: {}", e));
                    }
                };
                
                if hotpe_response.state == "success" {
                    let mut categories = Vec::new();
                    
                    for hotpe_category in hotpe_response.data {
                        let mut plugins = Vec::new();
                        
                        for hotpe_plugin in hotpe_category.list {
                            let file_name = hotpe_plugin.name.clone();
                            let parts: Vec<&str> = file_name.trim_end_matches(".HPM").split('_').collect();
                            
                            let (name, author, version, describe) = if parts.len() >= 4 {
                                (parts[0].to_string(), parts[1].to_string(), parts[2].to_string(), parts[3..].join("_"))
                            } else if parts.len() == 3 {
                                (parts[0].to_string(), parts[1].to_string(), parts[2].to_string(), String::new())
                            } else {
                                (file_name.clone(), String::new(), String::new(), String::new())
                            };
                            
                            let size_str = match hotpe_plugin.size {
                                serde_json::Value::Number(n) => {
                                    if let Some(size) = n.as_i64() {
                                        format_file_size(size)
                                    } else if let Some(size) = n.as_f64() {
                                        format_file_size(size as i64)
                                    } else {
                                        "未知大小".to_string()
                                    }
                                }
                                serde_json::Value::String(s) => s,
                                _ => "未知大小".to_string(),
                            };
                            
                            plugins.push(Plugin {
                                name,
                                size: size_str,
                                version,
                                author,
                                describe,
                                file: hotpe_plugin.name,
                                link: hotpe_plugin.link,
                            });
                        }
                        
                        categories.push(PluginCategory {
                            class: hotpe_category.class,
                            icon: hotpe_category.icon,
                            list: plugins,
                        });
                    }
                    
                    Ok(categories)
                } else {
                    anyhow::bail!("获取HotPE模块列表失败")
                }
            }
            _ => anyhow::bail!("不支持的模式"),
        }
    }
    
    pub fn get_categories(&self) -> &Vec<PluginCategory> {
        &self.categories
    }
    
    pub fn search_plugins(&self, keyword: &str) -> Vec<Plugin> {
        let keyword = keyword.to_lowercase();
        let mut results = Vec::new();
        let mut seen = HashSet::new();
        
        for category in &self.categories {
            for plugin in &category.list {
                let search_text = format!("{} {} {} {}", 
                    plugin.name, plugin.author, plugin.describe, plugin.version).to_lowercase();
                    
                if search_text.contains(&keyword) {
                    let key = plugin.get_unique_key();
                    if seen.insert(key) {
                        results.push(plugin.clone());
                    }
                }
            }
        }
        
        results
    }
    
    pub fn load_local_plugins(&mut self, drive_letter: &str) -> Result<()> {
        let plugin_dir = format!("{}\\{}", drive_letter, self.mode.get_plugin_folder());
        let dir_path = Path::new(&plugin_dir);
        
        if !dir_path.exists() {
            fs::create_dir_all(dir_path)?;
        }
        
        self.enabled_plugins.clear();
        self.disabled_plugins.clear();
        self.enabled_plugin_map.clear();
        
        let mut seen_enabled = HashSet::new();
        let mut seen_disabled = HashSet::new();
        
        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(extension) = path.extension() {
                    let ext = extension.to_string_lossy().to_lowercase();
                    let file_name = path.file_name().unwrap().to_string_lossy().to_string();
                    
                    let enabled_ext = self.mode.get_enabled_extension().to_lowercase();
                    let disabled_ext = self.mode.get_disabled_extension().to_lowercase();
                    
                    let is_enabled = match self.mode {
                        PluginMode::HotPE => {
                            ext == "hpm" && !file_name.ends_with(".hpm.off")
                        }
                        _ => ext == enabled_ext,
                    };
                    
                    let is_disabled = match self.mode {
                        PluginMode::HotPE => file_name.ends_with(".hpm.off"),
                        _ => ext == disabled_ext,
                    };
                    
                    if is_enabled || is_disabled {
                        if let Some(plugin) = self.parse_plugin_file(&path) {
                            let key = plugin.get_unique_key();
                            
                            if is_enabled {
                                if seen_enabled.insert(key) {
                                    let plugin_id = plugin.get_plugin_id();
                                    self.enabled_plugin_map.insert(plugin_id, plugin.clone());
                                    self.enabled_plugins.push(plugin);
                                }
                            } else {
                                if seen_disabled.insert(key) {
                                    self.disabled_plugins.push(plugin);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn parse_plugin_file(&self, path: &Path) -> Option<Plugin> {
        let file_name = path.file_name()?.to_string_lossy().to_string();
        
        match self.mode {
            PluginMode::CloudPE => {
                let parts: Vec<&str> = file_name.split('_').collect();
                
                if parts.len() >= 4 {
                    let name = parts[0].to_string();
                    let version = parts[1].to_string();
                    let author = parts[2].to_string();
                    
                    let describe_with_ext = parts[3..].join("_");
                    let describe = describe_with_ext
                        .strip_suffix(".ce")
                        .or_else(|| describe_with_ext.strip_suffix(".CBK"))
                        .unwrap_or(&describe_with_ext)
                        .to_string();
                    
                    let metadata = fs::metadata(path).ok()?;
                    let size = format!("{:.2} MB", metadata.len() as f64 / 1024.0 / 1024.0);
                    
                    Some(Plugin {
                        name,
                        size,
                        version,
                        author,
                        describe,
                        file: file_name,
                        link: String::new(),
                    })
                } else {
                    None
                }
            }
            PluginMode::HotPE => {
                let base_name = file_name
                    .strip_suffix(".HPM")
                    .or_else(|| file_name.strip_suffix(".hpm.off"))
                    .unwrap_or(&file_name);
                    
                let parts: Vec<&str> = base_name.split('_').collect();
                
                if parts.len() >= 3 {
                    let name = parts[0].to_string();
                    let author = parts[1].to_string();
                    let version = parts[2].to_string();
                    let describe = if parts.len() > 3 {
                        parts[3..].join("_")
                    } else {
                        String::new()
                    };
                    
                    let metadata = fs::metadata(path).ok()?;
                    let size = format!("{:.2} MB", metadata.len() as f64 / 1024.0 / 1024.0);
                    
                    Some(Plugin {
                        name,
                        size,
                        version,
                        author,
                        describe,
                        file: file_name,
                        link: String::new(),
                    })
                } else {
                    None
                }
            }
            PluginMode::Edgeless => {
                let base_name = file_name
                    .strip_suffix(".7z")
                    .or_else(|| file_name.strip_suffix(".7zf"))
                    .unwrap_or(&file_name);
                    
                let parts: Vec<&str> = base_name.split('_').collect();
                
                if parts.len() >= 3 {
                    let name = parts[0].to_string();
                    let version = parts[1].to_string();
                    let author = parts[2..].join("_");
                    
                    let metadata = fs::metadata(path).ok()?;
                    let size = format!("{:.2} MB", metadata.len() as f64 / 1024.0 / 1024.0);
                    
                    Some(Plugin {
                        name,
                        size,
                        version,
                        author,
                        describe: String::new(),
                        file: file_name,
                        link: String::new(),
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    
    pub fn enable_plugin(&mut self, drive_letter: &str, file_name: &str) -> Result<()> {
        let plugin_dir = format!("{}\\{}", drive_letter, self.mode.get_plugin_folder());
        let file_path = Path::new(&plugin_dir).join(file_name);
        
        if !file_path.exists() {
            anyhow::bail!("文件不存在");
        }
        
        let new_file_name = match self.mode {
            PluginMode::CloudPE => file_name.replace(".CBK", ".ce"),
            PluginMode::HotPE => file_name.replace(".hpm.off", ".HPM"),
            PluginMode::Edgeless => file_name.replace(".7zf", ".7z"),
            _ => return Ok(()),
        };
        
        let new_file_path = Path::new(&plugin_dir).join(&new_file_name);
        
        fs::rename(&file_path, &new_file_path)?;
        self.load_local_plugins(drive_letter)?;
        
        Ok(())
    }
    
    pub fn disable_plugin(&mut self, drive_letter: &str, file_name: &str) -> Result<()> {
        let plugin_dir = format!("{}\\{}", drive_letter, self.mode.get_plugin_folder());
        let file_path = Path::new(&plugin_dir).join(file_name);
        
        if !file_path.exists() {
            anyhow::bail!("文件不存在");
        }
        
        let new_file_name = match self.mode {
            PluginMode::CloudPE => file_name.replace(".ce", ".CBK"),
            PluginMode::HotPE => {
                if file_name.ends_with(".HPM") {
                    file_name.replace(".HPM", ".hpm.off")
                } else {
                    format!("{}.off", file_name)
                }
            }
            PluginMode::Edgeless => file_name.replace(".7z", ".7zf"),
            _ => return Ok(()),
        };
        
        let new_file_path = Path::new(&plugin_dir).join(&new_file_name);
        
        fs::rename(&file_path, &new_file_path)?;
        self.load_local_plugins(drive_letter)?;
        
        Ok(())
    }
    
    pub fn get_enabled_plugins(&self) -> &Vec<Plugin> {
        &self.enabled_plugins
    }
    
    pub fn get_disabled_plugins(&self) -> &Vec<Plugin> {
        &self.disabled_plugins
    }
    
    pub fn get_enabled_plugin_by_id(&self, plugin_id: &str) -> Option<&Plugin> {
        self.enabled_plugin_map.get(plugin_id)
    }
    
    pub fn compare_versions(&self, version1: &str, version2: &str) -> std::cmp::Ordering {
        let v1_parts = parse_version(version1);
        let v2_parts = parse_version(version2);
        
        let max_len = v1_parts.len().max(v2_parts.len());
        
        for i in 0..max_len {
            let p1 = v1_parts.get(i).unwrap_or(&VersionPart::Number(0));
            let p2 = v2_parts.get(i).unwrap_or(&VersionPart::Number(0));
            
            match p1.cmp(p2) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            }
        }
        
        std::cmp::Ordering::Equal
    }
    
    pub fn delete_plugin_file(&self, drive_letter: &str, file_name: &str) -> Result<()> {
        let plugin_dir = format!("{}\\{}", drive_letter, self.mode.get_plugin_folder());
        let file_path = Path::new(&plugin_dir).join(file_name);
        
        if !file_path.exists() {
            anyhow::bail!("文件不存在");
        }
        
        fs::remove_file(&file_path)?;
        
        Ok(())
    }
    
    pub fn find_market_plugin_by_id(&self, plugin_id: &str) -> Option<Plugin> {
        for category in &self.categories {
            for plugin in &category.list {
                if plugin.get_plugin_id() == plugin_id {
                    return Some(plugin.clone());
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum VersionPart {
    Number(u64),
    Text(String),
}

fn parse_version(version: &str) -> Vec<VersionPart> {
    let mut parts = Vec::new();
    let mut current_number = String::new();
    let mut current_text = String::new();
    
    for ch in version.chars() {
        if ch.is_ascii_digit() {
            if !current_text.is_empty() {
                parts.push(VersionPart::Text(current_text.to_lowercase()));
                current_text.clear();
            }
            current_number.push(ch);
        } else if ch.is_ascii_alphabetic() {
            if !current_number.is_empty() {
                if let Ok(num) = current_number.parse::<u64>() {
                    parts.push(VersionPart::Number(num));
                }
                current_number.clear();
            }
            current_text.push(ch);
        } else {
            if !current_number.is_empty() {
                if let Ok(num) = current_number.parse::<u64>() {
                    parts.push(VersionPart::Number(num));
                }
                current_number.clear();
            }
            if !current_text.is_empty() {
                parts.push(VersionPart::Text(current_text.to_lowercase()));
                current_text.clear();
            }
        }
    }
    
    if !current_number.is_empty() {
        if let Ok(num) = current_number.parse::<u64>() {
            parts.push(VersionPart::Number(num));
        }
    }
    if !current_text.is_empty() {
        parts.push(VersionPart::Text(current_text.to_lowercase()));
    }
    
    parts
}

fn format_file_size(size: i64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.2} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.2} MB", size as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", size as f64 / 1024.0 / 1024.0 / 1024.0)
    }
}
