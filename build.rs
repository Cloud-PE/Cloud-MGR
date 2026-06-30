use winres::WindowsResource;

fn main() {
    if cfg!(target_os = "windows") {
        // 默认 requireAdministrator（生产）。设了 CM_NOADMIN 环境变量则编译成 asInvoker，
        // 方便开发期离屏截图不弹 UAC。生产构建不带该变量即恢复需管理员。
        println!("cargo:rerun-if-env-changed=CM_NOADMIN");
        println!("cargo:rustc-check-cfg=cfg(cm_noadmin)");
        let no_admin = std::env::var("CM_NOADMIN").is_ok();
        if no_admin {
            // 开发期：除清单 asInvoker 外，同时跳过运行时 runas 自我提权（见 main.rs request_admin）。
            println!("cargo:rustc-cfg=cm_noadmin");
        }
        let level = if no_admin { "asInvoker" } else { "requireAdministrator" };

        let manifest = format!(
            r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
    <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
        <security>
            <requestedPrivileges>
                <requestedExecutionLevel level="{level}" uiAccess="false" />
            </requestedPrivileges>
        </security>
    </trustInfo>
    <application xmlns="urn:schemas-microsoft-com:asm.v3">
        <windowsSettings>
            <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
            <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, PerMonitor</dpiAwareness>
        </windowsSettings>
    </application>
</assembly>
            "#
        );

        WindowsResource::new()
            .set_icon("assets/icon.ico")
            .set("CompanyName", "Cloud-PE Dev.")
            .set("FileDescription", "Cloud-PE 插件市场")
            .set("FileVersion", "0.2.0.0")
            .set("InternalName", "cloud-pe-plugin-market")
            .set("LegalCopyright", "© 2025-present Cloud-PE Dev.")
            .set("OriginalFilename", "cloud-pe-plugin-market.exe")
            .set("ProductName", "Cloud-PE 插件市场")
            .set("ProductVersion", "0.2.0")
            .set_manifest(&manifest)
            .compile()
            .unwrap();
    }
}
