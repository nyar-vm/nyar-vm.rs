use rusty_typescript::RustyTypescriptFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_interface_with_generics() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "
        interface Box<T> {
            value: T;
        }
        let b: Box<number> = { value: 42 };
        b.value;
    ";

    let _module = frontend.compile_to_nyar(source).expect("Compilation failed");
}

#[test]
fn test_type_alias_complex() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "
        type ID = string | number;
        type Point = { x: number, y: number };
        type Point3D = Point & { z: number };
        
        let id: ID = 123;
        let p: Point3D = { x: 1, y: 2, z: 3 };
    ";

    let _module = frontend.compile_to_nyar(source).expect("Compilation failed");
}

#[test]
fn test_enum_with_initializers() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "
        enum Color {
            Red = 1,
            Green,
            Blue = 4
        }
        let c = Color.Green;
    ";

    let _module = frontend.compile_to_nyar(source).expect("Compilation failed");
}

#[test]
fn test_generic_function() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "
        function identity<T>(arg: T): T {
            return arg;
        }
        let x = identity<number>(10);
    ";

    let _module = frontend.compile_to_nyar(source).expect("Compilation failed");
}

#[test]
fn test_advanced_type_operators() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "
        type Point = { x: number, y: number };
        type PointKeys = keyof Point;
        let k: PointKeys = 'x';
        
        let p = { x: 1, y: 2 };
        type P = typeof p;
    ";

    let _module = frontend.compile_to_nyar(source).expect("Compilation failed");
}

#[test]
fn test_utility_types() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "
        interface Todo {
            title: string;
            description: string;
        }
        type PartialTodo = Partial<Todo>;
        type TodoPreview = Pick<Todo, 'title'>;
        type TodoWithoutDesc = Omit<Todo, 'description'>;
        type StringRecord = Record<string, string>;
    ";

    let _module = frontend.compile_to_nyar(source).expect("Compilation failed");
}

#[test]
fn test_null_safety() {
    let frontend = RustyTypescriptFrontend::new();
    let source = "
        let x: string | null = null;
        let y: number | undefined = undefined;
        
        interface User {
            name: string;
            age?: number;
        }
    ";

    let _module = frontend.compile_to_nyar(source).expect("Compilation failed");
}
