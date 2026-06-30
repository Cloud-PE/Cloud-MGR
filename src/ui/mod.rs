//! fluentpx UI 层：加载页 / 选择源 / 主界面（市场·管理·设置）。

mod loading;
mod manage;
mod market;
mod selector;
mod settings;
mod shell;
mod shot;
mod widgets;

pub use loading::LoadingRoot;
pub use selector::SelectorRoot;
pub use shell::CloudMgrRoot;
pub use shot::shot;
