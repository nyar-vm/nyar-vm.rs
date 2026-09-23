# Valkyrie 图形编程与 Shader 开发

Valkyrie 语言原生支持图形编程，可以直接编写 shader 代码，无需依赖 GLSL 或 HLSL。通过内置的图形 API 和 wgpu 集成，Valkyrie 提供了现代化的 GPU 编程体验。

## Valkyrie Shader 语言特性

### 内置图形类型

Valkyrie 提供了丰富的图形编程类型：

```valkyrie
# 向量类型
type Vec2 = [f32; 2]
type Vec3 = [f32; 3]
type Vec4 = [f32; 4]

# 矩阵类型
type Mat2 = [[f32; 2]; 2]
type Mat3 = [[f32; 3]; 3]
type Mat4 = [[f32; 4]; 4]

# 纹理类型
type Texture2D = texture<f32, 2>
type TextureCube = texture<f32, cube>
type Sampler = sampler

# 颜色类型
type Color = Vec4  # RGBA
type RGB = Vec3
```

### Shader 函数标记

```valkyrie
# Vertex Shader 标记
@vertex
micro vertex_main(vertex: VertexInput) -> VertexOutput {
    # 顶点着色器逻辑
}

# Fragment Shader 标记
@fragment
micro fragment_main(input: FragmentInput) -> FragmentOutput {
    # 片段着色器逻辑
}

# Compute Shader 标记
@compute(workgroup_size = [8, 8, 1])
micro compute_main(id: ComputeInput) {
    # 计算着色器逻辑
}
```

## 基础 Shader 示例

### 简单顶点着色器

```valkyrie
# 顶点输入结构
struct VertexInput {
    @location(0) position: Vec3,
    @location(1) color: Vec3,
    @location(2) uv: Vec2
}

# 顶点输出结构
struct VertexOutput {
    @builtin(position) clip_position: Vec4,
    @location(0) color: Vec3,
    @location(1) uv: Vec2
}

# Uniform 缓冲区
@group(0) @binding(0)
struct CameraUniform {
    view_proj: Mat4
}

@vertex
micro vertex_main(vertex: VertexInput, camera: CameraUniform) -> VertexOutput {
    VertexOutput {
        clip_position: camera.view_proj * Vec4(vertex.position, 1.0),
        color: vertex.color,
        uv: vertex.uv
    }
}
```

### 纹理片段着色器

```valkyrie
# 片段输入（来自顶点着色器）
struct FragmentInput {
    @location(0) color: Vec3,
    @location(1) uv: Vec2
}

# 片段输出
struct FragmentOutput {
    @location(0) color: Vec4
}

# 纹理和采样器
@group(1) @binding(0)
let texture: Texture2D
@group(1) @binding(1)
let sampler: Sampler

@fragment
micro fragment_main(input: FragmentInput) -> FragmentOutput {
    let tex_color = texture.sample(sampler, input.uv)
    let final_color = tex_color * Vec4(input.color, 1.0)
    
    FragmentOutput {
        color: final_color
    }
}
```

## 高级 Shader 技术

### 光照计算

```valkyrie
# 光照数据结构
struct Light {
    position: Vec3,
    color: Vec3,
    intensity: f32
}

# 材质属性
struct Material {
    albedo: Vec3,
    metallic: f32,
    roughness: f32,
    normal: Vec3
}

# Phong 光照模型
micro phong_lighting(material: Material, light: Light, view_dir: Vec3, light_dir: Vec3) -> Vec3 {
    let normal = normalize(material.normal)
    let light_dir = normalize(light_dir)
    let view_dir = normalize(view_dir)
    
    # 环境光
    let ambient = 0.1 * material.albedo
    
    # 漫反射
    let diff = max(dot(normal, light_dir), 0.0)
    let diffuse = diff * light.color * material.albedo
    
    # 镜面反射
    let reflect_dir = reflect(-light_dir, normal)
    let spec = pow(max(dot(view_dir, reflect_dir), 0.0), 32.0)
    let specular = spec * light.color
    
    (ambient + diffuse + specular) * light.intensity
}

# PBR 光照模型
micro pbr_lighting(material: Material, light: Light, view_dir: Vec3, light_dir: Vec3) -> Vec3 {
    let normal = normalize(material.normal)
    let light_dir = normalize(light_dir)
    let view_dir = normalize(view_dir)
    let half_dir = normalize(light_dir + view_dir)
    
    # 菲涅尔反射
    let f0 = mix(Vec3(0.04), material.albedo, material.metallic)
    let fresnel = fresnel_schlick(max(dot(half_dir, view_dir), 0.0), f0)
    
    # 法线分布函数
    let ndf = distribution_ggx(normal, half_dir, material.roughness)
    
    # 几何函数
    let geometry = geometry_smith(normal, view_dir, light_dir, material.roughness)
    
    # Cook-Torrance BRDF
    let numerator = ndf * geometry * fresnel
    let denominator = 4.0 * max(dot(normal, view_dir), 0.0) * max(dot(normal, light_dir), 0.0) + 0.0001
    let specular = numerator / denominator
    
    # 能量守恒
    let ks = fresnel
    let kd = (Vec3(1.0) - ks) * (1.0 - material.metallic)
    
    let ndotl = max(dot(normal, light_dir), 0.0)
    (kd * material.albedo / PI + specular) * light.color * light.intensity * ndotl
}

# 辅助函数
micro fresnel_schlick(cos_theta: f32, f0: Vec3) -> Vec3 {
    f0 + (Vec3(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0)
}

micro distribution_ggx(normal: Vec3, half_dir: Vec3, roughness: f32) -> f32 {
    let a = roughness * roughness
    let a2 = a * a
    let ndoth = max(dot(normal, half_dir), 0.0)
    let ndoth2 = ndoth * ndoth
    
    let num = a2
    let denom = ndoth2 * (a2 - 1.0) + 1.0
    
    num / (PI * denom * denom)
}

micro geometry_smith(normal: Vec3, view_dir: Vec3, light_dir: Vec3, roughness: f32) -> f32 {
    let ndotv = max(dot(normal, view_dir), 0.0)
    let ndotl = max(dot(normal, light_dir), 0.0)
    let ggx2 = geometry_schlick_ggx(ndotv, roughness)
    let ggx1 = geometry_schlick_ggx(ndotl, roughness)
    
    ggx1 * ggx2
}

micro geometry_schlick_ggx(ndotv: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0
    let k = (r * r) / 8.0
    
    let num = ndotv
    let denom = ndotv * (1.0 - k) + k
    
    num / denom
}
```

### 后处理效果

```valkyrie
# 屏幕空间效果的片段着色器
@fragment
micro post_process_main(input: FragmentInput) -> FragmentOutput {
    let uv = input.uv
    let color = texture.sample(sampler, uv)
    
    # 应用多种后处理效果
    let processed = color
        |> apply_bloom
        |> apply_tone_mapping
        |> apply_gamma_correction
        |> apply_vignette(uv)
    
    FragmentOutput { color: processed }
}

# Bloom 效果
micro apply_bloom(color: Vec4) -> Vec4 {
    let brightness = dot(color.rgb, Vec3(0.2126, 0.7152, 0.0722))
    if brightness > 1.0 {
        color * (brightness - 1.0) * 0.5
    } else {
        color
    }
}

# 色调映射
micro apply_tone_mapping(color: Vec4) -> Vec4 {
    # Reinhard 色调映射
    let mapped = color.rgb / (color.rgb + Vec3(1.0))
    Vec4(mapped, color.a)
}

# 伽马校正
micro apply_gamma_correction(color: Vec4) -> Vec4 {
    let gamma = 2.2
    Vec4(pow(color.rgb, Vec3(1.0 / gamma)), color.a)
}

# 暗角效果
micro apply_vignette(color: Vec4, uv: Vec2) -> Vec4 {
    let center = Vec2(0.5, 0.5)
    let distance = length(uv - center)
    let vignette = smoothstep(0.8, 0.2, distance)
    
    Vec4(color.rgb * vignette, color.a)
}
```

## 计算着色器

### 粒子系统

```valkyrie
# 粒子数据结构
struct Particle {
    position: Vec3,
    velocity: Vec3,
    life: f32,
    size: f32
}

# 计算着色器输入
@group(0) @binding(0)
let particles: storage<array<Particle>, read_write>

@group(0) @binding(1)
struct SimulationParams {
    delta_time: f32,
    gravity: Vec3,
    damping: f32
}

@compute(workgroup_size = [64, 1, 1])
micro update_particles(id: ComputeInput, params: SimulationParams) {
    let index = id.global_invocation_id.x
    if index >= particles.len() {
        return
    }
    
    let mut particle = particles[index]
    
    # 更新粒子物理
    particle.velocity += params.gravity * params.delta_time
    particle.velocity *= params.damping
    particle.position += particle.velocity * params.delta_time
    
    # 更新生命周期
    particle.life -= params.delta_time
    
    # 重置死亡粒子
    if particle.life <= 0.0 {
        particle = spawn_new_particle()
    }
    
    particles[index] = particle
}

micro spawn_new_particle() -> Particle {
    Particle {
        position: Vec3(0.0, 0.0, 0.0),
        velocity: random_vec3() * 5.0,
        life: 3.0,
        size: 1.0
    }
}
```

### GPU 加速的图像处理

```valkyrie
# 图像卷积计算着色器
@group(0) @binding(0)
let input_texture: texture_2d<f32>
@group(0) @binding(1)
let output_texture: texture_storage_2d<rgba8unorm, write>

# 卷积核
const SOBEL_X: [[f32; 3]; 3] = [
    [-1.0, 0.0, 1.0],
    [-2.0, 0.0, 2.0],
    [-1.0, 0.0, 1.0]
]

const SOBEL_Y: [[f32; 3]; 3] = [
    [-1.0, -2.0, -1.0],
    [ 0.0,  0.0,  0.0],
    [ 1.0,  2.0,  1.0]
]

@compute(workgroup_size = [8, 8, 1])
micro edge_detection(id: ComputeInput) {
    let coords = id.global_invocation_id.xy
    let dimensions = textureDimensions(input_texture)
    
    if coords.x >= dimensions.x || coords.y >= dimensions.y {
        return
    }
    
    let mut gx = 0.0
    let mut gy = 0.0
    
    # 应用 Sobel 算子
    for i in 0..3 {
        for j in 0..3 {
            let sample_coords = coords + Vec2(i - 1, j - 1)
            let color = textureLoad(input_texture, sample_coords, 0)
            let gray = dot(color.rgb, Vec3(0.299, 0.587, 0.114))
            
            gx += gray * SOBEL_X[i][j]
            gy += gray * SOBEL_Y[i][j]
        }
    }
    
    let magnitude = sqrt(gx * gx + gy * gy)
    let edge_color = Vec4(magnitude, magnitude, magnitude, 1.0)
    
    textureStore(output_texture, coords, edge_color)
}
```

## wgpu 集成

### 渲染管线设置

```valkyrie
# wgpu 设备和队列
struct GraphicsContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    config: wgpu::SurfaceConfiguration
}

# 渲染管线创建
micro create_render_pipeline(context: GraphicsContext) -> wgpu::RenderPipeline {
    # 编译 Valkyrie shader 到 WGSL
    let vertex_shader = compile_valkyrie_shader("
        @vertex
        micro vertex_main(vertex: VertexInput, camera: CameraUniform) -> VertexOutput {
            VertexOutput {
                clip_position: camera.view_proj * Vec4(vertex.position, 1.0),
                color: vertex.color,
                uv: vertex.uv
            }
        }
    ")
    
    let fragment_shader = compile_valkyrie_shader("
        @fragment
        micro fragment_main(input: FragmentInput) -> FragmentOutput {
            let tex_color = texture.sample(sampler, input.uv)
            FragmentOutput { color: tex_color }
        }
    ")
    
    # 创建 shader 模块
    let vs_module = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Vertex Shader"),
        source: wgpu::ShaderSource::Wgsl(vertex_shader.into())
    })
    
    let fs_module = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Fragment Shader"),
        source: wgpu::ShaderSource::Wgsl(fragment_shader.into())
    })
    
    # 创建渲染管线
    context.device.create_render_pipeline(wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: vs_module,
            entry_point: "vertex_main",
            buffers: vertex_buffer_layout
        },
        fragment: Some(wgpu::FragmentState {
            module: fs_module,
            entry_point: "fragment_main",
            targets: color_target_states
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None
    })
}
```

### 资源管理

```valkyrie
# 纹理加载和管理
class TextureManager {
    textures: HashMap<String, wgpu::Texture>,
    device: wgpu::Device,
    queue: wgpu::Queue
    
    micro new(device: wgpu::Device, queue: wgpu::Queue) -> TextureManager {
        TextureManager {
            textures: HashMap::new(),
            device,
            queue
        }
    }
    
    micro load_texture(mut self, name: String, path: String) -> Result<(), Error> {
        let image_data = load_image_from_file(path)?
        
        let texture = self.device.create_texture(wgpu::TextureDescriptor {
            label: Some(name.as_str()),
            size: wgpu::Extent3d {
                width: image_data.width,
                height: image_data.height,
                depth_or_array_layers: 1
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: []
        })
        
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All
            },
            image_data.data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * image_data.width),
                rows_per_image: Some(image_data.height)
            },
            texture.size()
        )
        
        self.textures.insert(name, texture)
        Ok(())
    }
    
    micro get_texture(self, name: String) -> Option<wgpu::Texture> {
        self.textures.get(name).cloned()
    }
}
```

## 高级图形技术

### 延迟渲染

```valkyrie
# G-Buffer 结构
struct GBuffer {
    albedo: wgpu::Texture,      # RGB: 反照率, A: 金属度
    normal: wgpu::Texture,      # RGB: 世界空间法线
    material: wgpu::Texture,    # R: 粗糙度, G: AO, B: 自发光
    depth: wgpu::Texture        # 深度缓冲
}

# 几何阶段着色器
@fragment
micro geometry_pass_fragment(input: FragmentInput) -> GBufferOutput {
    let albedo = texture_albedo.sample(sampler, input.uv)
    let normal_map = texture_normal.sample(sampler, input.uv)
    let material_props = texture_material.sample(sampler, input.uv)
    
    # 计算世界空间法线
    let world_normal = calculate_world_normal(input.normal, input.tangent, normal_map)
    
    GBufferOutput {
        albedo: Vec4(albedo.rgb, material_props.r),  # 金属度存储在 alpha 通道
        normal: Vec4(world_normal * 0.5 + 0.5, 1.0), # 法线编码到 [0,1] 范围
        material: Vec4(material_props.g, material_props.b, material_props.a, 1.0)
    }
}

# 光照阶段着色器
@fragment
micro lighting_pass_fragment(input: ScreenQuadInput) -> Vec4 {
    let uv = input.uv
    
    # 从 G-Buffer 读取数据
    let albedo_metallic = g_buffer_albedo.sample(sampler, uv)
    let encoded_normal = g_buffer_normal.sample(sampler, uv)
    let material_data = g_buffer_material.sample(sampler, uv)
    let depth = g_buffer_depth.sample(sampler, uv).r
    
    # 重建世界位置
    let world_pos = reconstruct_world_position(uv, depth, inv_view_proj)
    
    # 解码法线
    let world_normal = normalize(encoded_normal.xyz * 2.0 - 1.0)
    
    # 材质属性
    let albedo = albedo_metallic.rgb
    let metallic = albedo_metallic.a
    let roughness = material_data.r
    let ao = material_data.g
    
    # 计算光照
    let view_dir = normalize(camera_pos - world_pos)
    let mut final_color = Vec3(0.0)
    
    # 处理所有光源
    for light in lights {
        let light_dir = normalize(light.position - world_pos)
        let light_contribution = pbr_lighting(
            Material { albedo, metallic, roughness, normal: world_normal },
            light,
            view_dir,
            light_dir
        )
        final_color += light_contribution
    }
    
    # 应用环境遮蔽
    final_color *= ao
    
    Vec4(final_color, 1.0)
}
```

### 阴影映射

```valkyrie
# 阴影映射顶点着色器
@vertex
micro shadow_vertex_main(vertex: VertexInput, light_space: LightSpaceUniform) -> ShadowVertexOutput {
    ShadowVertexOutput {
        clip_position: light_space.light_space_matrix * Vec4(vertex.position, 1.0)
    }
}

# 阴影映射片段着色器（深度写入）
@fragment
micro shadow_fragment_main(input: ShadowVertexOutput) {
    # 只写入深度，不需要颜色输出
}

# 使用阴影的主渲染着色器
@fragment
micro main_fragment_with_shadow(input: FragmentInput) -> Vec4 {
    let world_pos = input.world_position
    let normal = normalize(input.normal)
    
    # 转换到光空间
    let light_space_pos = light_space_matrix * Vec4(world_pos, 1.0)
    let proj_coords = light_space_pos.xyz / light_space_pos.w
    let shadow_coords = proj_coords * 0.5 + 0.5
    
    # 采样阴影贴图
    let shadow_depth = shadow_map.sample(shadow_sampler, shadow_coords.xy).r
    let current_depth = shadow_coords.z
    
    # 阴影偏移以减少阴影痤疮
    let bias = max(0.05 * (1.0 - dot(normal, light_dir)), 0.005)
    let shadow = if current_depth - bias > shadow_depth { 0.0 } else { 1.0 }
    
    # PCF 软阴影
    let shadow_soft = pcf_shadow(shadow_map, shadow_coords, bias)
    
    # 计算最终光照
    let lighting = calculate_lighting(world_pos, normal)
    let final_color = lighting * shadow_soft
    
    Vec4(final_color, 1.0)
}

# PCF (Percentage Closer Filtering) 软阴影
micro pcf_shadow(shadow_map: Texture2D, shadow_coords: Vec3, bias: f32) -> f32 {
    let texel_size = 1.0 / textureDimensions(shadow_map)
    let mut shadow = 0.0
    
    for x in -1..=1 {
        for y in -1..=1 {
            let offset = Vec2(x as f32, y as f32) * texel_size
            let sample_coords = shadow_coords.xy + offset
            let pcf_depth = shadow_map.sample(shadow_sampler, sample_coords).r
            
            shadow += if shadow_coords.z - bias > pcf_depth { 0.0 } else { 1.0 }
        }
    }
    
    shadow / 9.0
}
```

## 性能优化

### GPU 性能分析

```valkyrie
# GPU 时间戳查询
class GPUProfiler {
    query_set: wgpu::QuerySet,
    query_buffer: wgpu::Buffer,
    timestamps: [f64]
    
    micro new(device: wgpu::Device) -> GPUProfiler {
        let query_set = device.create_query_set(wgpu::QuerySetDescriptor {
            label: Some("Timestamp Queries"),
            ty: wgpu::QueryType::Timestamp,
            count: 32
        })
        
        let query_buffer = device.create_buffer(wgpu::BufferDescriptor {
            label: Some("Query Buffer"),
            size: 32 * 8, # 32 queries * 8 bytes per timestamp
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false
        })
        
        GPUProfiler {
            query_set,
            query_buffer,
            timestamps: []
        }
    }
    
    micro begin_pass(self, encoder: wgpu::CommandEncoder, label: String) {
        encoder.write_timestamp(self.query_set, self.timestamps.len() as u32)
        self.timestamps.push(0.0) # 占位符
    }
    
    micro end_pass(self, encoder: wgpu::CommandEncoder) {
        encoder.write_timestamp(self.query_set, self.timestamps.len() as u32)
        self.timestamps.push(0.0) # 占位符
    }
    
    micro resolve_queries(self, encoder: wgpu::CommandEncoder) {
        encoder.resolve_query_set(
            self.query_set,
            0..self.timestamps.len() as u32,
            self.query_buffer,
            0
        )
    }
}
```

### 批处理和实例化

```valkyrie
# 实例化渲染数据
struct InstanceData {
    model_matrix: Mat4,
    color: Vec4
}

# 实例化顶点着色器
@vertex
micro instanced_vertex_main(
    vertex: VertexInput,
    instance: InstanceData,
    camera: CameraUniform
) -> VertexOutput {
    let world_position = instance.model_matrix * Vec4(vertex.position, 1.0)
    
    VertexOutput {
        clip_position: camera.view_proj * world_position,
        world_position: world_position.xyz,
        color: vertex.color * instance.color,
        uv: vertex.uv
    }
}

# 批处理管理器
class BatchRenderer {
    instance_buffer: wgpu::Buffer,
    instance_data: [InstanceData],
    max_instances: usize
    
    micro new(device: wgpu::Device, max_instances: usize) -> BatchRenderer {
        let instance_buffer = device.create_buffer(wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: (max_instances * size_of::<InstanceData>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false
        })
        
        BatchRenderer {
            instance_buffer,
            instance_data: Vec::with_capacity(max_instances),
            max_instances
        }
    }
    
    micro add_instance(mut self, transform: Mat4, color: Vec4) {
        if self.instance_data.len() < self.max_instances {
            self.instance_data.push(InstanceData {
                model_matrix: transform,
                color
            })
        }
    }
    
    micro flush(mut self, queue: wgpu::Queue) {
        if !self.instance_data.is_empty() {
            queue.write_buffer(
                self.instance_buffer,
                0,
                bytemuck::cast_slice(self.instance_data.as_slice())
            )
        }
    }
    
    micro render(self, render_pass: wgpu::RenderPass) {
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..))
        render_pass.draw_indexed(0..index_count, 0, 0..self.instance_data.len() as u32)
        
        self.instance_data.clear()
    }
}
```

## 调试和工具

### Shader 调试

```valkyrie
# 调试输出宏
macro debug_color(color: Vec3) {
    # 在调试模式下输出颜色到特殊缓冲区
    #[cfg(debug)]
    debug_output.color = Vec4(color, 1.0)
}

macro debug_value(name: String, value: f32) {
    # 在调试模式下输出数值
    #[cfg(debug)]
    debug_output.values[name] = value
}

# 带调试信息的片段着色器
@fragment
micro debug_fragment_main(input: FragmentInput) -> FragmentOutput {
    let uv = input.uv
    let color = texture.sample(sampler, uv)
    
    # 调试 UV 坐标
    debug_color(Vec3(uv, 0.0))
    debug_value("brightness", dot(color.rgb, Vec3(0.299, 0.587, 0.114)))
    
    # 可视化法线
    if debug_mode == DebugMode::Normals {
        return FragmentOutput {
            color: Vec4(input.normal * 0.5 + 0.5, 1.0)
        }
    }
    
    # 可视化深度
    if debug_mode == DebugMode::Depth {
        let depth = linearize_depth(input.depth)
        return FragmentOutput {
            color: Vec4(depth, depth, depth, 1.0)
        }
    }
    
    FragmentOutput { color }
}
```

### 热重载支持

```valkyrie
# Shader 热重载管理器
class ShaderHotReload {
    shader_files: HashMap<String, FileWatcher>,
    pipelines: HashMap<String, wgpu::RenderPipeline>,
    device: wgpu::Device
    
    micro new(device: wgpu::Device) -> ShaderHotReload {
        ShaderHotReload {
            shader_files: HashMap::new(),
            pipelines: HashMap::new(),
            device
        }
    }
    
    micro watch_shader(mut self, name: String, path: String) {
        let watcher = FileWatcher::new(path, || {
            self.reload_shader(name.clone())
        })
        
        self.shader_files.insert(name, watcher)
    }
    
    micro reload_shader(mut self, name: String) {
        try {
            let shader_source = read_file(self.shader_files[name].path)?
            let compiled_shader = compile_valkyrie_shader(shader_source)?
            
            # 重新创建管线
            let new_pipeline = create_pipeline_from_shader(compiled_shader)
            self.pipelines.insert(name.clone(), new_pipeline)
            
            print("Shader '${name}' 重载成功")
        }
        .catch {
            case e:
                print("Shader '${name}' 重载失败: ${e}")
        }
    }
}
```

## 总结

Valkyrie 语言提供了完整的图形编程解决方案：

1. **原生 Shader 支持** - 无需学习 GLSL/HLSL，直接用 Valkyrie 编写 shader
2. **现代图形 API** - 完整的 wgpu 集成和现代渲染管线支持
3. **高级图形技术** - PBR、延迟渲染、阴影映射等现代渲染技术
4. **性能优化** - 批处理、实例化、GPU 性能分析等优化工具
5. **开发体验** - 热重载、调试工具、类型安全的 GPU 编程

通过 Valkyrie 的图形编程能力，开发者可以构建高性能的游戏和图形应用，同时享受现代编程语言的所有优势。