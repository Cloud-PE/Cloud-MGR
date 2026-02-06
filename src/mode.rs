use std::hash::Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginMode {
    CloudPE,
    HotPE,
    Edgeless,
    Select,
}

impl PluginMode {
    pub fn get_api_url(&self) -> &str {
        match self {
            PluginMode::CloudPE => "https://api.cloud-pe.cn/GetPlugins/",
            PluginMode::HotPE => "https://api.hotpe.top/API/HotPE/GetHPMList/",
            PluginMode::Edgeless => "https://api.cloud-pe.cn/EdgelessPlugins/",
            _ => "",
        }
    }
    
    pub fn get_connect_test_url(&self) -> &str {
        match self {
            PluginMode::CloudPE => "https://api.cloud-pe.cn/connecttest/",
            PluginMode::HotPE => "https://api.hotpe.top/API/HotPE/GetHPMList/",
            PluginMode::Edgeless => "https://api.cloud-pe.cn/EdgelessPlugins/",
            _ => "",
        }
    }
    
    pub fn get_plugin_folder(&self) -> &str {
        match self {
            PluginMode::CloudPE => "ce-apps",
            PluginMode::HotPE => "HotPEModule",
            PluginMode::Edgeless => "Edgeless\\Resource",
            _ => "",
        }
    }
    
    pub fn get_enabled_extension(&self) -> &str {
        match self {
            PluginMode::CloudPE => "ce",
            PluginMode::HotPE => "HPM",
            PluginMode::Edgeless => "7z",
            _ => "",
        }
    }
    
    pub fn get_disabled_extension(&self) -> &str {
        match self {
            PluginMode::CloudPE => "CBK",
            PluginMode::HotPE => "hpm.off",
            PluginMode::Edgeless => "7zf",
            _ => "",
        }
    }
    
    pub fn get_plugin_market_name(&self) -> &str {
        match self {
            PluginMode::HotPE => "模块市场",
            _ => "插件市场",
        }
    }
    
    pub fn get_plugin_manage_name(&self) -> &str {
        match self {
            PluginMode::HotPE => "模块管理",
            _ => "插件管理",
        }
    }
    
    pub fn get_title(&self) -> &str {
        match self {
            PluginMode::CloudPE => "Cloud-PE 插件市场",
            PluginMode::HotPE => "HotPE 模块下载",
            PluginMode::Edgeless => "Edgeless 插件下载",
            _ => "选择插件源",
        }
    }
    
    pub fn get_server_name(&self) -> &str {
        match self {
            PluginMode::CloudPE => "Cloud-PE",
            PluginMode::HotPE => "HotPE",
            PluginMode::Edgeless => "Edgeless",
            _ => "",
        }
    }
}