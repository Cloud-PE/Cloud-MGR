use winres::WindowsResource;

fn main() {
    if cfg!(target_os = "windows") {
        WindowsResource::new()
            .set_icon("assets/icon.ico")
            .set("CompanyName", "Cloud-PE Dev.")
            .set("FileDescription", "Cloud-PE 插件市场")
            .set("FileVersion", "0.1.0.0")
            .set("InternalName", "cloud-pe-plugin-market")
            .set("LegalCopyright", "© 2025-present Cloud-PE Dev.")
            .set("OriginalFilename", "cloud-pe-plugin-market.exe")
            .set("ProductName", "Cloud-PE 插件市场")
            .set("ProductVersion", "0.1.0")
            .set_manifest(r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
    <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
        <security>
            <requestedPrivileges>
                <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
            </requestedPrivileges>
        </security>
    </trustInfo>
</assembly>
            "#)
            .compile()
            .unwrap();
    }
}