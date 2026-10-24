//! 正式编译链 feature 合同：默认装配不得链接旁路 crate。

#[test]
fn default_features_exclude_format_spy_and_bootstrap_lane() {
    assert!(cfg!(feature = "ovm-lane"), "正式链默认应启用 ovm-lane");
    assert!(!cfg!(feature = "format-spy"), "默认 legion 不得启用诊断链（format-spy）");
    assert!(!cfg!(feature = "bootstrap-lane"), "默认 legion 不得启用 bootstrap-lane");
}
