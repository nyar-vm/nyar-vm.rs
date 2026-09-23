# 芯片设计

Valkyrie 提供了类似于 Scala Chisel 的硬件描述语言 (HDL) 功能，支持数字电路设计、FPGA 开发、ASIC 设计等。通过类型安全的硬件抽象和强大的元编程能力，Valkyrie 为现代芯片设计提供了高效的开发环境。

## 核心概念

### 硬件数据类型

```valkyrie
# 基础硬件类型
type UInt<const W: usize> = HardwareType<u64, W>  # 无符号整数
type SInt<const W: usize> = HardwareType<i64, W>  # 有符号整数
type Bool = UInt<1>                               # 布尔类型
type Clock = HardwareSignal                       # 时钟信号
type Reset = HardwareSignal                       # 复位信号

# 硬件向量
type Vec<T, const N: usize> = [T; N]

# 硬件束 (Bundle)
trait Bundle {
    type Elements
    micro elements(self) -> Self::Elements
}

# 内存类型
struct Mem<T, const DEPTH: usize> {
    data: PhantomData<T>,
    depth: PhantomData<[(); DEPTH]>
}

# 寄存器类型
struct Reg<T> {
    data: T,
    clock: Clock,
    reset: Option<Reset>
}
```

### 模块定义

```valkyrie
# 硬件模块特征
trait Module {
    type IO: Bundle
    
    micro io(self) -> Self::IO
    micro elaborate(self)
}

# 简单的加法器模块
struct Adder {
    width: usize
}

struct AdderIO {
    a: UInt<32>,
    b: UInt<32>,
    sum: UInt<33>
}

imply AdderIO: Bundle {
    type Elements = (UInt<32>, UInt<32>, UInt<33>)
    
    micro elements(self) -> Self::Elements {
        (self.a, self.b, self.sum)
    }
}

imply Adder: Module {
    type IO = AdderIO
    
    micro io(self) -> AdderIO {
        AdderIO {
            a: input(UInt::<32>),
            b: input(UInt::<32>),
            sum: output(UInt::<33>)
        }
    }
    
    micro elaborate(self) {
        let io = self.io()
        io.sum := io.a + io.b
    }
}
```

### 时序逻辑

```valkyrie
# 寄存器操作
micro reg<T>(init: T, clock: Clock) -> Reg<T> {
    Reg {
        data: init,
        clock,
        reset: None
    }
}

micro reg_with_reset<T>(init: T, clock: Clock, reset: Reset, reset_value: T) -> Reg<T> {
    Reg {
        data: init,
        clock,
        reset: Some(reset)
    }
}

# 计数器模块
struct Counter {
    width: usize,
    max_count: u64
}

struct CounterIO {
    enable: Bool,
    reset: Bool,
    count: UInt<32>,
    overflow: Bool
}

imply CounterIO: Bundle {
    type Elements = (Bool, Bool, UInt<32>, Bool)
    
    micro elements(self) -> Self::Elements {
        (self.enable, self.reset, self.count, self.overflow)
    }
}

imply Counter: Module {
    type IO = CounterIO
    
    micro io(self) -> CounterIO {
        CounterIO {
            enable: input(Bool),
            reset: input(Bool),
            count: output(UInt::<32>),
            overflow: output(Bool)
        }
    }
    
    micro elaborate(self) {
        let io = self.io()
        let clock = Clock::global()
        
        let count_reg = reg_with_reset(
            UInt::<32>::from(0),
            clock,
            io.reset,
            UInt::<32>::from(0)
        )
        
        let next_count = mux(
            io.enable,
            mux(
                count_reg.data === UInt::<32>::from(self.max_count),
                UInt::<32>::from(0),
                count_reg.data + UInt::<32>::from(1)
            ),
            count_reg.data
        )
        
        count_reg.data := next_count
        io.count := count_reg.data
        io.overflow := io.enable && (count_reg.data === UInt::<32>::from(self.max_count))
    }
}
```

## 组合逻辑

### 基本操作符

```valkyrie
# 逻辑操作
micro and<const W: usize>(a: UInt<W>, b: UInt<W>) -> UInt<W> {
    a & b
}

micro or<const W: usize>(a: UInt<W>, b: UInt<W>) -> UInt<W> {
    a | b
}

micro xor<const W: usize>(a: UInt<W>, b: UInt<W>) -> UInt<W> {
    a ^ b
}

micro not<const W: usize>(a: UInt<W>) -> UInt<W> {
    !a
}

# 算术操作
micro add<const W: usize>(a: UInt<W>, b: UInt<W>) -> UInt<W+1> {
    a + b
}

micro sub<const W: usize>(a: UInt<W>, b: UInt<W>) -> UInt<W+1> {
    a - b
}

micro mul<const W1: usize, const W2: usize>(a: UInt<W1>, b: UInt<W2>) -> UInt<W1+W2> {
    a * b
}

# 比较操作
micro eq<const W: usize>(a: UInt<W>, b: UInt<W>) -> Bool {
    a === b
}

micro lt<const W: usize>(a: UInt<W>, b: UInt<W>) -> Bool {
    a < b
}

micro gt<const W: usize>(a: UInt<W>, b: UInt<W>) -> Bool {
    a > b
}

# 多路选择器
micro mux<T>(sel: Bool, true_val: T, false_val: T) -> T {
    if sel { true_val } else { false_val }
}

micro mux_lookup<T, const N: usize>(sel: UInt<log2(N)>, values: [T; N]) -> T {
    values[sel.as_index()]
}
```

### ALU 设计

```valkyrie
# ALU 操作码
enum AluOp {
    Add = 0b0000,
    Sub = 0b0001,
    And = 0b0010,
    Or  = 0b0011,
    Xor = 0b0100,
    Sll = 0b0101,  # 逻辑左移
    Srl = 0b0110,  # 逻辑右移
    Sra = 0b0111,  # 算术右移
    Lt  = 0b1000,  # 小于比较
    Ltu = 0b1001   # 无符号小于比较
}

# ALU 模块
struct Alu {
    width: usize
}

struct AluIO {
    a: UInt<32>,
    b: UInt<32>,
    op: UInt<4>,
    result: UInt<32>,
    zero: Bool,
    overflow: Bool
}

imply AluIO: Bundle {
    type Elements = (UInt<32>, UInt<32>, UInt<4>, UInt<32>, Bool, Bool)
    
    micro elements(self) -> Self::Elements {
        (self.a, self.b, self.op, self.result, self.zero, self.overflow)
    }
}

imply Alu: Module {
    type IO = AluIO
    
    micro io(self) -> AluIO {
        AluIO {
            a: input(UInt::<32>),
            b: input(UInt::<32>),
            op: input(UInt::<4>),
            result: output(UInt::<32>),
            zero: output(Bool),
            overflow: output(Bool)
        }
    }
    
    micro elaborate(self) {
        let io = self.io()
        
        let add_result = io.a + io.b
        let sub_result = io.a - io.b
        let and_result = io.a & io.b
        let or_result = io.a | io.b
        let xor_result = io.a ^ io.b
        let sll_result = io.a << io.b[4:0]
        let srl_result = io.a >> io.b[4:0]
        let sra_result = (io.a.as_sint()) >> io.b[4:0]
        let lt_result = mux(io.a.as_sint() < io.b.as_sint(), UInt::<32>::from(1), UInt::<32>::from(0))
        let ltu_result = mux(io.a < io.b, UInt::<32>::from(1), UInt::<32>::from(0))
        
        io.result := mux_lookup(io.op, [
            add_result[31:0],
            sub_result[31:0],
            and_result,
            or_result,
            xor_result,
            sll_result,
            srl_result,
            sra_result.as_uint(),
            lt_result,
            ltu_result,
            UInt::<32>::from(0),  # 未使用的操作码
            UInt::<32>::from(0),
            UInt::<32>::from(0),
            UInt::<32>::from(0),
            UInt::<32>::from(0),
            UInt::<32>::from(0)
        ])
        
        io.zero := io.result === UInt::<32>::from(0)
        
        # 溢出检测（仅对加法和减法）
        let add_overflow = (io.a[31] === io.b[31]) && (io.result[31] =/= io.a[31])
        let sub_overflow = (io.a[31] =/= io.b[31]) && (io.result[31] =/= io.a[31])
        
        io.overflow := mux(
            io.op === UInt::<4>::from(AluOp::Add as u8),
            add_overflow,
            mux(
                io.op === UInt::<4>::from(AluOp::Sub as u8),
                sub_overflow,
                Bool::from(false)
            )
        )
    }
}
```

## 内存系统

### 单端口内存

```valkyrie
# 单端口 RAM
struct SinglePortRam<const WIDTH: usize, const DEPTH: usize> {
    _phantom: PhantomData<(UInt<WIDTH>, [(); DEPTH])>
}

struct SinglePortRamIO<const WIDTH: usize, const ADDR_WIDTH: usize> {
    addr: UInt<ADDR_WIDTH>,
    data_in: UInt<WIDTH>,
    data_out: UInt<WIDTH>,
    write_enable: Bool,
    chip_enable: Bool
}

imply<const WIDTH: usize, const ADDR_WIDTH: usize> SinglePortRamIO<WIDTH, ADDR_WIDTH>: Bundle {
    type Elements = (UInt<ADDR_WIDTH>, UInt<WIDTH>, UInt<WIDTH>, Bool, Bool)
    
    micro elements(self) -> Self::Elements {
        (self.addr, self.data_in, self.data_out, self.write_enable, self.chip_enable)
    }
}

imply<const WIDTH: usize, const DEPTH: usize> SinglePortRam<WIDTH, DEPTH>: Module {
    type IO = SinglePortRamIO<WIDTH, {log2_ceil(DEPTH)}>
    
    micro io(self) -> Self::IO {
        SinglePortRamIO {
            addr: input(UInt::<{log2_ceil(DEPTH)}>),
            data_in: input(UInt::<WIDTH>),
            data_out: output(UInt::<WIDTH>),
            write_enable: input(Bool),
            chip_enable: input(Bool)
        }
    }
    
    micro elaborate(self) {
        let io = self.io()
        let clock = Clock::global()
        
        let mem = Mem::<UInt<WIDTH>, DEPTH>::new()
        
        # 写操作
        when(io.chip_enable && io.write_enable) {
            mem.write(io.addr, io.data_in)
        }
        
        # 读操作
        io.data_out := mux(
            io.chip_enable,
            mem.read(io.addr),
            UInt::<WIDTH>::from(0)
        )
    }
}
```

### 双端口内存

```valkyrie
# 双端口 RAM
struct DualPortRam<const WIDTH: usize, const DEPTH: usize> {
    _phantom: PhantomData<(UInt<WIDTH>, [(); DEPTH])>
}

struct DualPortRamIO<const WIDTH: usize, const ADDR_WIDTH: usize> {
    # 端口 A
    addr_a: UInt<ADDR_WIDTH>,
    data_in_a: UInt<WIDTH>,
    data_out_a: UInt<WIDTH>,
    write_enable_a: Bool,
    chip_enable_a: Bool,
    
    # 端口 B
    addr_b: UInt<ADDR_WIDTH>,
    data_in_b: UInt<WIDTH>,
    data_out_b: UInt<WIDTH>,
    write_enable_b: Bool,
    chip_enable_b: Bool
}

imply<const WIDTH: usize, const ADDR_WIDTH: usize> DualPortRamIO<WIDTH, ADDR_WIDTH>: Bundle {
    type Elements = (
        UInt<ADDR_WIDTH>, UInt<WIDTH>, UInt<WIDTH>, Bool, Bool,
        UInt<ADDR_WIDTH>, UInt<WIDTH>, UInt<WIDTH>, Bool, Bool
    )
    
    micro elements(self) -> Self::Elements {
        (
            self.addr_a, self.data_in_a, self.data_out_a, self.write_enable_a, self.chip_enable_a,
            self.addr_b, self.data_in_b, self.data_out_b, self.write_enable_b, self.chip_enable_b
        )
    }
}

imply<const WIDTH: usize, const DEPTH: usize> DualPortRam<WIDTH, DEPTH>: Module {
    type IO = DualPortRamIO<WIDTH, {log2_ceil(DEPTH)}>
    
    micro io(self) -> Self::IO {
        DualPortRamIO {
            addr_a: input(UInt::<{log2_ceil(DEPTH)}>),
            data_in_a: input(UInt::<WIDTH>),
            data_out_a: output(UInt::<WIDTH>),
            write_enable_a: input(Bool),
            chip_enable_a: input(Bool),
            
            addr_b: input(UInt::<{log2_ceil(DEPTH)}>),
            data_in_b: input(UInt::<WIDTH>),
            data_out_b: output(UInt::<WIDTH>),
            write_enable_b: input(Bool),
            chip_enable_b: input(Bool)
        }
    }
    
    micro elaborate(self) {
        let io = self.io()
        let clock = Clock::global()
        
        let mem = Mem::<UInt<WIDTH>, DEPTH>::new()
        
        # 端口 A 操作
        when(io.chip_enable_a && io.write_enable_a) {
            mem.write(io.addr_a, io.data_in_a)
        }
        
        io.data_out_a := mux(
            io.chip_enable_a,
            mem.read(io.addr_a),
            UInt::<WIDTH>::from(0)
        )
        
        # 端口 B 操作
        when(io.chip_enable_b && io.write_enable_b) {
            mem.write(io.addr_b, io.data_in_b)
        }
        
        io.data_out_b := mux(
            io.chip_enable_b,
            mem.read(io.addr_b),
            UInt::<WIDTH>::from(0)
        )
    }
}
```

## 处理器设计

### 简单的 RISC-V 核心

```valkyrie
# RISC-V 指令格式
enum InstructionType {
    RType,  # 寄存器-寄存器操作
    IType,  # 立即数操作
    SType,  # 存储操作
    BType,  # 分支操作
    UType,  # 上位立即数
    JType   # 跳转操作
}

# 指令解码器
struct InstructionDecoder {
    _phantom: PhantomData<()>
}

struct DecodedInstruction {
    opcode: UInt<7>,
    rd: UInt<5>,
    rs1: UInt<5>,
    rs2: UInt<5>,
    funct3: UInt<3>,
    funct7: UInt<7>,
    imm: UInt<32>,
    inst_type: UInt<3>
}

imply DecodedInstruction: Bundle {
    type Elements = (UInt<7>, UInt<5>, UInt<5>, UInt<5>, UInt<3>, UInt<7>, UInt<32>, UInt<3>)
    
    micro elements(self) -> Self::Elements {
        (self.opcode, self.rd, self.rs1, self.rs2, self.funct3, self.funct7, self.imm, self.inst_type)
    }
}

struct InstructionDecoderIO {
    instruction: UInt<32>,
    decoded: DecodedInstruction
}

imply InstructionDecoderIO: Bundle {
    type Elements = (UInt<32>, DecodedInstruction)
    
    micro elements(self) -> Self::Elements {
        (self.instruction, self.decoded)
    }
}

imply InstructionDecoder: Module {
    type IO = InstructionDecoderIO
    
    micro io(self) -> InstructionDecoderIO {
        InstructionDecoderIO {
            instruction: input(UInt::<32>),
            decoded: output(DecodedInstruction::default())
        }
    }
    
    micro elaborate(self) {
        let io = self.io()
        let inst = io.instruction
        
        io.decoded.opcode := inst[6:0]
        io.decoded.rd := inst[11:7]
        io.decoded.rs1 := inst[19:15]
        io.decoded.rs2 := inst[24:20]
        io.decoded.funct3 := inst[14:12]
        io.decoded.funct7 := inst[31:25]
        
        # 立即数生成
        let i_imm = cat(fill(inst[31], 21), inst[30:20])
        let s_imm = cat(fill(inst[31], 21), inst[30:25], inst[11:7])
        let b_imm = cat(fill(inst[31], 20), inst[7], inst[30:25], inst[11:8], UInt::<1>::from(0))
        let u_imm = cat(inst[31:12], UInt::<12>::from(0))
        let j_imm = cat(fill(inst[31], 12), inst[19:12], inst[20], inst[30:21], UInt::<1>::from(0))
        
        # 指令类型检测
        let is_r_type = (inst[6:0] === UInt::<7>::from(0b0110011))
        let is_i_type = (inst[6:0] === UInt::<7>::from(0b0010011)) || (inst[6:0] === UInt::<7>::from(0b0000011))
        let is_s_type = (inst[6:0] === UInt::<7>::from(0b0100011))
        let is_b_type = (inst[6:0] === UInt::<7>::from(0b1100011))
        let is_u_type = (inst[6:0] === UInt::<7>::from(0b0110111)) || (inst[6:0] === UInt::<7>::from(0b0010111))
        let is_j_type = (inst[6:0] === UInt::<7>::from(0b1101111))
        
        io.decoded.inst_type := mux_lookup(cat(is_j_type, is_u_type, is_b_type, is_s_type, is_i_type, is_r_type), [
            UInt::<3>::from(0),  # 无效
            UInt::<3>::from(InstructionType::RType as u8),
            UInt::<3>::from(InstructionType::IType as u8),
            UInt::<3>::from(0),  # R+I 不可能
            UInt::<3>::from(InstructionType::SType as u8),
            UInt::<3>::from(0),  # R+S 不可能
            UInt::<3>::from(0),  # I+S 不可能
            UInt::<3>::from(0),  # R+I+S 不可能
            UInt::<3>::from(InstructionType::BType as u8),
            # ... 其他组合
        ])
        
        io.decoded.imm := mux_lookup(io.decoded.inst_type, [
            UInt::<32>::from(0),  # 无效
            UInt::<32>::from(0),  # R-type 无立即数
            i_imm,                # I-type
            s_imm,                # S-type
            b_imm,                # B-type
            u_imm,                # U-type
            j_imm,                # J-type
            UInt::<32>::from(0)   # 保留
        ])
    }
}
```

### 寄存器文件

```valkyrie
# 32 个 32 位寄存器的寄存器文件
struct RegisterFile {
    _phantom: PhantomData<()>
}

struct RegisterFileIO {
    # 读端口 1
    rs1_addr: UInt<5>,
    rs1_data: UInt<32>,
    
    # 读端口 2
    rs2_addr: UInt<5>,
    rs2_data: UInt<32>,
    
    # 写端口
    rd_addr: UInt<5>,
    rd_data: UInt<32>,
    rd_enable: Bool
}

imply RegisterFileIO: Bundle {
    type Elements = (UInt<5>, UInt<32>, UInt<5>, UInt<32>, UInt<5>, UInt<32>, Bool)
    
    micro elements(self) -> Self::Elements {
        (self.rs1_addr, self.rs1_data, self.rs2_addr, self.rs2_data, self.rd_addr, self.rd_data, self.rd_enable)
    }
}

imply RegisterFile: Module {
    type IO = RegisterFileIO
    
    micro io(self) -> RegisterFileIO {
        RegisterFileIO {
            rs1_addr: input(UInt::<5>),
            rs1_data: output(UInt::<32>),
            rs2_addr: input(UInt::<5>),
            rs2_data: output(UInt::<32>),
            rd_addr: input(UInt::<5>),
            rd_data: input(UInt::<32>),
            rd_enable: input(Bool)
        }
    }
    
    micro elaborate(self) {
        let io = self.io()
        let clock = Clock::global()
        
        # 32 个寄存器，x0 始终为 0
        let registers = Vec::<Reg<UInt<32>>, 32>::new(|i| {
            reg_with_reset(
                UInt::<32>::from(0),
                clock,
                Reset::global(),
                UInt::<32>::from(0)
            )
        })
        
        # 写操作（x0 不可写）
        for i in 1..32 {
            when(io.rd_enable && (io.rd_addr === UInt::<5>::from(i))) {
                registers[i].data := io.rd_data
            }
        }
        
        # 读操作
        io.rs1_data := mux(
            io.rs1_addr === UInt::<5>::from(0),
            UInt::<32>::from(0),
            mux_lookup(io.rs1_addr, registers.map(|r| r.data))
        )
        
        io.rs2_data := mux(
            io.rs2_addr === UInt::<5>::from(0),
            UInt::<32>::from(0),
            mux_lookup(io.rs2_addr, registers.map(|r| r.data))
        )
    }
}
```

## 总线和互连

### AXI4 总线接口

```valkyrie
# AXI4 写地址通道
struct Axi4WriteAddress {
    awid: UInt<4>,      # 写事务 ID
    awaddr: UInt<32>,   # 写地址
    awlen: UInt<8>,     # 突发长度
    awsize: UInt<3>,    # 突发大小
    awburst: UInt<2>,   # 突发类型
    awlock: Bool,       # 锁定类型
    awcache: UInt<4>,   # 缓存类型
    awprot: UInt<3>,    # 保护类型
    awqos: UInt<4>,     # 服务质量
    awregion: UInt<4>,  # 区域标识
    awvalid: Bool,      # 写地址有效
    awready: Bool       # 写地址就绪
}

# AXI4 写数据通道
struct Axi4WriteData {
    wdata: UInt<32>,    # 写数据
    wstrb: UInt<4>,     # 写字节选通
    wlast: Bool,        # 写最后一个
    wvalid: Bool,       # 写数据有效
    wready: Bool        # 写数据就绪
}

# AXI4 写响应通道
struct Axi4WriteResponse {
    bid: UInt<4>,       # 响应 ID
    bresp: UInt<2>,     # 写响应
    bvalid: Bool,       # 写响应有效
    bready: Bool        # 写响应就绪
}

# AXI4 读地址通道
struct Axi4ReadAddress {
    arid: UInt<4>,      # 读事务 ID
    araddr: UInt<32>,   # 读地址
    arlen: UInt<8>,     # 突发长度
    arsize: UInt<3>,    # 突发大小
    arburst: UInt<2>,   # 突发类型
    arlock: Bool,       # 锁定类型
    arcache: UInt<4>,   # 缓存类型
    arprot: UInt<3>,    # 保护类型
    arqos: UInt<4>,     # 服务质量
    arregion: UInt<4>,  # 区域标识
    arvalid: Bool,      # 读地址有效
    arready: Bool       # 读地址就绪
}

# AXI4 读数据通道
struct Axi4ReadData {
    rid: UInt<4>,       # 读数据 ID
    rdata: UInt<32>,    # 读数据
    rresp: UInt<2>,     # 读响应
    rlast: Bool,        # 读最后一个
    rvalid: Bool,       # 读数据有效
    rready: Bool        # 读数据就绪
}

# 完整的 AXI4 接口
struct Axi4Interface {
    write_address: Axi4WriteAddress,
    write_data: Axi4WriteData,
    write_response: Axi4WriteResponse,
    read_address: Axi4ReadAddress,
    read_data: Axi4ReadData
}

imply Axi4Interface: Bundle {
    type Elements = (
        Axi4WriteAddress,
        Axi4WriteData,
        Axi4WriteResponse,
        Axi4ReadAddress,
        Axi4ReadData
    )
    
    micro elements(self) -> Self::Elements {
        (
            self.write_address,
            self.write_data,
            self.write_response,
            self.read_address,
            self.read_data
        )
    }
}
```

### 交叉开关 (Crossbar)

```valkyrie
# N 主设备到 M 从设备的交叉开关
struct Crossbar<const N_MASTERS: usize, const M_SLAVES: usize> {
    _phantom: PhantomData<([(); N_MASTERS], [(); M_SLAVES])>
}

struct CrossbarIO<const N_MASTERS: usize, const M_SLAVES: usize> {
    master_ports: [Axi4Interface; N_MASTERS],
    slave_ports: [Axi4Interface; M_SLAVES]
}

imply<const N_MASTERS: usize, const M_SLAVES: usize> CrossbarIO<N_MASTERS, M_SLAVES>: Bundle {
    type Elements = ([Axi4Interface; N_MASTERS], [Axi4Interface; M_SLAVES])
    
    micro elements(self) -> Self::Elements {
        (self.master_ports, self.slave_ports)
    }
}

imply<const N_MASTERS: usize, const M_SLAVES: usize> Crossbar<N_MASTERS, M_SLAVES>: Module {
    type IO = CrossbarIO<N_MASTERS, M_SLAVES>
    
    micro io(self) -> Self::IO {
        CrossbarIO {
            master_ports: [Axi4Interface::input(); N_MASTERS],
            slave_ports: [Axi4Interface::output(); M_SLAVES]
        }
    }
    
    micro elaborate(self) {
        let io = self.io()
        
        # 地址解码器
        let address_decoders = Vec::<AddressDecoder, N_MASTERS>::new(|_| {
            AddressDecoder::new(M_SLAVES)
        })
        
        # 仲裁器
        let arbiters = Vec::<Arbiter<N_MASTERS>, M_SLAVES>::new(|_| {
            Arbiter::new()
        })
        
        # 连接主设备到从设备
        for master_id in 0..N_MASTERS {
            let master = &io.master_ports[master_id]
            let decoder = &address_decoders[master_id]
            
            # 解码读地址
            decoder.decode_address(master.read_address.araddr)
            
            for slave_id in 0..M_SLAVES {
                let slave = &io.slave_ports[slave_id]
                let arbiter = &arbiters[slave_id]
                
                # 仲裁逻辑
                arbiter.request[master_id] := decoder.slave_select[slave_id] && master.read_address.arvalid
                
                when(arbiter.grant[master_id]) {
                    # 连接读通道
                    slave.read_address <> master.read_address
                    master.read_data <> slave.read_data
                    
                    # 连接写通道
                    slave.write_address <> master.write_address
                    slave.write_data <> master.write_data
                    master.write_response <> slave.write_response
                }
            }
        }
    }
}
```

## 验证和测试

### 测试平台

```valkyrie
# 测试基础设施
trait Testbench {
    type DUT: Module  # 被测设计
    
    micro setup(self) -> Self::DUT
    micro run_test(self, dut: Self::DUT)
    micro cleanup(self)
}

# 时钟生成器
struct ClockGenerator {
    period: u64,
    duty_cycle: f32
}

impl ClockGenerator {
    micro new(frequency_mhz: f32) -> ClockGenerator {
        ClockGenerator {
            period: (1000.0 / frequency_mhz) as u64,
            duty_cycle: 0.5
        }
    }
    
    micro generate_clock(self) -> Clock {
        let clock = Clock::new()
        
        # 时钟生成逻辑
        fork {
            loop {
                clock.set_high()
                delay(self.period * self.duty_cycle as u64)
                clock.set_low()
                delay(self.period * (1.0 - self.duty_cycle) as u64)
            }
        }
        
        clock
    }
}

# ALU 测试平台
struct AluTestbench {
    clock_gen: ClockGenerator,
    test_vectors: Vec<AluTestVector>
}

struct AluTestVector {
    a: u32,
    b: u32,
    op: u8,
    expected_result: u32,
    expected_zero: bool,
    expected_overflow: bool
}

imply AluTestbench: Testbench {
    type DUT = Alu
    
    micro setup(self) -> Alu {
        Alu { width: 32 }
    }
    
    micro run_test(self, dut: Alu) {
        let clock = self.clock_gen.generate_clock()
        let io = dut.io()
        
        # 复位
        let reset = Reset::new()
        reset.assert()
        clock.tick(5)
        reset.deassert()
        
        # 运行测试向量
        for test_vector in self.test_vectors {
            io.a := UInt::<32>::from(test_vector.a)
            io.b := UInt::<32>::from(test_vector.b)
            io.op := UInt::<4>::from(test_vector.op)
            
            clock.tick(1)
            
            # 检查结果
            assert_eq!(io.result.value(), test_vector.expected_result, "ALU result mismatch")
            assert_eq!(io.zero.value(), test_vector.expected_zero, "ALU zero flag mismatch")
            assert_eq!(io.overflow.value(), test_vector.expected_overflow, "ALU overflow flag mismatch")
        }
        
        print("ALU test passed with {} test vectors", self.test_vectors.len())
    }
    
    micro cleanup(self) {
        # 清理资源
    }
}
```

### 形式化验证

```valkyrie
# 形式化验证属性
trait FormalProperty {
    micro assume(self, condition: Bool)
    micro assert(self, property: Bool)
    micro cover(self, condition: Bool)
}

# ALU 形式化验证
struct AluFormalVerification {
    dut: Alu
}

imply AluFormalVerification: FormalProperty {
    micro assume(self, condition: Bool) {
        # 假设条件
        formal_assume(condition)
    }
    
    micro assert(self, property: Bool) {
        # 断言属性
        formal_assert(property)
    }
    
    micro cover(self, condition: Bool) {
        # 覆盖条件
        formal_cover(condition)
    }
}

impl AluFormalVerification {
    micro verify_add_properties(self) {
        let io = self.dut.io()
        
        # 假设：操作码为加法
        self.assume(io.op === UInt::<4>::from(AluOp::Add as u8))
        
        # 属性 1：加法交换律
        let result1 = io.result
        io.a := io.b
        io.b := io.a
        let result2 = io.result
        self.assert(result1 === result2)
        
        # 属性 2：加零等于自身
        io.b := UInt::<32>::from(0)
        self.assert(io.result === io.a)
        
        # 属性 3：溢出检测
        let max_val = UInt::<32>::from(0xFFFFFFFF)
        io.a := max_val
        io.b := UInt::<32>::from(1)
        self.assert(io.overflow === Bool::from(true))
    }
    
    micro verify_all_operations(self) {
        let io = self.dut.io()
        
        # 覆盖所有操作码
        for op in 0..16 {
            io.op := UInt::<4>::from(op)
            self.cover(Bool::from(true))
        }
        
        # 边界值测试
        let boundary_values = [0, 1, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF]
        
        for &val in boundary_values {
            io.a := UInt::<32>::from(val)
            io.b := UInt::<32>::from(val)
            self.cover(Bool::from(true))
        }
    }
}
```

## 综合和实现

### 综合约束

```valkyrie
# 时序约束
struct TimingConstraints {
    clock_period: f32,      # 时钟周期 (ns)
    setup_time: f32,        # 建立时间 (ns)
    hold_time: f32,         # 保持时间 (ns)
    clock_skew: f32,        # 时钟偏斜 (ns)
    input_delay: f32,       # 输入延迟 (ns)
    output_delay: f32       # 输出延迟 (ns)
}

# 面积约束
struct AreaConstraints {
    max_area: u32,          # 最大面积 (gate equivalent)
    max_memory: u32,        # 最大内存 (bits)
    max_dsp: u32            # 最大 DSP 块数量
}

# 功耗约束
struct PowerConstraints {
    max_static_power: f32,  # 最大静态功耗 (mW)
    max_dynamic_power: f32, # 最大动态功耗 (mW)
    voltage: f32,           # 工作电压 (V)
    temperature: f32        # 工作温度 (°C)
}

# 综合配置
struct SynthesisConfig {
    target_technology: String,
    optimization_goal: OptimizationGoal,
    timing_constraints: TimingConstraints,
    area_constraints: AreaConstraints,
    power_constraints: PowerConstraints
}

enum OptimizationGoal {
    Speed,      # 优化速度
    Area,       # 优化面积
    Power,      # 优化功耗
    Balanced    # 平衡优化
}

# 综合属性注解
↯synthesis(clock_gating = true)
↯synthesis(retiming = true)
↯synthesis(resource_sharing = true)
struct OptimizedProcessor {
    config: SynthesisConfig
}

imply OptimizedProcessor: Module {
    type IO = ProcessorIO
    
    micro io(self) -> ProcessorIO {
        ProcessorIO::new()
    }
    
    ↯synthesis(pipeline_stages = 5)
    micro elaborate(self) {
        # 流水线处理器实现
        let fetch_stage = FetchStage::new()
        let decode_stage = DecodeStage::new()
        let execute_stage = ExecuteStage::new()
        let memory_stage = MemoryStage::new()
        let writeback_stage = WritebackStage::new()
        
        # 流水线连接
        decode_stage.input <> fetch_stage.output
        execute_stage.input <> decode_stage.output
        memory_stage.input <> execute_stage.output
        writeback_stage.input <> memory_stage.output
    }
}
```

## 文档链接

- [硬件描述语言基础](hdl-basics.md) - Valkyrie HDL 语法和基本概念
- [数字电路设计](digital-circuits.md) - 组合逻辑和时序逻辑设计
- [处理器设计](processor-design.md) - CPU 和 DSP 设计实例
- [内存系统设计](memory-systems.md) - 各种内存结构的实现
- [总线和互连](bus-interconnect.md) - AXI、AHB 等总线协议
- [验证方法学](verification.md) - 测试平台和形式化验证
- [FPGA 实现](fpga-implementation.md) - FPGA 开发流程和优化
- [ASIC 设计流程](asic-design-flow.md) - 从 RTL 到 GDSII 的完整流程

Valkyrie 的芯片设计框架提供了从高级抽象到底层实现的完整工具链，支持现代数字集成电路设计的各个环节，为硬件工程师提供了高效、安全、可维护的设计环境。