# GPU 计算与并行编程

Valkyrie 提供强大的 GPU 计算能力，支持通用 GPU 编程 (GPGPU)，可以利用现代显卡的并行计算能力进行高性能计算任务。

## 计算着色器基础

### 工作组和线程模型

```valkyrie
# 计算着色器的基本结构
@compute(workgroup_size = [64, 1, 1])  # 64个线程为一个工作组
micro basic_compute(id: ComputeInput) {
    let thread_id = id.global_invocation_id.x
    let local_id = id.local_invocation_id.x
    let workgroup_id = id.workgroup_id.x
    
    # 计算逻辑
    process_data(thread_id)
}

# 2D 工作组示例
@compute(workgroup_size = [8, 8, 1])  # 8x8 = 64个线程
micro image_process_compute(id: ComputeInput) {
    let coords = id.global_invocation_id.xy
    let local_coords = id.local_invocation_id.xy
    
    # 处理图像像素
    process_pixel(coords)
}

# 3D 工作组示例
@compute(workgroup_size = [4, 4, 4])  # 4x4x4 = 64个线程
micro volume_compute(id: ComputeInput) {
    let coords = id.global_invocation_id.xyz
    
    # 处理体素数据
    process_voxel(coords)
}
```

### 存储缓冲区

```valkyrie
# 只读存储缓冲区
@group(0) @binding(0)
let input_data: storage<array<f32>, read>

# 可读写存储缓冲区
@group(0) @binding(1)
let output_data: storage<array<f32>, read_write>

# 结构化数据
struct ParticleData {
    position: Vec3,
    velocity: Vec3,
    mass: f32,
    life: f32
}

@group(0) @binding(2)
let particles: storage<array<ParticleData>, read_write>

# Uniform 缓冲区（常量数据）
@group(1) @binding(0)
struct ComputeParams {
    delta_time: f32,
    particle_count: u32,
    gravity: Vec3,
    damping: f32
}
```

## 并行算法实现

### 并行归约 (Parallel Reduction)

```valkyrie
# 共享内存用于工作组内通信
@compute(workgroup_size = [256, 1, 1])
micro parallel_sum(id: ComputeInput) {
    # 共享内存声明
    var<workgroup> shared_data: array<f32, 256>
    
    let thread_id = id.local_invocation_id.x
    let global_id = id.global_invocation_id.x
    
    # 加载数据到共享内存
    if global_id < input_data.len() {
        shared_data[thread_id] = input_data[global_id]
    } else {
        shared_data[thread_id] = 0.0
    }
    
    # 同步工作组内所有线程
    workgroupBarrier()
    
    # 并行归约
    let mut stride = 128u32
    while stride > 0 {
        if thread_id < stride {
            shared_data[thread_id] += shared_data[thread_id + stride]
        }
        workgroupBarrier()
        stride /= 2
    }
    
    # 工作组的第一个线程写入结果
    if thread_id == 0 {
        let workgroup_id = id.workgroup_id.x
        output_data[workgroup_id] = shared_data[0]
    }
}

# 完整的归约实现
class ParallelReduction {
    device: wgpu::Device,
    queue: wgpu::Queue,
    compute_pipeline: wgpu::ComputePipeline,
    input_buffer: wgpu::Buffer,
    output_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer
    
    micro new(device: wgpu::Device, queue: wgpu::Queue) -> ParallelReduction {
        let shader = compile_valkyrie_shader(include_str!("parallel_sum.val"))
        let compute_pipeline = create_compute_pipeline(device, shader)
        
        ParallelReduction {
            device,
            queue,
            compute_pipeline,
            input_buffer: create_storage_buffer(device, 1024 * 1024 * 4), # 1M floats
            output_buffer: create_storage_buffer(device, 4096 * 4), # 4K floats
            staging_buffer: create_staging_buffer(device, 4096 * 4)
        }
    }
    
    micro compute_sum(mut self, data: [f32]) -> f32 {
        # 上传数据
        self.queue.write_buffer(self.input_buffer, 0, bytemuck::cast_slice(data))
        
        let mut encoder = self.device.create_command_encoder()
        
        # 第一阶段：并行归约
        {
            let mut compute_pass = encoder.begin_compute_pass()
            compute_pass.set_pipeline(self.compute_pipeline)
            compute_pass.set_bind_group(0, bind_group, [])
            
            let workgroups = (data.len() + 255) / 256
            compute_pass.dispatch_workgroups(workgroups as u32, 1, 1)
        }
        
        # 复制结果到 staging buffer
        encoder.copy_buffer_to_buffer(
            self.output_buffer, 0,
            self.staging_buffer, 0,
            workgroups * 4
        )
        
        self.queue.submit([encoder.finish()])
        
        # 读取结果
        let slice = self.staging_buffer.slice(..(workgroups * 4))
        let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel()
        slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap()
        })
        
        self.device.poll(wgpu::Maintain::Wait)
        receiver.receive().await.unwrap().unwrap()
        
        let data = slice.get_mapped_range()
        let result: [f32] = bytemuck::cast_slice(data)
        
        # CPU 端完成最终归约
        result.iter().sum()
    }
}
```

### 并行前缀和 (Parallel Prefix Sum)

```valkyrie
# Blelloch 扫描算法实现
@compute(workgroup_size = [256, 1, 1])
micro prefix_sum_up_sweep(id: ComputeInput, params: ScanParams) {
    var<workgroup> shared_data: array<u32, 512>  # 双倍大小用于 padding
    
    let thread_id = id.local_invocation_id.x
    let global_id = id.global_invocation_id.x
    
    # 加载数据
    let ai = thread_id
    let bi = thread_id + 256
    
    shared_data[ai] = if global_id < input_data.len() { input_data[global_id] } else { 0 }
    shared_data[bi] = if global_id + 256 < input_data.len() { input_data[global_id + 256] } else { 0 }
    
    # Up-sweep 阶段
    let mut offset = 1u32
    let mut d = 256u32
    while d > 0 {
        workgroupBarrier()
        
        if thread_id < d {
            let ai = offset * (2 * thread_id + 1) - 1
            let bi = offset * (2 * thread_id + 2) - 1
            shared_data[bi] += shared_data[ai]
        }
        
        offset *= 2
        d /= 2
    }
    
    # 清零最后一个元素
    if thread_id == 0 {
        shared_data[511] = 0
    }
    
    # Down-sweep 阶段
    d = 1
    while d < 512 {
        offset /= 2
        workgroupBarrier()
        
        if thread_id < d {
            let ai = offset * (2 * thread_id + 1) - 1
            let bi = offset * (2 * thread_id + 2) - 1
            
            let temp = shared_data[ai]
            shared_data[ai] = shared_data[bi]
            shared_data[bi] += temp
        }
        
        d *= 2
    }
    
    workgroupBarrier()
    
    # 写回结果
    if global_id < output_data.len() {
        output_data[global_id] = shared_data[ai]
    }
    if global_id + 256 < output_data.len() {
        output_data[global_id + 256] = shared_data[bi]
    }
}
```

### 并行排序

```valkyrie
# 双调排序 (Bitonic Sort)
@compute(workgroup_size = [256, 1, 1])
micro bitonic_sort(id: ComputeInput, params: SortParams) {
    var<workgroup> shared_data: array<u32, 256>
    
    let thread_id = id.local_invocation_id.x
    let global_id = id.global_invocation_id.x
    
    # 加载数据
    shared_data[thread_id] = if global_id < input_data.len() {
        input_data[global_id]
    } else {
        0xFFFFFFFF  # 最大值作为填充
    }
    
    workgroupBarrier()
    
    # 双调排序主循环
    let mut k = 2u32
    while k <= 256 {
        let mut j = k / 2
        while j > 0 {
            let ixj = thread_id ^ j
            
            if ixj > thread_id {
                let ascending = (thread_id & k) == 0
                
                if (shared_data[thread_id] > shared_data[ixj]) == ascending {
                    # 交换元素
                    let temp = shared_data[thread_id]
                    shared_data[thread_id] = shared_data[ixj]
                    shared_data[ixj] = temp
                }
            }
            
            workgroupBarrier()
            j /= 2
        }
        k *= 2
    }
    
    # 写回结果
    if global_id < output_data.len() {
        output_data[global_id] = shared_data[thread_id]
    }
}

# 基数排序实现
@compute(workgroup_size = [256, 1, 1])
micro radix_sort_count(id: ComputeInput, params: RadixSortParams) {
    var<workgroup> local_histogram: array<u32, 16>  # 4-bit 基数
    
    let thread_id = id.local_invocation_id.x
    let global_id = id.global_invocation_id.x
    
    # 初始化本地直方图
    if thread_id < 16 {
        local_histogram[thread_id] = 0
    }
    
    workgroupBarrier()
    
    # 计算本地直方图
    if global_id < input_data.len() {
        let value = input_data[global_id]
        let digit = (value >> (params.bit_shift * 4)) & 0xF
        atomicAdd(local_histogram[digit], 1u)
    }
    
    workgroupBarrier()
    
    # 将本地直方图写入全局直方图
    if thread_id < 16 {
        let workgroup_id = id.workgroup_id.x
        global_histogram[workgroup_id * 16 + thread_id] = local_histogram[thread_id]
    }
}
```

## 物理模拟

### N-Body 粒子模拟

```valkyrie
# N-Body 重力模拟
struct Body {
    position: Vec3,
    velocity: Vec3,
    mass: f32,
    _padding: f32  # 对齐到 16 字节
}

@group(0) @binding(0)
let bodies: storage<array<Body>, read_write>

@group(0) @binding(1)
struct NBodyParams {
    body_count: u32,
    delta_time: f32,
    softening: f32,  # 软化参数避免奇点
    damping: f32
}

@compute(workgroup_size = [64, 1, 1])
micro n_body_simulation(id: ComputeInput, params: NBodyParams) {
    let index = id.global_invocation_id.x
    if index >= params.body_count {
        return
    }
    
    let body = bodies[index]
    let mut force = Vec3(0.0, 0.0, 0.0)
    
    # 计算所有其他物体的引力
    for i in 0..params.body_count {
        if i != index {
            let other = bodies[i]
            let r = other.position - body.position
            let dist_sq = dot(r, r) + params.softening * params.softening
            let dist = sqrt(dist_sq)
            let force_magnitude = other.mass / (dist_sq * dist)
            force += r * force_magnitude
        }
    }
    
    # 更新速度和位置
    let acceleration = force / body.mass
    let new_velocity = body.velocity + acceleration * params.delta_time
    let new_position = body.position + new_velocity * params.delta_time
    
    # 应用阻尼
    let damped_velocity = new_velocity * params.damping
    
    # 写回结果
    bodies[index].velocity = damped_velocity
    bodies[index].position = new_position
}

# 优化版本：使用共享内存
@compute(workgroup_size = [64, 1, 1])
micro n_body_optimized(id: ComputeInput, params: NBodyParams) {
    var<workgroup> shared_bodies: array<Body, 64>
    
    let thread_id = id.local_invocation_id.x
    let global_id = id.global_invocation_id.x
    
    if global_id >= params.body_count {
        return
    }
    
    let body = bodies[global_id]
    let mut force = Vec3(0.0, 0.0, 0.0)
    
    # 分块处理
    let num_tiles = (params.body_count + 63) / 64
    
    for tile in 0..num_tiles {
        let tile_start = tile * 64
        let shared_index = tile_start + thread_id
        
        # 加载一个 tile 的数据到共享内存
        if shared_index < params.body_count {
            shared_bodies[thread_id] = bodies[shared_index]
        }
        
        workgroupBarrier()
        
        # 计算与当前 tile 中所有物体的相互作用
        let tile_size = min(64u32, params.body_count - tile_start)
        for i in 0..tile_size {
            if tile_start + i != global_id {
                let other = shared_bodies[i]
                let r = other.position - body.position
                let dist_sq = dot(r, r) + params.softening * params.softening
                let dist = sqrt(dist_sq)
                let force_magnitude = other.mass / (dist_sq * dist)
                force += r * force_magnitude
            }
        }
        
        workgroupBarrier()
    }
    
    # 更新物体状态
    let acceleration = force / body.mass
    bodies[global_id].velocity += acceleration * params.delta_time
    bodies[global_id].position += bodies[global_id].velocity * params.delta_time
    bodies[global_id].velocity *= params.damping
}
```

### 流体模拟 (SPH)

```valkyrie
# 光滑粒子流体动力学 (Smoothed Particle Hydrodynamics)
struct FluidParticle {
    position: Vec3,
    velocity: Vec3,
    density: f32,
    pressure: f32,
    force: Vec3,
    _padding: f32
}

@group(0) @binding(0)
let particles: storage<array<FluidParticle>, read_write>

@group(0) @binding(1)
struct SPHParams {
    particle_count: u32,
    rest_density: f32,
    gas_constant: f32,
    viscosity: f32,
    smoothing_radius: f32,
    particle_mass: f32,
    delta_time: f32,
    gravity: Vec3
}

# SPH 核函数
micro poly6_kernel(r: f32, h: f32) -> f32 {
    if r >= h {
        return 0.0
    }
    
    let h2 = h * h
    let h9 = h2 * h2 * h2 * h2 * h
    let r2 = r * r
    
    315.0 / (64.0 * PI * h9) * pow(h2 - r2, 3.0)
}

micro spiky_gradient_kernel(r_vec: Vec3, h: f32) -> Vec3 {
    let r = length(r_vec)
    if r >= h || r == 0.0 {
        return Vec3(0.0, 0.0, 0.0)
    }
    
    let h6 = h * h * h * h * h * h
    let factor = -45.0 / (PI * h6) * pow(h - r, 2.0) / r
    
    r_vec * factor
}

micro viscosity_laplacian_kernel(r: f32, h: f32) -> f32 {
    if r >= h {
        return 0.0
    }
    
    let h6 = h * h * h * h * h * h
    45.0 / (PI * h6) * (h - r)
}

# 密度计算
@compute(workgroup_size = [64, 1, 1])
micro compute_density(id: ComputeInput, params: SPHParams) {
    let index = id.global_invocation_id.x
    if index >= params.particle_count {
        return
    }
    
    let particle = particles[index]
    let mut density = 0.0
    
    # 计算密度
    for i in 0..params.particle_count {
        let other = particles[i]
        let r = length(particle.position - other.position)
        
        if r < params.smoothing_radius {
            density += params.particle_mass * poly6_kernel(r, params.smoothing_radius)
        }
    }
    
    particles[index].density = density
    particles[index].pressure = params.gas_constant * (density - params.rest_density)
}

# 力计算
@compute(workgroup_size = [64, 1, 1])
micro compute_forces(id: ComputeInput, params: SPHParams) {
    let index = id.global_invocation_id.x
    if index >= params.particle_count {
        return
    }
    
    let particle = particles[index]
    let mut pressure_force = Vec3(0.0, 0.0, 0.0)
    let mut viscosity_force = Vec3(0.0, 0.0, 0.0)
    
    for i in 0..params.particle_count {
        if i == index {
            continue
        }
        
        let other = particles[i]
        let r_vec = particle.position - other.position
        let r = length(r_vec)
        
        if r < params.smoothing_radius && r > 0.0 {
            # 压力力
            let pressure_gradient = spiky_gradient_kernel(r_vec, params.smoothing_radius)
            pressure_force -= params.particle_mass * 
                (particle.pressure + other.pressure) / (2.0 * other.density) * 
                pressure_gradient
            
            # 粘性力
            let velocity_diff = other.velocity - particle.velocity
            let viscosity_laplacian = viscosity_laplacian_kernel(r, params.smoothing_radius)
            viscosity_force += params.viscosity * params.particle_mass * 
                velocity_diff / other.density * viscosity_laplacian
        }
    }
    
    # 总力 = 压力力 + 粘性力 + 重力
    let total_force = pressure_force + viscosity_force + params.gravity * particle.density
    particles[index].force = total_force
}

# 积分更新
@compute(workgroup_size = [64, 1, 1])
micro integrate_particles(id: ComputeInput, params: SPHParams) {
    let index = id.global_invocation_id.x
    if index >= params.particle_count {
        return
    }
    
    let mut particle = particles[index]
    
    # Leapfrog 积分
    let acceleration = particle.force / particle.density
    particle.velocity += acceleration * params.delta_time
    particle.position += particle.velocity * params.delta_time
    
    # 边界条件（简单反弹）
    let boundary = 10.0
    if particle.position.x < -boundary || particle.position.x > boundary {
        particle.velocity.x *= -0.8
        particle.position.x = clamp(particle.position.x, -boundary, boundary)
    }
    if particle.position.y < -boundary || particle.position.y > boundary {
        particle.velocity.y *= -0.8
        particle.position.y = clamp(particle.position.y, -boundary, boundary)
    }
    if particle.position.z < -boundary || particle.position.z > boundary {
        particle.velocity.z *= -0.8
        particle.position.z = clamp(particle.position.z, -boundary, boundary)
    }
    
    particles[index] = particle
}
```

## 机器学习加速

### 矩阵乘法

```valkyrie
# 高效的矩阵乘法实现
@compute(workgroup_size = [16, 16, 1])
micro matrix_multiply(id: ComputeInput, params: MatMulParams) {
    var<workgroup> tile_a: array<array<f32, 16>, 16>
    var<workgroup> tile_b: array<array<f32, 16>, 16>
    
    let row = id.global_invocation_id.y
    let col = id.global_invocation_id.x
    let local_row = id.local_invocation_id.y
    let local_col = id.local_invocation_id.x
    
    if row >= params.m || col >= params.n {
        return
    }
    
    let mut sum = 0.0
    let num_tiles = (params.k + 15) / 16
    
    for tile in 0..num_tiles {
        let tile_start = tile * 16
        
        # 加载 A 的 tile
        let a_col = tile_start + local_col
        if row < params.m && a_col < params.k {
            tile_a[local_row][local_col] = matrix_a[row * params.k + a_col]
        } else {
            tile_a[local_row][local_col] = 0.0
        }
        
        # 加载 B 的 tile
        let b_row = tile_start + local_row
        if b_row < params.k && col < params.n {
            tile_b[local_row][local_col] = matrix_b[b_row * params.n + col]
        } else {
            tile_b[local_row][local_col] = 0.0
        }
        
        workgroupBarrier()
        
        # 计算部分乘积
        for k in 0..16 {
            sum += tile_a[local_row][k] * tile_b[k][local_col]
        }
        
        workgroupBarrier()
    }
    
    # 写入结果
    matrix_c[row * params.n + col] = sum
}

# 批量矩阵乘法
@compute(workgroup_size = [8, 8, 4])
micro batch_matrix_multiply(id: ComputeInput, params: BatchMatMulParams) {
    let batch = id.global_invocation_id.z
    let row = id.global_invocation_id.y
    let col = id.global_invocation_id.x
    
    if batch >= params.batch_size || row >= params.m || col >= params.n {
        return
    }
    
    let mut sum = 0.0
    let a_offset = batch * params.m * params.k
    let b_offset = batch * params.k * params.n
    let c_offset = batch * params.m * params.n
    
    for k in 0..params.k {
        let a_val = matrix_a[a_offset + row * params.k + k]
        let b_val = matrix_b[b_offset + k * params.n + col]
        sum += a_val * b_val
    }
    
    matrix_c[c_offset + row * params.n + col] = sum
}
```

### 卷积神经网络

```valkyrie
# 2D 卷积层
@compute(workgroup_size = [8, 8, 1])
micro conv2d(id: ComputeInput, params: Conv2DParams) {
    let out_y = id.global_invocation_id.y
    let out_x = id.global_invocation_id.x
    let out_c = id.global_invocation_id.z
    
    if out_y >= params.output_height || out_x >= params.output_width || out_c >= params.output_channels {
        return
    }
    
    let mut sum = 0.0
    
    # 卷积计算
    for in_c in 0..params.input_channels {
        for ky in 0..params.kernel_size {
            for kx in 0..params.kernel_size {
                let in_y = out_y * params.stride + ky - params.padding
                let in_x = out_x * params.stride + kx - params.padding
                
                if in_y >= 0 && in_y < params.input_height && 
                   in_x >= 0 && in_x < params.input_width {
                    
                    let input_idx = in_c * params.input_height * params.input_width + 
                                   in_y * params.input_width + in_x
                    let weight_idx = out_c * params.input_channels * params.kernel_size * params.kernel_size +
                                    in_c * params.kernel_size * params.kernel_size +
                                    ky * params.kernel_size + kx
                    
                    sum += input_data[input_idx] * weights[weight_idx]
                }
            }
        }
    }
    
    # 添加偏置并应用激活函数
    sum += biases[out_c]
    let activated = relu(sum)  # ReLU 激活
    
    let output_idx = out_c * params.output_height * params.output_width + 
                    out_y * params.output_width + out_x
    output_data[output_idx] = activated
}

# 激活函数
micro relu(x: f32) -> f32 {
    max(0.0, x)
}

micro sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + exp(-x))
}

micro tanh_activation(x: f32) -> f32 {
    tanh(x)
}

micro gelu(x: f32) -> f32 {
    0.5 * x * (1.0 + tanh(sqrt(2.0 / PI) * (x + 0.044715 * pow(x, 3.0))))
}

# 批量归一化
@compute(workgroup_size = [256, 1, 1])
micro batch_norm(id: ComputeInput, params: BatchNormParams) {
    let index = id.global_invocation_id.x
    if index >= params.size {
        return
    }
    
    let channel = index % params.channels
    let mean = running_mean[channel]
    let var = running_var[channel]
    let gamma = scale[channel]
    let beta = shift[channel]
    
    let normalized = (input_data[index] - mean) / sqrt(var + params.epsilon)
    output_data[index] = gamma * normalized + beta
}
```

## 图像处理

### 高级滤波器

```valkyrie
# 双边滤波器
@compute(workgroup_size = [8, 8, 1])
micro bilateral_filter(id: ComputeInput, params: BilateralParams) {
    let coords = id.global_invocation_id.xy
    let dimensions = textureDimensions(input_texture)
    
    if coords.x >= dimensions.x || coords.y >= dimensions.y {
        return
    }
    
    let center_color = textureLoad(input_texture, coords, 0)
    let mut weighted_sum = Vec3(0.0, 0.0, 0.0)
    let mut weight_sum = 0.0
    
    let radius = params.radius
    let sigma_space = params.sigma_space
    let sigma_color = params.sigma_color
    
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let sample_coords = coords + Vec2(dx, dy)
            
            if sample_coords.x >= 0 && sample_coords.x < dimensions.x &&
               sample_coords.y >= 0 && sample_coords.y < dimensions.y {
                
                let sample_color = textureLoad(input_texture, sample_coords, 0)
                
                # 空间权重
                let spatial_dist = sqrt((dx * dx + dy * dy) as f32)
                let spatial_weight = exp(-(spatial_dist * spatial_dist) / (2.0 * sigma_space * sigma_space))
                
                # 颜色权重
                let color_dist = length(sample_color.rgb - center_color.rgb)
                let color_weight = exp(-(color_dist * color_dist) / (2.0 * sigma_color * sigma_color))
                
                let weight = spatial_weight * color_weight
                weighted_sum += sample_color.rgb * weight
                weight_sum += weight
            }
        }
    }
    
    let filtered_color = if weight_sum > 0.0 {
        weighted_sum / weight_sum
    } else {
        center_color.rgb
    }
    
    textureStore(output_texture, coords, Vec4(filtered_color, center_color.a))
}

# 非局部均值去噪
@compute(workgroup_size = [8, 8, 1])
micro non_local_means(id: ComputeInput, params: NLMParams) {
    let coords = id.global_invocation_id.xy
    let dimensions = textureDimensions(input_texture)
    
    if coords.x >= dimensions.x || coords.y >= dimensions.y {
        return
    }
    
    let patch_size = params.patch_size
    let search_window = params.search_window
    let h = params.filtering_parameter
    
    let mut weighted_sum = Vec3(0.0, 0.0, 0.0)
    let mut weight_sum = 0.0
    
    # 搜索窗口
    for sy in -search_window..=search_window {
        for sx in -search_window..=search_window {
            let search_coords = coords + Vec2(sx, sy)
            
            if search_coords.x >= patch_size && search_coords.x < dimensions.x - patch_size &&
               search_coords.y >= patch_size && search_coords.y < dimensions.y - patch_size {
                
                # 计算补丁相似度
                let mut patch_distance = 0.0
                let mut patch_count = 0
                
                for py in -patch_size..=patch_size {
                    for px in -patch_size..=patch_size {
                        let p1 = textureLoad(input_texture, coords + Vec2(px, py), 0)
                        let p2 = textureLoad(input_texture, search_coords + Vec2(px, py), 0)
                        
                        let diff = p1.rgb - p2.rgb
                        patch_distance += dot(diff, diff)
                        patch_count += 1
                    }
                }
                
                patch_distance /= patch_count as f32
                
                # 计算权重
                let weight = exp(-max(patch_distance - 2.0 * params.noise_variance, 0.0) / (h * h))
                
                let sample_color = textureLoad(input_texture, search_coords, 0)
                weighted_sum += sample_color.rgb * weight
                weight_sum += weight
            }
        }
    }
    
    let denoised_color = if weight_sum > 0.0 {
        weighted_sum / weight_sum
    } else {
        textureLoad(input_texture, coords, 0).rgb
    }
    
    textureStore(output_texture, coords, Vec4(denoised_color, 1.0))
}
```

## 性能优化技巧

### 内存访问优化

```valkyrie
# 合并内存访问
@compute(workgroup_size = [32, 1, 1])  # warp size
micro coalesced_access(id: ComputeInput) {
    let thread_id = id.global_invocation_id.x
    
    # 好的访问模式：连续访问
    let value = input_data[thread_id]  # 线程 i 访问元素 i
    
    # 避免的访问模式：跨步访问
    # let value = input_data[thread_id * stride]  # 可能导致内存带宽浪费
    
    output_data[thread_id] = process(value)
}

# 使用共享内存减少全局内存访问
@compute(workgroup_size = [256, 1, 1])
micro shared_memory_optimization(id: ComputeInput) {
    var<workgroup> shared_cache: array<f32, 256>
    
    let thread_id = id.local_invocation_id.x
    let global_id = id.global_invocation_id.x
    
    # 协作加载数据到共享内存
    shared_cache[thread_id] = input_data[global_id]
    
    workgroupBarrier()
    
    # 现在可以快速访问共享内存中的数据
    let mut result = 0.0
    for i in 0..256 {
        result += shared_cache[i] * coefficients[i]
    }
    
    output_data[global_id] = result
}
```

### 分支优化

```valkyrie
# 避免分支分歧
@compute(workgroup_size = [32, 1, 1])
micro branch_optimization(id: ComputeInput) {
    let thread_id = id.global_invocation_id.x
    let value = input_data[thread_id]
    
    # 不好的分支：可能导致 warp 分歧
    # if thread_id % 2 == 0 {
    #     result = expensive_computation_a(value)
    # } else {
    #     result = expensive_computation_b(value)
    # }
    
    # 更好的方法：使用条件表达式
    let condition = thread_id % 2 == 0
    let result_a = expensive_computation_a(value)
    let result_b = expensive_computation_b(value)
    let result = if condition { result_a } else { result_b }
    
    # 或者重新组织算法避免分支
    output_data[thread_id] = result
}
```

### 占用率优化

```valkyrie
# 优化寄存器使用
@compute(workgroup_size = [128, 1, 1])  # 调整工作组大小以平衡占用率
micro register_optimization(id: ComputeInput) {
    let thread_id = id.global_invocation_id.x
    
    # 避免使用过多局部变量（寄存器）
    # 将大数组移到共享内存或重新计算
    
    let value = input_data[thread_id]
    let result = complex_computation(value)  # 内联小函数
    output_data[thread_id] = result
}

# 内存带宽优化
@compute(workgroup_size = [64, 1, 1])
micro bandwidth_optimization(id: ComputeInput) {
    let thread_id = id.global_invocation_id.x
    
    # 向量化访问
    let vec4_index = thread_id / 4
    let vec4_data = input_vec4[vec4_index]  # 一次读取 4 个 f32
    
    # 处理向量数据
    let processed = process_vec4(vec4_data)
    
    output_vec4[vec4_index] = processed
}
```

## 调试和性能分析

### GPU 调试工具

```valkyrie
# 调试缓冲区
@group(2) @binding(0)
let debug_buffer: storage<array<DebugInfo>, write>

struct DebugInfo {
    thread_id: u32,
    value: f32,
    iteration: u32,
    timestamp: u32
}

@compute(workgroup_size = [64, 1, 1])
micro debug_compute(id: ComputeInput) {
    let thread_id = id.global_invocation_id.x
    
    # 记录调试信息
    debug_buffer[thread_id] = DebugInfo {
        thread_id: thread_id,
        value: input_data[thread_id],
        iteration: 0,
        timestamp: 0  # GPU 时间戳
    }
    
    # 主要计算逻辑
    let result = compute_something(input_data[thread_id])
    
    # 更新调试信息
    debug_buffer[thread_id].value = result
    debug_buffer[thread_id].iteration = 1
    
    output_data[thread_id] = result
}

# 性能计数器
class GPUPerformanceCounter {
    query_sets: [wgpu::QuerySet],
    timestamp_period: f32
    
    micro new(device: wgpu::Device) -> GPUPerformanceCounter {
        let timestamp_query = device.create_query_set(wgpu::QuerySetDescriptor {
            label: Some("Timestamp Queries"),
            ty: wgpu::QueryType::Timestamp,
            count: 64
        })
        
        GPUPerformanceCounter {
            query_sets: [timestamp_query],
            timestamp_period: queue.get_timestamp_period()
        }
    }
    
    micro measure_compute_time(self, encoder: wgpu::CommandEncoder, compute_pass: impl FnOnce()) -> f64 {
        encoder.write_timestamp(self.query_sets[0], 0)
        
        compute_pass()
        
        encoder.write_timestamp(self.query_sets[0], 1)
        
        # 解析时间戳并返回毫秒
        let timestamps = self.resolve_timestamps()
        (timestamps[1] - timestamps[0]) as f64 * self.timestamp_period as f64 / 1_000_000.0
    }
}
```

## 总结

Valkyrie 的 GPU 计算能力提供了：

1. **现代并行编程模型** - 支持计算着色器和 GPGPU 编程
2. **高性能算法实现** - 并行归约、排序、前缀和等基础算法
3. **物理模拟加速** - N-Body、SPH 流体模拟等复杂物理计算
4. **机器学习支持** - 矩阵运算、卷积神经网络等 AI 计算
5. **图像处理优化** - 高级滤波器和图像算法的 GPU 实现
6. **性能优化工具** - 内存访问优化、分支优化、调试工具等

通过 Valkyrie 的 GPU 计算框架，开发者可以充分利用现代显卡的并行计算能力，实现高性能的科学计算、游戏物理、机器学习和图像处理应用。