# JVM Backend Maintenance Guide

The JVM backend is responsible for compiling Valkyrie into the Java class file format.

## Compilation Pipeline

`Source -> AST -> HIR -> CFG -> JVM Bytecode`

## Design Considerations

### Stack Machine Architecture
The JVM is a stack-based virtual machine. Using a register-based LIR would introduce redundant `iload`/`istore` instructions. Therefore, the JVM backend skips the SSA and LIR stages and generates code directly from the **CFG** (Control Flow Graph).

### Linearization
Basic blocks of the CFG are linearized into a flat instruction stream. Jump instructions (`Goto`, `SwitchInt`) are handled by calculating relative offsets during the second pass (label fixing).

### Type System
Valkyrie types map to JVM descriptors:
- `Int32` -> `I`
- `Int64` -> `J`
- `Float32` -> `F`
- `Float64` -> `D`
- `String` -> `Ljava/lang/String;`
- `Bool` -> `Z`
- `Unit` -> `V`
- `Struct/Enum` -> `Lpath/to/Class;`
- `Pointer` -> `J` (Currently mapped to a 64-bit integer)

### Expression Generation
- **Arithmetic Operations**: Directly mapped to instructions like `iadd`, `ladd`, `fadd`, `dadd`, and `irem`, `lrem`, `frem`, `drem`.
- **Comparison Operations**:
    - For `Int32`: Use `if_icmp<cond>` to jump and push 0 or 1.
    - For `Int64/Float/Double`: Use `lcmp/fcmpl/dcmpl` instructions, followed by `if<cond>` instructions to generate a boolean value.
    - For `Object` types (Struct, Enum, String): Use `if_acmp<cond>` for reference equality comparison.
- **Bitwise and Logical Operations**:
    - `And`, `Or`, `Xor` map to `iand/land`, `ior/lor`, `ixor/lxor`.
    - `Shl`, `Shr` map to `ishl/lshl`, `ishr/lshr`.
- **Unary Operations**:
    - `Neg` maps to `ineg/lneg/fneg/dneg`.
    - `Not`: Implemented using `iconst_1/iconst_m1` and `ixor` for `Bool` and `Int32`; implemented using `-1L` from the constant pool and `lxor` for `Int64`.

### Methods and Calls
- Each Valkyrie function is emitted as a `static` method in a class.
- **Name Mangling**:
    - Global functions keep their original names.
    - Struct/Enum methods: `ClassName$MethodName`.
    - Trait methods: `TraitName$MethodName`.
    - Impl methods: `[TraitName$]TargetName_MethodName`.
- Method calls (`Call`) currently use `invokestatic`. The backend automatically retrieves the callee's signature from the `CfgProgram` to generate the correct descriptor.
- For dynamic calls or function pointers, the backend uses the `invoke` method of `java/lang/invoke/MethodHandle`.

### Generics Support
- **Type Erasure**: All generic types are erased to `Ljava/lang/Object;` in descriptors.
- **Signature Attribute**: To preserve generic information, the backend generates JVM `Signature` attributes for generic functions and fields.
- **Function Types**: Valkyrie function types map to `Ljava/lang/invoke/MethodHandle;`.

### Algebraic Effects
- Valkyrie's `raise` is treated as an effect, governed by the AE mechanism.
- **Raise Implementation**: Equivalent to a Java exception in non-resume cases, mapped to the `athrow` instruction.
- **Handlers**: Implemented using the `exception_table` in the JVM `Code` attribute for `PushHandler`/`PopHandler`.
- When entering a handler block, the backend automatically initializes `current_stack` to 1 to match the JVM specification (the effect object will be pushed onto the stack top).
- **Resume**: Currently only supports non-resume paths; full continuation support is planned for the future via stack frame capture or bytecode rewriting.

### Control Flow Optimization
- **Jump Instructions**:
    - Uses `goto_w` (4-byte offset) by default to support jumps in very large basic blocks.
    - `SwitchInt` automatically chooses between `tableswitch` or `lookupswitch` based on the jump range and density, both using 4-byte offsets.

## Current Progress

- [x] Basic Class file structure generation
- [x] Constant pool management (UTF8, Integer, Float, Long, Double, Class, String, FieldRef, MethodRef, NameAndType, MethodHandle, MethodType)
- [x] Basic type mapping (Bool, Int32, Int64, Float32, Float64, String, Unit)
- [x] Arithmetic operations (Add, Sub, Mul, Div, Rem) support for Int32, Int64, Float32, Float64
- [x] Bitwise and logical operations (And, Or, Xor, Shl, Shr)
- [x] Unary operations (Neg, Not)
- [x] Comparison operations (Eq, Ne, Lt, Le, Gt, Ge) support for Int32, Int64, Float32, Float64
- [x] Basic instantiation and field access for Structs and Enums (limited to 1 level)
- [x] Basic array support (newarray, anewarray, iastore/iaload, etc.)
- [x] Static method calls (invokestatic)
- [x] Basic algebraic effects (Raise, Handlers)
- [x] Basic generics Signature support

## Roadmap

### Short-term
1. **Code Refactoring**: Refactor `emitter.rs` to reduce redundancy in bytecode emission logic.
2. **Debug Support**: Implement the `LineNumberTable` attribute to support source line mapping.
3. **Field Access Enhancement**: Support multi-level deep field/index access.
4. **Method Call Refinement**: Support `invokevirtual` and `invokeinterface`.

### Mid-term
1. **Closure Support**: Implement Lambda and closures using `invokedynamic`.
2. **Performance Optimization**: Implement smarter stack balance optimization to reduce unnecessary `dup` and `pop`.
3. **Reflection and Introspection**: Support Valkyrie runtime reflection.

### Long-term
1. **Incremental Compilation**: Support incremental compilation based on class files.
2. **Native Interoperability**: Optimize interaction performance with the Java standard library.
