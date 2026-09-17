//! 测试目标哨兵：它的存在让 `cargo:rustc-link-arg-tests`（build.rs 给 test
//! 目标链接 Common-Controls v6 manifest）生效；同时验证 manifest 链接后的
//! 测试可执行文件可以正常加载并运行（缺 manifest 时会因 comctl32 导入解析
//! 失败而直接崩溃，见 build.rs 注释）。

#[test]
fn test_binary_loads_with_common_controls_manifest() {
    assert_eq!(2 + 2, 4);
}
