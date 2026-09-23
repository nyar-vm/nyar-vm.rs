# 嵌入式开发

Valkyrie 提供了完整的嵌入式开发解决方案，支持 WebAssembly (WASM) 目标、微控制器编程、实时系统开发等。通过零成本抽象和内存安全保证，Valkyrie 为嵌入式系统提供了现代化的开发体验。

## 核心特性

### 内存管理

```valkyrie
# 栈分配的固定大小数组
type FixedBuffer<T, const N: usize> = [T; N]

# 无堆分配的向量实现
struct HeaplessVec<T, const N: usize> {
    data: [MaybeUninit<T>; N],
    len: usize
}

impl<T, const N: usize> HeaplessVec<T, N> {
    micro new() -> Self {
        HeaplessVec {
            data: unsafe { MaybeUninit::uninit().assume_init() },
            len: 0
        }
    }
    
    micro push(mut self, item: T) -> Result<(), T> {
        if self.len < N {
            self.data[self.len] = MaybeUninit::new(item)
            self.len += 1
            Ok(())
        } else {
            Err(item)
        }
    }
    
    micro pop(mut self) -> Option<T> {
        if self.len > 0 {
            self.len -= 1
            Some(unsafe { self.data[self.len].assume_init_read() })
        } else {
            None
        }
    }
    
    micro get(self, index: usize) -> Option<T> {
        if index < self.len {
            Some(unsafe { self.data[index].assume_init_ref() })
        } else {
            None
        }
    }
}
```

### 实时系统支持

```valkyrie
# 实时任务调度
struct RTOSTask {
    priority: u8,
    stack_size: usize,
    entry_point: fn(),
    state: TaskState
}

enum TaskState {
    Ready,
    Running,
    Blocked,
    Suspended
}

# 优先级调度器
class PriorityScheduler {
    tasks: HeaplessVec<RTOSTask, 32>,
    current_task: Option<usize>
    
    micro new() -> PriorityScheduler {
        PriorityScheduler {
            tasks: HeaplessVec::new(),
            current_task: None
        }
    }
    
    micro create_task(mut self, priority: u8, stack_size: usize, entry: fn()) -> Result<usize, ()> {
        let task = RTOSTask {
            priority,
            stack_size,
            entry_point: entry,
            state: TaskState::Ready
        }
        
        match self.tasks.push(task) {
            Ok(()) => Ok(self.tasks.len - 1),
            Err(_) => Err(())
        }
    }
    
    micro schedule(mut self) -> Option<usize> {
        let mut highest_priority = 0
        let mut selected_task = None
        
        for (i, task) in self.tasks.iter().enumerate() {
            if matches!(task.state, TaskState::Ready) && task.priority > highest_priority {
                highest_priority = task.priority
                selected_task = Some(i)
            }
        }
        
        if let Some(task_id) = selected_task {
            if let Some(current) = self.current_task {
                self.tasks[current].state = TaskState::Ready
            }
            
            self.tasks[task_id].state = TaskState::Running
            self.current_task = Some(task_id)
        }
        
        selected_task
    }
}
```

## WebAssembly 集成

### WASM 模块开发

```valkyrie
# WASM 导出函数
↯wasm_export
micro add(a: i32, b: i32) -> i32 {
    a + b
}

↯wasm_export
micro process_buffer(ptr: *mut u8, len: usize) -> i32 {
    let buffer = unsafe { slice::from_raw_parts_mut(ptr, len) }
    
    # 处理缓冲区数据
    for byte in buffer {
        *byte = (*byte).wrapping_add(1)
    }
    
    len as i32
}

# WASM 内存管理
struct WasmAllocator {
    heap_start: *mut u8,
    heap_size: usize,
    free_blocks: HeaplessVec<MemoryBlock, 64>
}

struct MemoryBlock {
    ptr: *mut u8,
    size: usize
}

impl WasmAllocator {
    micro new(heap_start: *mut u8, heap_size: usize) -> WasmAllocator {
        let mut allocator = WasmAllocator {
            heap_start,
            heap_size,
            free_blocks: HeaplessVec::new()
        }
        
        # 初始化一个大的空闲块
        allocator.free_blocks.push(MemoryBlock {
            ptr: heap_start,
            size: heap_size
        }).unwrap()
        
        allocator
    }
    
    micro allocate(mut self, size: usize, align: usize) -> Option<*mut u8> {
        for (i, block) in self.free_blocks.iter().enumerate() {
            let aligned_ptr = align_up(block.ptr as usize, align) as *mut u8
            let aligned_size = (block.ptr as usize + block.size) - (aligned_ptr as usize)
            
            if aligned_size >= size {
                # 分割块
                let remaining_size = aligned_size - size
                
                if remaining_size > 0 {
                    let remaining_block = MemoryBlock {
                        ptr: unsafe { aligned_ptr.add(size) },
                        size: remaining_size
                    }
                    
                    self.free_blocks[i] = remaining_block
                } else {
                    self.free_blocks.remove(i)
                }
                
                return Some(aligned_ptr)
            }
        }
        
        None
    }
    
    micro deallocate(mut self, ptr: *mut u8, size: usize) {
        let block = MemoryBlock { ptr, size }
        
        # 简单的释放实现，实际应该合并相邻块
        self.free_blocks.push(block).ok()
    }
}

micro align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
```

### WASI 接口

```valkyrie
# WASI 系統調用封裝
mod wasi {
    @import(wasm32, "wasi_snapshot_preview1", "fd_write")
    micro fd_write(fd: i32, iovs_ptr: *const IoVec, iovs_len: usize, nwritten: *mut usize) -> i32

    @import(wasm32, "wasi_snapshot_preview1", "fd_read")
    micro fd_read(fd: i32, iovs_ptr: *const IoVec, iovs_len: usize, nread: *mut usize) -> i32

    @import(wasm32, "wasi_snapshot_preview1", "clock_time_get")
    micro clock_time_get(id: i32, precision: i64, time: *mut i64) -> i32

    @import(wasm32, "wasi_snapshot_preview1", "random_get")
    micro random_get(buf: *mut u8, buf_len: usize) -> i32

    struct IoVec {
        buf: *const u8,
        buf_len: usize
    }
    
    micro print(msg: &str) {
        let iov = IoVec {
            buf: msg.as_ptr(),
            buf_len: msg.len()
        }
        
        let mut nwritten = 0
        unsafe {
            fd_write(1, &iov, 1, &mut nwritten)
        }
    }
    
    micro read_input(buffer: &mut [u8]) -> usize {
        let iov = IoVec {
            buf: buffer.as_mut_ptr(),
            buf_len: buffer.len()
        }
        
        let mut nread = 0
        unsafe {
            fd_read(0, &iov, 1, &mut nread)
        }
        
        nread
    }
    
    micro get_time() -> i64 {
        let mut time = 0
        unsafe {
            clock_time_get(0, 1, &mut time)
        }
        time
    }
    
    micro get_random_bytes(buffer: &mut [u8]) {
        unsafe {
            random_get(buffer.as_mut_ptr(), buffer.len())
        }
    }
}
```

## 微控制器编程

### GPIO 控制

```valkyrie
# GPIO 抽象层
trait GpioPin {
    micro set_high(mut self)
    micro set_low(mut self)
    micro is_high(self) -> bool
    micro is_low(self) -> bool
    micro toggle(mut self)
}

# ARM Cortex-M GPIO 实现
struct CortexMGpio {
    port: *mut u32,
    pin: u8
}

impl GpioPin for CortexMGpio {
    micro set_high(mut self) {
        unsafe {
            *self.port |= 1 << self.pin
        }
    }
    
    micro set_low(mut self) {
        unsafe {
            *self.port &= !(1 << self.pin)
        }
    }
    
    micro is_high(self) -> bool {
        unsafe {
            (*self.port & (1 << self.pin)) != 0
        }
    }
    
    micro is_low(self) -> bool {
        !self.is_high()
    }
    
    micro toggle(mut self) {
        if self.is_high() {
            self.set_low()
        } else {
            self.set_high()
        }
    }
}

# LED 控制示例
struct Led<P: GpioPin> {
    pin: P
}

impl<P: GpioPin> Led<P> {
    micro new(pin: P) -> Led<P> {
        Led { pin }
    }
    
    micro on(mut self) {
        self.pin.set_high()
    }
    
    micro off(mut self) {
        self.pin.set_low()
    }
    
    micro blink(mut self, delay_ms: u32) {
        self.pin.toggle()
        delay(delay_ms)
    }
}
```

### 中断处理

```valkyrie
# 中断向量表
↯link_section = ".vector_table"
static VECTOR_TABLE: [unsafe micro(); 256] = [
    reset_handler,
    nmi_handler,
    hard_fault_handler,
    // ... 其他中断处理程序
]

# 中断处理程序
↯interrupt
@export(c, "timer_interrupt")
micro timer_interrupt() {
    # 清除中断标志
    clear_timer_interrupt()
    
    # 处理定时器中断
    SYSTEM_TICK.fetch_add(1, Ordering::Relaxed)
    
    # 调度器检查
    if SYSTEM_TICK.load(Ordering::Relaxed) % 10 == 0 {
        schedule_next_task()
    }
}

↯interrupt
@export(c, "gpio_interrupt")
micro gpio_interrupt() {
    let pin_state = read_gpio_interrupt_status()
    
    # 处理按钮按下
    if pin_state & (1 << BUTTON_PIN) != 0 {
        BUTTON_PRESSED.store(true, Ordering::Relaxed)
        clear_gpio_interrupt(BUTTON_PIN)
    }
}

# 原子操作支持
use core::sync::atomic::{AtomicU32, AtomicBool, Ordering}

static SYSTEM_TICK: AtomicU32 = AtomicU32::new(0)
static BUTTON_PRESSED: AtomicBool = AtomicBool::new(false)
```

### 通信协议

```valkyrie
# SPI 通信
struct SpiMaster {
    base_addr: *mut u32,
    cs_pin: CortexMGpio
}

impl SpiMaster {
    micro new(base_addr: *mut u32, cs_pin: CortexMGpio) -> SpiMaster {
        SpiMaster { base_addr, cs_pin }
    }
    
    micro transfer(mut self, data: &[u8]) -> HeaplessVec<u8, 256> {
        let mut result = HeaplessVec::new()
        
        self.cs_pin.set_low()  # 选择从设备
        
        for &byte in data {
            self.write_data_register(byte)
            while !self.is_transfer_complete() {}
            
            let received = self.read_data_register()
            result.push(received).ok()
        }
        
        self.cs_pin.set_high()  # 取消选择
        result
    }
    
    micro write_data_register(self, data: u8) {
        unsafe {
            *(self.base_addr.offset(0)) = data as u32
        }
    }
    
    micro read_data_register(self) -> u8 {
        unsafe {
            *(self.base_addr.offset(0)) as u8
        }
    }
    
    micro is_transfer_complete(self) -> bool {
        unsafe {
            (*(self.base_addr.offset(1)) & 0x01) != 0
        }
    }
}

# I2C 通信
struct I2cMaster {
    base_addr: *mut u32
}

impl I2cMaster {
    micro write_register(self, device_addr: u8, reg_addr: u8, data: u8) -> Result<(), I2cError> {
        self.start_condition()
        self.send_address(device_addr, false)?  # 写模式
        self.send_data(reg_addr)?
        self.send_data(data)?
        self.stop_condition()
        Ok(())
    }
    
    micro read_register(self, device_addr: u8, reg_addr: u8) -> Result<u8, I2cError> {
        self.start_condition()
        self.send_address(device_addr, false)?  # 写模式
        self.send_data(reg_addr)?
        
        self.repeated_start()
        self.send_address(device_addr, true)?   # 读模式
        let data = self.receive_data(true)?     # 发送 NACK
        self.stop_condition()
        
        Ok(data)
    }
}

enum I2cError {
    Timeout,
    Nack,
    BusError
}
```

## 传感器接口

### 模拟传感器

```valkyrie
# ADC 接口
struct AdcChannel {
    channel: u8,
    resolution: u8  # 位数
}

impl AdcChannel {
    micro read_raw(self) -> u16 {
        # 启动 ADC 转换
        self.start_conversion()
        
        # 等待转换完成
        while !self.is_conversion_complete() {
            # 可以在这里让出 CPU
        }
        
        self.read_result()
    }
    
    micro read_voltage(self, vref: f32) -> f32 {
        let raw = self.read_raw() as f32
        let max_value = (1 << self.resolution) as f32
        (raw / max_value) * vref
    }
}

# 温度传感器
struct TemperatureSensor {
    adc: AdcChannel,
    calibration: TemperatureCalibration
}

struct TemperatureCalibration {
    offset: f32,
    scale: f32
}

impl TemperatureSensor {
    micro read_celsius(self) -> f32 {
        let voltage = self.adc.read_voltage(3.3)
        
        # 线性温度传感器转换
        (voltage - self.calibration.offset) * self.calibration.scale
    }
    
    micro read_fahrenheit(self) -> f32 {
        let celsius = self.read_celsius()
        celsius * 9.0 / 5.0 + 32.0
    }
}
```

### 数字传感器

```valkyrie
# IMU 传感器 (MPU6050)
struct Mpu6050 {
    i2c: I2cMaster,
    address: u8
}

struct AccelData {
    x: i16,
    y: i16,
    z: i16
}

struct GyroData {
    x: i16,
    y: i16,
    z: i16
}

impl Mpu6050 {
    const ACCEL_XOUT_H: u8 = 0x3B
    const GYRO_XOUT_H: u8 = 0x43
    const PWR_MGMT_1: u8 = 0x6B
    
    micro new(i2c: I2cMaster, address: u8) -> Mpu6050 {
        Mpu6050 { i2c, address }
    }
    
    micro init(self) -> Result<(), I2cError> {
        # 唤醒设备
        self.i2c.write_register(self.address, Self::PWR_MGMT_1, 0x00)
    }
    
    micro read_accel(self) -> Result<AccelData, I2cError> {
        let mut data = [0u8; 6]
        
        for i in 0..6 {
            data[i] = self.i2c.read_register(self.address, Self::ACCEL_XOUT_H + i)?
        }
        
        Ok(AccelData {
            x: ((data[0] as i16) << 8) | (data[1] as i16),
            y: ((data[2] as i16) << 8) | (data[3] as i16),
            z: ((data[4] as i16) << 8) | (data[5] as i16)
        })
    }
    
    micro read_gyro(self) -> Result<GyroData, I2cError> {
        let mut data = [0u8; 6]
        
        for i in 0..6 {
            data[i] = self.i2c.read_register(self.address, Self::GYRO_XOUT_H + i)?
        }
        
        Ok(GyroData {
            x: ((data[0] as i16) << 8) | (data[1] as i16),
            y: ((data[2] as i16) << 8) | (data[3] as i16),
            z: ((data[4] as i16) << 8) | (data[5] as i16)
        })
    }
}
```

## 电源管理

### 低功耗模式

```valkyrie
# 电源管理单元
enum PowerMode {
    Active,
    Sleep,
    DeepSleep,
    Standby
}

struct PowerManager {
    current_mode: PowerMode,
    wake_sources: u32
}

impl PowerManager {
    micro new() -> PowerManager {
        PowerManager {
            current_mode: PowerMode::Active,
            wake_sources: 0
        }
    }
    
    micro enter_sleep_mode(mut self, mode: PowerMode) {
        match mode {
            PowerMode::Sleep => {
                # 关闭不必要的外设
                self.disable_peripherals()
                # 进入睡眠模式
                self.cpu_sleep()
            },
            PowerMode::DeepSleep => {
                # 保存关键状态
                self.save_context()
                # 关闭更多外设
                self.disable_most_peripherals()
                # 进入深度睡眠
                self.cpu_deep_sleep()
            },
            PowerMode::Standby => {
                # 只保留最小功能
                self.enter_standby()
            },
            _ => {}
        }
        
        self.current_mode = mode
    }
    
    micro set_wake_source(mut self, source: WakeSource) {
        self.wake_sources |= source as u32
        self.configure_wake_source(source)
    }
    
    micro on_wake_up(mut self) {
        match self.current_mode {
            PowerMode::DeepSleep => {
                self.restore_context()
                self.enable_peripherals()
            },
            PowerMode::Sleep => {
                self.enable_peripherals()
            },
            _ => {}
        }
        
        self.current_mode = PowerMode::Active
    }
}

enum WakeSource {
    Timer = 0x01,
    Gpio = 0x02,
    Uart = 0x04,
    I2c = 0x08
}
```

## 文档链接

- [WASM 开发指南](wasm-development.md) - WebAssembly 模块开发和 WASI 接口
- [微控制器编程](microcontroller.md) - ARM Cortex-M 和其他 MCU 平台
- [实时系统](real-time-systems.md) - RTOS 开发和实时任务调度
- [传感器接口](sensor-interfaces.md) - 各种传感器的驱动开发
- [通信协议](communication-protocols.md) - SPI、I2C、UART 等协议实现
- [电源管理](power-management.md) - 低功耗设计和电源优化

Valkyrie 的嵌入式开发框架提供了从底层硬件抽象到高级应用开发的完整解决方案，支持现代嵌入式系统的各种需求。