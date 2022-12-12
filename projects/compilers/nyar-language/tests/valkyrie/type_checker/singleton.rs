use nyar_language::{ValkyrieCompiler, types::SourceID};

#[test]
fn rejects_generic_singleton() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4310 });
    let error = compiler
        .compile_source(
            r#"
singleton Container<T> {
    mut value: T = std::default::default()
}
"#,
        )
        .expect_err("generic singleton should be rejected");
    assert!(error.to_string().contains("generic singleton `Container` is not allowed"), "expected generic singleton rejection, got: {error}");
}

#[test]
fn rejects_finalizer_on_eager_singleton() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4311 });
    let error = compiler
        .compile_source(
            r#"
singleton EagerStore {
    mut data: i64 = 0

    micro finalize(mut self) {
        self.data = 0
    }
}
"#,
        )
        .expect_err("finalizer on eager singleton should be rejected");
    assert!(error.to_string().contains("eager and cannot define a finalizer"), "expected finalizer-on-eager rejection, got: {error}");
}

#[test]
fn lazy_singleton_with_constructor_and_finalizer_compiles() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4312 });
    compiler
        .compile_source(
            r#"
lazy singleton ResourcePool {
    mut handle: i64 = 0

    micro init(mut self) {
        self.handle = 42
    }

    micro finalize(mut self) {
        self.handle = 0
    }

    micro get_handle(self) -> i64 {
        self.handle
    }
}
"#,
        )
        .expect("lazy singleton with init and finalize should compile");
}

#[test]
fn init_method_routed_to_constructor_field() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4313 });
    let module = compiler
        .compile_source(
            r#"
singleton WithInit {
    mut x: i64 = 0

    micro init(mut self) {
        self.x = 1
    }

    micro query(self) -> i64 {
        self.x
    }
}
"#,
        )
        .expect("singleton with init should compile");

    assert_eq!(module.singletons.len(), 1);
    let s = &module.singletons[0];
    assert!(s.constructor.is_some(), "init should be routed to constructor");
    assert!(s.finalizer.is_none(), "no finalize, so finalizer should be None");
    assert_eq!(s.methods.len(), 1, "only `query` should remain in ordinary methods");
    assert_eq!(s.methods[0].name.as_str(), "query");
}

#[test]
fn finalize_method_routed_to_finalizer_field() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4314 });
    let module = compiler
        .compile_source(
            r#"
lazy singleton WithFinalizer {
    mut fd: i64 = 0

    micro finalize(mut self) {
        self.fd = -1
    }

    micro read(self) -> i64 {
        self.fd
    }
}
"#,
        )
        .expect("lazy singleton with finalize should compile");

    assert_eq!(module.singletons.len(), 1);
    let s = &module.singletons[0];
    assert!(s.finalizer.is_some(), "finalize should be routed to finalizer");
    assert!(s.constructor.is_none(), "no init, so constructor should be None");
    assert_eq!(s.methods.len(), 1, "only `read` should remain in ordinary methods");
    assert_eq!(s.methods[0].name.as_str(), "read");
}

#[test]
fn allows_mut_field_write_in_singleton_method_body() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4301 });
    compiler
        .compile_source(
            r#"
singleton Counter {
    mut total: i64 = 0

    micro increment(mut self) -> i64 {
        self.total += 1
        self.total
    }
}

micro main() -> i64 {
    Counter.increment()
}
"#,
        )
        .expect("mut field write in singleton method should compile");
}

#[test]
fn rejects_readonly_field_write_on_singleton() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4302 });
    let error = compiler
        .compile_source(
            r#"
singleton Counter {
    total: i64 = 0
}

micro main() {
    Counter.total = 1
}
"#,
        )
        .expect_err("readonly singleton field write should fail");
    assert!(error.to_string().contains("readonly field write denied"));
}

#[test]
fn rejects_singleton_constructor() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4303 });
    let error = compiler
        .compile_source(
            r#"
singleton Counter {
    total: i64 = 0
}

micro main() {
    Counter { total: 1 }
}
"#,
        )
        .expect_err("singleton construction should fail");
    assert!(error.to_string().contains("cannot be constructed"));
}

#[test]
fn allows_static_singleton_field_assignment_for_mut_field() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4304 });
    compiler
        .compile_source(
            r#"
singleton AppConfig {
    mut debug_mode: bool = false
}

micro main() {
    AppConfig.debug_mode = true
}
"#,
        )
        .expect("mut singleton field assignment at module level should compile");
}

#[test]
fn allows_self_method_call_inside_singleton_method_body() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4305 });
    compiler
        .compile_source(
            r#"
singleton Counter {
    mut total: i64 = 0

    micro increment(mut self) -> i64 {
        self.total += 1
        self.total
    }

    micro tick(mut self) -> i64 {
        self.increment()
    }
}

micro main() -> i64 {
    Counter.tick()
}
"#,
        )
        .expect("singleton self method call should compile");
}

#[test]
fn rejects_readonly_self_field_write_inside_singleton_method_body() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4306 });
    let error = compiler
        .compile_source(
            r#"
singleton Counter {
    total: i64 = 0

    micro reset(mut self) {
        self.total = 1
    }
}

micro main() {
    Counter.reset()
}
"#,
        )
        .expect_err("readonly self field write in singleton method should fail");
    assert!(error.to_string().contains("readonly field write denied"));
}
