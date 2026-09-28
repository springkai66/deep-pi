fn main() {
    // tauri-build 默认只把 manifest 资源链接到 bin 目标（rustc-link-arg-bins）。
    // tauri-plugin-dialog 在 Windows 上启用 rfd/common-controls-v6，会让所有
    // 链接它的目标静态导入 TaskDialogIndirect——该导出只存在于 comctl32 v6，
    // 而 v6 必须经应用程序 manifest 的依赖声明激活。为让 `cargo test` 的测试
    // 可执行文件也能加载（否则 STATUS_ENTRYPOINT_NOT_FOUND），这里关闭
    // tauri-build 的 manifest，改由下方统一为所有目标（bin/cdylib/test）链接
    // 同一份 manifest（内容与 tauri-build 默认的 windows-app-manifest.xml 一致）。
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    tauri_build::try_build(attributes).expect("failed to run tauri-build");

    #[cfg(target_os = "windows")]
    embed_common_controls_manifest();
}

/// 给全部链接目标（bin/cdylib/test，`rustc-link-arg` 覆盖范围）嵌入
/// Common-Controls v6 依赖的 manifest 资源。
#[cfg(target_os = "windows")]
fn embed_common_controls_manifest() {
    const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"#;

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is required"));
    let manifest_path = out_dir.join("deeppi-common-controls.manifest");
    std::fs::write(&manifest_path, MANIFEST).expect("failed to write the app manifest");
    let rc_path = out_dir.join("deeppi-common-controls.rc");
    // `1 24`：RT_MANIFEST，资源 ID 1 = 可执行文件根激活上下文。
    // 路径统一使用正斜杠，避免 RC 字符串对反斜杠转义的敏感处理。
    let forward = manifest_path.display().to_string().replace('\\', "/");
    std::fs::write(&rc_path, format!("1 24 \"{forward}\""))
        .expect("failed to write the resource script");
    let _ = embed_resource::compile_for_everything(&rc_path, embed_resource::NONE);
}
