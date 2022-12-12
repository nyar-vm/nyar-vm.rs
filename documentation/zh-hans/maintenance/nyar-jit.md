# nyar-jit

`nyar-jit` 负责将热点代码段从解释执行转换为本机机器码执行。通过深度集成 `ProjectChomsky` 的优化能力与 `Gaia` 引擎的底层抽象，实现接近原生的执行效率。

## 核心设计哲学：等价饱和优化 (Equality Saturation)

与传统编译器采用固定顺序的优化 Pass 不同，`nyar-jit` 的核心是基于 **E-Graph** 的等价饱和优化：

- **非破坏性变换**: 优化过程不会丢失原始信息，而是不断在 E-Graph 中添加等价的表达式。
- **全局最优提取**: 利用 `UniversalOptimizer` 根据代价模型（Cost Model）从成千上万种等价组合中提取出当前架构下的最优指令序列。
- **跨语言优化**: 由于 `IKun` 意图流是语言无关的，同一套优化逻辑可以同时服务于多种托管语言。

## 编译流水线与产物复用

`nyar-jit` 采用 **多层编译 (Tiered Compilation)** 策略，并支持 **JIT 产物持久化 (Artifact Reuse)**，以平衡启动速度、内存占用与峰值性能：

-   **Tier 0: 解释器 (Interpreter)**: 快速启动，收集运行时 Profile 数据（类型反馈、分支频率）。
-   **Tier 1: 基准 JIT (Baseline JIT)**: 快速编译，进行简单的局部优化，减少解释开销。
-   **Tier 2: 优化 JIT (Optimizing JIT)**: 对极热点代码启用 `UniversalOptimizer`，进行昂贵的 E-Graph 等价饱和优化。
-   **产物复用 (Artifact Reuse)**: 
    -   **持久化存储**: 经过 Tier 2 优化后的机器码产物可以被序列化并缓存到磁盘（如 `x86_64` 架构的 `.nyar.x86_64` 文件中）。
    -   **即时加载**: 在后续运行中，如果模块未发生变化，VM 可以直接加载已编译的机器码，跳过耗时的 E-Graph 饱和与指令选择阶段。
    -   **FFI 集成**: JIT 产物可以作为高性能的 FFI 模块被其他项目直接调用，实现“一次优化，处处复用”。

具体的编译工作流程分为以下六个阶段：

1.  **Hotness Tracking (热点追踪)**: VM 监控字节码执行频率。当函数或循环的调用次数超过预设阈值时，触发 JIT 编译。
2.  **Intent Extraction (意图提取)**: 从运行中的 VM 提取 `IKun` 意图流。这不仅仅是简单的字节码翻译，还包含了运行时的类型信息和分支预测数据。
3.  **E-Graph Saturation (等价饱和)**: 将意图流注入 `UniversalOptimizer`，在 E-Graph 中进行代数化简、公共表达式消除 (CSE) 等优化。
4.  **Instruction Selection (指令选择)**: 根据目标后端的指令集特征，从饱和后的 E-Graph 中提取最优 `IKunTree`。
5.  **Machine Code Generation (机器码生成)**: 调用 `Gaia` 后端将 `IKunTree` 降级为特定架构（如 x86_64, ARM64）的二进制指令。
6.  **Secure Execution (安全执行)**: 将生成的机器码写入 `JitMemory`，应用 W^X 安全策略后跳转执行。

## 现代 JIT 技术挑战与应对

为了应对高性能动态语言的需求，`nyar-jit` 引入了多项前沿技术：

### 1. 推测性优化与去优化 (Speculative Optimization & Deoptimization)
-   **趋势**: 动态语言往往缺乏静态类型。JIT 编译器会根据历史运行数据假设变量类型（推测）。
-   **机制**: 如果推测成功，代码运行极快；如果运行中发现假设失败（如变量类型改变），则触发 **Deoptimization**，安全地回退到解释器状态并重新编译。

### 2. 内联缓存 (Inline Caching)
-   **挑战**: 虚函数调用或动态分发（Dynamic Dispatch）是性能杀手。
-   **优化**: `nyar-jit` 使用单态或多态内联缓存。在机器码中直接嵌入最近使用的目标地址，将间接跳转转化为直接跳转甚至内联。

### 3. 栈上替换 (On-Stack Replacement, OSR)
-   **场景**: 对于一个运行时间极长的死循环，如果仅在函数入口触发 JIT，则该循环永远无法享受到优化。
-   **解决**: OSR 允许在函数运行中途（通常在循环回跳点）将执行权从解释器无缝移交给 JIT 生成的机器码。

### 4. 协程与代数效应优化 (Coroutines & Effects Optimization)

NyarVM 原生支持代数效应（Algebraic Effects），这对 JIT 提出了更高要求。`nyar-jit` 针对这一领域实现了深度优化：

-   **效应内联 (Effect Inlining)**: 通过静态分析或内联缓存（IC）识别特定的 Effect Handler。如果 Handler 是确定的，JIT 会将其逻辑直接内联到 `raise` 调用点，消除动态分发开销。
-   **延续对象的标量替换 (Scalar Replacement of Continuations)**: 对于不逃逸当前作用域的协程延续（Continuation），JIT 会将其状态拆散并分配到寄存器中，避免在堆上创建延续对象。
-   **尾调用优化 (Tail-Call Optimization)**: 许多效应模式（如状态传递）本质上是递归的。`nyar-jit` 确保这些调用被转换为跳转，防止栈溢出并提升性能。
-   **栈图与精确追踪**: 为了支持多跳延续（Multi-shot Continuations），JIT 生成的机器码包含精确的栈图（Stack Maps），确保 GC 在协程挂起和恢复时能准确识别根集合。

### 5. 类型特化与运行时单态化 (Type Specialization & Late Monomorphization)

`nyar-jit` 选择了 **运行时单态化 (Late Monomorphization)** 而非字节码级的强行实例化，这是基于以下核心考量：

-   **抑制字节码体积爆炸**: 强行在 AOT 或字节码生成阶段进行单态化会导致代码体积呈几何倍数增长。通过 JIT 进行“按需实例化”，可以在保持字节码精简（Poly-morphic Bytecode）的同时，获得原生级别的执行性能。
-   **支持高级类型特性 (HKT)**: 对于高阶类型（Higher-Kinded Types）等复杂抽象，静态单态化往往难以处理。JIT 能够利用运行时的具体类型信息，对这些高度抽象的路径进行特化，从而支持更强大的类型系统。
-   **消除装箱开销**: JIT 会根据具体类型生成专门的指令序列，彻底消除通用泛型代码中的装箱（Boxing）、拆箱及虚表查找开销。
-   **去虚化 (Devirtualization)**: 通过分析类层次结构（CHA）或追踪单态调用点，将虚函数调用（Virtual Calls）转化为直接的机器指令跳转，并进一步实现内联。

### 6. Effect 实例化与 Await 内联 (Effect Instantiation & Await Inlining)

针对 NyarVM 的核心特性，JIT 进行了指令级的深度定制：

-   **Await 点内联 (Await Inlining)**: JIT 会尝试分析 `await` 目标的确定性。对于已就绪（Ready）的任务或可预测的异步流，JIT 会直接将异步等待逻辑展开为顺序执行代码，消除协程挂起与恢复的上下文切换开销。
-   **Effect 路径特化**: 当一个 Effect 被频繁 raise 且对应的 Handler 处于固定上下文时，JIT 会将整个 Effect 处理路径实例化为一段紧凑的直接跳转序列，使代数效应的性能接近普通的函数调用。
-   **协程状态机展开**: JIT 会将基于状态机的协程逻辑重新构造为扁平的控制流图，利用 CPU 的分支预测器优化原本复杂的状态跳转。

## 前瞻性：GC 与 JIT 的深度协同 (GC-JIT Co-optimization)

现代虚拟机（如 V8, HotSpot, .NET）的演进趋势不再是将 GC 和 JIT 视为独立的组件，而是实现深度的“共生”优化。`nyar-vm` 正在探索以下前沿方向：

### 1. 乐观栈分配与动态堆化 (Optimistic Stack Allocation)
传统的逃逸分析（Escape Analysis）是静态且保守的。前沿研究（如 *CGO 2024* 相关论文）提出：
-   **推测性分配**: JIT 默认将大部分短生命周期对象分配在栈上，即使无法 100% 确定其不逃逸。
-   **动态触发**: 在写入长寿命对象时触发一个特殊的“写屏障”，如果发现目标是栈上对象，则实时将其搬移（Heapification）到托管堆中。这极大提升了内存密集型代码的吞吐量。

### 2. 屏障消除与指令特化 (Barrier Elision)
写屏障（Write Barrier）是维持分代 GC 不变性的必要开销，但并非所有写入都需要屏障：
-   **新生代证明**: 如果 JIT 能证明某个写入操作的目标对象是刚刚分配的（尚未经历过任何 GC 周期），则可以完全消除写屏障指令。
-   **硬件加速屏障**: 利用现代 CPU 特性（如 ARM64 的特定内存顺序指令或 Intel MPK），将 GC 标记位的检查降级为单条硬件指令。

### 3. 分配下沉 (Allocation Sinking)
借鉴自 *LuaJIT* 的高级优化：
-   JIT 并不立即分配对象，而是记录对象的创建意图。
-   如果对象仅在当前热点路径内部使用，JIT 会将其字段“散开”到寄存器中（标量替换）。
-   只有当对象真正进入非热点路径（如抛出异常或调用外部 FFI）时，才按需在堆上恢复（Materialize）该对象。

### 4. 协同安全点 (Cooperative Safepoints)
-   **零成本安全点**: 利用内存页保护（Page Fault）或特殊的跳转指令，使得在没有 GC 请求时，代码运行开销为零；仅在 GC 需要挂起线程时才动态激活安全点检测。

## 与 Gaia 引擎的集成

`nyar-jit` 并不直接操作硬件，而是通过 `gaia-jit` 抽象层实现跨平台：

-   **W^X 内存管理**: 严格遵循“不可同时读写与执行”的原则。内存页在写入时为可写不可执行，在执行前切换为只读且可执行。
-   **架构感知**: `Gaia` 提供了丰富的指令描述符，使得 `nyar-jit` 可以利用现代 CPU 的 SIMD、AVX-512 等高级指令特性。

## 开发调试

在调试 JIT 生成的代码时，可以使用以下环境变量：

- `NYAR_LOG_JIT=1`: 打印 JIT 编译过程的详细日志（包括 E-Graph 状态）。
- `NYAR_DUMP_ASM=1`: 将生成的机器码反汇编并保存到本地。
- `NYAR_JIT_THRESHOLD=N`: 手动调整触发 JIT 编译的热点阈值。
