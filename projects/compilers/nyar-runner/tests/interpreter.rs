use nyar_runner::{RuntimeContract, RuntimeFamily};

#[test]
fn exposes_runtime_family_defaults_for_all_builtin_families() {
    let clr = RuntimeFamily::Clr.default_template(None);
    assert_eq!(clr.command, "dotnet");
    assert_eq!(clr.args, vec!["exec".to_string(), "{artifact}".to_string()]);

    let jvm = RuntimeFamily::Jvm.default_template(None);
    assert_eq!(jvm.command, "java");
    assert_eq!(jvm.args, vec!["-cp".to_string(), "{classpath}".to_string(), "{entry}".to_string()]);

    let node = RuntimeFamily::Node.default_template(None);
    assert_eq!(node.command, "node");
    assert_eq!(node.args, vec!["{artifact}".to_string()]);

    let windows = RuntimeFamily::Windows.default_template(None);
    assert_eq!(windows.command, "{artifact}");
    assert!(windows.args.is_empty());

    let wasi = RuntimeFamily::Wasi.default_template(Some(RuntimeContract {
        logical_entry: Some("main"),
        physical_entry: Some("app"),
        wasi_p3: false,
    }));
    assert_eq!(wasi.command, "wasmtime");
    assert_eq!(
        wasi.args,
        vec![
            "run".to_string(),
            "-W".to_string(),
            "gc".to_string(),
            "-W".to_string(),
            "wmemcheck".to_string(),
            "-W".to_string(),
            "max-memory-size=16777216".to_string(),
            "{artifact}".to_string(),
        ]
    );

    let wasip3 = RuntimeFamily::Wasi.default_template(Some(RuntimeContract {
        logical_entry: Some("_start"),
        physical_entry: Some("app.wasi"),
        wasi_p3: true,
    }));
    assert!(wasip3.args.windows(2).any(|pair| pair == ["-S", "p3"]));
}

#[test]
fn parses_runtime_family_names() {
    assert_eq!(RuntimeFamily::parse("clr"), Some(RuntimeFamily::Clr));
    assert_eq!(RuntimeFamily::parse("JVM"), Some(RuntimeFamily::Jvm));
    assert_eq!(RuntimeFamily::parse("node"), Some(RuntimeFamily::Node));
    assert_eq!(RuntimeFamily::parse("windows"), Some(RuntimeFamily::Windows));
    assert_eq!(RuntimeFamily::parse("wasi"), Some(RuntimeFamily::Wasi));
    assert_eq!(RuntimeFamily::parse("unknown"), None);
}
