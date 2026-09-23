# Valkyrie 示例集合

本目录包含了 Valkyrie 语言在各个领域的完整示例和教程，展示了 Valkyrie 的强大功能和广泛应用场景。

## 🎮 游戏开发

[游戏开发框架](game-development/) - 完整的游戏开发解决方案

- **核心功能**：游戏引擎架构、ECS 系统、渲染管线
- **图形编程**：Shader 开发、GPU 计算、wgpu 集成
- **性能优化**：内存管理、并发处理、资源管理
- **实用工具**：场景管理、资源加载、调试工具

## 🔧 嵌入式开发

[嵌入式开发](embedded-development/) - 嵌入式系统和 WebAssembly 开发

- **WASM 开发**：WebAssembly 模块、WASI 接口、内存管理
- **微控制器编程**：GPIO 控制、中断处理、通信协议
- **实时系统**：RTOS 开发、任务调度、时序控制
- **传感器接口**：ADC 采集、I2C/SPI 通信、数据处理
- **电源管理**：低功耗设计、睡眠模式、唤醒机制

## 🔬 芯片设计

[芯片设计](chip-design/) - 硬件描述语言和数字电路设计

- **HDL 基础**：硬件数据类型、模块定义、时序逻辑
- **数字电路**：组合逻辑、ALU 设计、状态机
- **处理器设计**：RISC-V 核心、指令解码、流水线
- **内存系统**：RAM 设计、缓存结构、存储控制器
- **总线互连**：AXI4 协议、交叉开关、仲裁器
- **验证方法**：测试平台、形式化验证、覆盖率分析
- **综合实现**：FPGA 开发、ASIC 设计流程、时序约束

## 🌐 网页开发

[网页开发](web-development/) - 现代化的 Web 开发框架

- **Web 服务器**：高性能 HTTP 服务器、路由系统、中间件
- **界面组件**：基于 Widget 的响应式 UI 开发
- **XML 语法**：类 TSX 的声明式语法 (X-Grammar)
- **原生语法**：基于 DSL 的函数式语法 (V-Grammar)
- **事件处理**：单点事件与广播事件机制

## 🌟 特色功能

### 类型安全

Valkyrie 在所有领域都提供编译时类型检查，确保代码的正确性和安全性：

```valkyrie
# 游戏开发中的类型安全
struct Player {
    position: Vec3<f32>,
    health: Health<100>,  # 编译时范围检查
    inventory: Inventory<32>  # 固定大小容器
}

# 嵌入式开发中的硬件抽象
struct GpioPin<const PIN: u8, const PORT: char> {
    _phantom: PhantomData<(PIN, PORT)>
}

# 芯片设计中的位宽检查
type UInt<const W: usize> = HardwareType<u64, W>
let result: UInt<33> = add(a: UInt<32>, b: UInt<32>)  # 自动推导位宽
```

### 零成本抽象

高级抽象在编译时完全优化，运行时性能等同于手写的底层代码：

```valkyrie
# 高级游戏逻辑
entities.query::<(Transform, Velocity)>()
    .par_iter()
    .for_each(|(transform, velocity)| {
        transform.position += velocity.delta * dt
    })

# 编译后等效于优化的循环
# 无运行时开销，无动态分配
```

### 内存安全

在系统编程中提供内存安全保证，避免常见的内存错误：

```valkyrie
# 嵌入式开发中的安全内存操作
micro process_buffer(buffer: &mut [u8]) {
    # 编译时边界检查
    for i in 0..buffer.len() {
        buffer[i] = buffer[i].wrapping_add(1)  # 明确的溢出行为
    }
}

# 芯片设计中的安全硬件访问
micro write_register<const ADDR: u32>(value: u32) {
    # 编译时地址验证
    unsafe { *(ADDR as *mut u32) = value }
}
```

### 并发编程

内置的并发原语支持安全的多线程和异步编程：

```valkyrie
# 游戏中的并行系统
async micro update_physics(world: &World) {
    let (positions, velocities) = world.query_mut::<(Position, Velocity)>()
    
    positions.par_iter_mut()
        .zip(velocities.par_iter())
        .for_each(|(pos, vel)| {
            pos.update(*vel)
        })
}

# 嵌入式中的异步 I/O
async micro read_sensor() -> SensorData {
    let data = i2c.read_async(SENSOR_ADDR).await?
    SensorData::parse(data)
}
```

## 🚀 开始使用

1. **选择领域**：根据你的项目需求选择相应的示例目录
2. **阅读文档**：每个目录都包含详细的说明和教程
3. **运行示例**：所有示例都可以直接编译和运行
4. **深入学习**：通过修改示例代码来学习 Valkyrie 的特性

## 📚 相关资源

- [Valkyrie 语言参考](../language/) - 完整的语言规范和特性说明
- [标准库文档](../stdlib/) - 标准库 API 参考
- [工具链指南](../toolchain/) - 编译器、调试器等工具使用说明
- [最佳实践](../best-practices/) - 代码风格和设计模式

## 🤝 贡献指南

欢迎为 Valkyrie 示例集合贡献代码和文档：

1. **报告问题**：发现 bug 或有改进建议请提交 issue
2. **提交代码**：遵循项目的代码规范和测试要求
3. **完善文档**：帮助改进示例的说明和教程
4. **分享经验**：分享你在使用 Valkyrie 过程中的心得体会

Valkyrie 致力于为现代系统编程提供安全、高效、易用的解决方案。通过这些示例，你可以快速掌握 Valkyrie 在各个领域的应用，并开始构建自己的项目。