use rusty_typescript::RustyTypescriptFrontend;
use nyar_types::NyarFrontend;
use nyar_vm::vm::interpreter::NyarVM;

#[test]
fn test_if_statement() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "let x = 10; let y = 0; if (x > 5) { y = 1; } else { y = 2; } y;";

    let module = frontend.compile_to_nyar(source).expect("Compilation failed");
    let mut vm = NyarVM::new();
    vm.load_module(module.into());

    let result = vm.execute(0, 0).expect("Execution failed");
    assert_eq!(unsafe { result.as_int() }, 1);
}

#[test]
fn test_while_loop() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "let i = 0; let sum = 0; while (i < 10) { sum = sum + i; i = i + 1; } sum;";

    let module = frontend.compile_to_nyar(source).expect("Compilation failed");
    let mut vm = NyarVM::new();
    vm.load_module(module.into());

    let result = vm.execute(0, 0).expect("Execution failed");
    assert_eq!(unsafe { result.as_int() }, 45);
}

#[test]
fn test_arrow_function() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "let add = (a, b) => a + b; add(10, 20);";

    let module = frontend.compile_to_nyar(source).expect("Compilation failed");
    let mut vm = NyarVM::new();
    vm.load_module(module.into());

    let result = vm.execute(0, 0).expect("Execution failed");
    assert_eq!(unsafe { result.as_int() }, 30);
}

#[test]
fn test_class_modifiers() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "
        class Person {
            private name: string;
            protected age: number;
            public readonly id: number;
            static count: number = 0;

            constructor(name: string, age: number, id: number) {
                this.name = name;
                this.age = age;
                this.id = id;
                Person.count = Person.count + 1;
            }

            public get_name() { return this.name; }
            static get_count() { return Person.count; }
        }
        let p = new Person('Alice', 25, 1);
        p.get_name();
    ";

    let module = frontend.compile_to_nyar(source).expect("Compilation failed");
    // Since we don't have full GC/Class implementation in VM yet, 
    // we just verify it compiles to UIR/Nyar module without errors.
}

#[test]
fn test_abstract_class() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "
        abstract class Animal {
            abstract make_sound(): void;
            move(): void {
                // move
            }
        }
        class Dog extends Animal {
            make_sound() {
                // bark
            }
        }
        let d = new Dog();
        d.make_sound();
    ";

    let module = frontend.compile_to_nyar(source).expect("Compilation failed");
}

#[test]
fn test_implements() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "
        interface Drawable {
            draw(): void;
        }
        class Circle implements Drawable {
            draw() {
                // draw circle
            }
        }
        let c = new Circle();
        c.draw();
    ";

    let module = frontend.compile_to_nyar(source).expect("Compilation failed");
}

#[test]
fn test_array_literal() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "let arr = [1, 2, 3]; arr[1];";

    let module = frontend.compile_to_nyar(source).expect("Compilation failed");
    let mut vm = NyarVM::new();
    vm.load_module(module.into());

    let result = vm.execute(0, 0).expect("Execution failed");
    assert_eq!(unsafe { result.as_int() }, 2);
}

#[test]
fn test_object_literal() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "let obj = { x: 10, y: 20 }; obj.x + obj.y;";

    let module = frontend.compile_to_nyar(source).expect("Compilation failed");
    let mut vm = NyarVM::new();
    vm.load_module(module.into());

    let result = vm.execute(0, 0).expect("Execution failed");
    assert_eq!(unsafe { result.as_int() }, 30);
}
