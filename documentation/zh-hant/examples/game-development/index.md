# 游戏开发

Valkyrie 提供了完整的游戏开发框架，支持2D/3D游戏、实时渲染、物理模拟、音频处理、输入管理等功能，为游戏开发者提供高性能的开发体验。

## 游戏引擎核心

### 游戏循环

```valkyrie
# 游戏引擎主循环
class GameEngine {
    window: Window,
    renderer: Renderer,
    scene: Scene,
    input: InputManager,
    audio: AudioEngine,
    physics: PhysicsWorld,
    running: bool,
    target_fps: f64,
}

impl GameEngine {
    micro new(title: &str, width: u32, height: u32) -> Self {
        let window = Window::new(title, width, height)
        let renderer = Renderer::new(&window)
        let scene = Scene::new()
        let input = InputManager::new()
        let audio = AudioEngine::new()
        let physics = PhysicsWorld::new()
        
        GameEngine {
            window,
            renderer,
            scene,
            input,
            audio,
            physics,
            running: true,
            target_fps: 60.0,
        }
    }
    
    micro run(&mut self) {
        let mut last_time = std::time::Instant::now()
        let frame_duration = std::time::Duration::from_secs_f64(1.0 / self.target_fps)
        
        while self.running {
            let current_time = std::time::Instant::now()
            let delta_time = current_time.duration_since(last_time).as_secs_f64()
            last_time = current_time
            
            # 处理输入
            self.input.update()
            if self.input.is_key_pressed(Key::Escape) {
                self.running = false
            }
            
            # 更新游戏逻辑
            self.update(delta_time)
            
            # 渲染
            self.render()
            
            # 帧率控制
            let elapsed = std::time::Instant::now().duration_since(current_time)
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed)
            }
        }
    }
    
    micro update(&mut self, delta_time: f64) {
        # 更新物理世界
        self.physics.step(delta_time)
        
        # 更新场景
        self.scene.update(delta_time)
        
        # 更新音频
        self.audio.update()
    }
    
    micro render(&mut self) {
        self.renderer.clear(Color::BLACK)
        self.renderer.render_scene(&self.scene)
        self.renderer.present()
    }
}
```

### 实体组件系统 (ECS)

```valkyrie
# 组件定义
trait Component {
    micro type_id() -> ComponentTypeId
}

#[derive(Clone, Copy)]
class Transform {
    position: Vec3
    rotation: Quaternion
    scale: Vec3
}

impl Component for Transform {
    micro type_id() -> ComponentTypeId { ComponentTypeId::Transform }
}

#[derive(Clone)]
class Sprite {
    texture: TextureHandle
    color: Color
    size: Vec2
}

impl Component for Sprite {
    micro type_id() -> ComponentTypeId { ComponentTypeId::Sprite }
}

class RigidBody {
    velocity: Vec3
    acceleration: Vec3
    mass: f32
    drag: f32
}

impl Component for RigidBody {
    micro type_id() -> ComponentTypeId { ComponentTypeId::RigidBody }
}

# 实体管理器
class EntityManager {
    entities: Vector<Entity>
    components: HashMap<ComponentTypeId, Vector<Box<dyn Component>>>
    entity_components: HashMap<EntityId, HashSet<ComponentTypeId>>
    next_entity_id: EntityId
}

impl EntityManager {
    micro new() -> Self {
        EntityManager {
            entities: Vec::new(),
            components: HashMap::new(),
            entity_components: HashMap::new(),
            next_entity_id: 0,
        }
    }
    
    micro create_entity(&mut self) -> EntityId {
        let id = self.next_entity_id
        self.next_entity_id += 1
        
        let entity = Entity { id }
        self.entities.push(entity)
        self.entity_components.insert(id, HashSet::new())
        
        id
    }
    
    micro add_component<T: Component + 'static>(&mut self, entity_id: EntityId, component: T) {
        let type_id = T::type_id()
        
        # 添加组件到存储
        self.components.entry(type_id)
            .or_insert_with(Vec::new)
            .push(Box::new(component))
        
        # 记录实体拥有的组件
        if let Some(entity_components) = self.entity_components.get_mut(&entity_id) {
            entity_components.insert(type_id)
        }
    }
}
```

## 2D 游戏开发

### 精灵和动画

```valkyrie
# 精灵管理
class SpriteSheet {
    texture: TextureHandle
    frame_width: u32
    frame_height: u32
    frames_per_row: u32
    total_frames: u32
}

impl SpriteSheet {
    micro new(texture: TextureHandle, frame_width: u32, frame_height: u32) -> Self {
        let texture_info = texture.get_info()
        let frames_per_row = texture_info.width / frame_width
        let frames_per_col = texture_info.height / frame_height
        let total_frames = frames_per_row * frames_per_col
        
        SpriteSheet {
            texture,
            frame_width,
            frame_height,
            frames_per_row,
            total_frames,
        }
    }
    
    micro get_frame_rect(&self, frame_index: u32) -> Rect {
        let row = frame_index / self.frames_per_row
        let col = frame_index % self.frames_per_row
        
        Rect {
            x: col * self.frame_width,
            y: row * self.frame_height,
            width: self.frame_width,
            height: self.frame_height,
        }
    }
}

# 动画系统
class Animation {
    frames: Vector<u32>
    frame_duration: f64
    looping: bool
    current_frame: usize
    elapsed_time: f64
}

impl Animation {
    micro new(frames: Vector<u32>, frame_duration: f64, looping: bool) -> Self {
        Animation {
            frames,
            frame_duration,
            looping,
            current_frame: 0,
            elapsed_time: 0.0,
        }
    }
    
    micro update(&mut self, delta_time: f64) -> bool {
        self.elapsed_time += delta_time
        
        if self.elapsed_time >= self.frame_duration {
            self.elapsed_time -= self.frame_duration
            self.current_frame += 1
            
            if self.current_frame >= self.frames.len() {
                if self.looping {
                    self.current_frame = 0
                } else {
                    self.current_frame = self.frames.len() - 1
                    return true  # 动画结束
                }
            }
        }
        
        false
    }
    
    micro get_current_frame(&self) -> u32 {
        self.frames[self.current_frame]
    }
}
```

## 图形编程与 Shader 开发

Valkyrie 原生支持图形编程，可以直接编写 shader 代码替代 GLSL，并提供完整的 wgpu 集成。

- [图形编程与 Shader 开发](graphics-shader.md) - 完整的图形编程指南，包括 shader 编写、wgpu 集成、高级渲染技术等
- [GPU 计算与并行编程](gpu-compute.md) - GPGPU 编程、并行算法、物理模拟、机器学习加速等

## 3D 游戏开发

### 3D 渲染管线

```valkyrie
# 3D 网格
class Mesh {
    vertices: Vector<Vertex>
    indices: Vector<u32>
    vertex_buffer: BufferHandle
    index_buffer: BufferHandle
}

class Vertex {
    position: Vec3
    normal: Vec3
    uv: Vec2
    color: Color
}

impl Mesh {
    micro create_cube(size: f32) -> Self {
        let half_size = size * 0.5
        
        let vertices = vec![
            # 前面
            Vertex { position: Vec3::new(-half_size, -half_size,  half_size), normal: Vec3::new(0.0, 0.0, 1.0), uv: Vec2::new(0.0, 0.0), color: Color::WHITE },
            Vertex { position: Vec3::new( half_size, -half_size,  half_size), normal: Vec3::new(0.0, 0.0, 1.0), uv: Vec2::new(1.0, 0.0), color: Color::WHITE },
            Vertex { position: Vec3::new( half_size,  half_size,  half_size), normal: Vec3::new(0.0, 0.0, 1.0), uv: Vec2::new(1.0, 1.0), color: Color::WHITE },
            Vertex { position: Vec3::new(-half_size,  half_size,  half_size), normal: Vec3::new(0.0, 0.0, 1.0), uv: Vec2::new(0.0, 1.0), color: Color::WHITE },
        ]
        
        let indices = vec![
            # 前面
            0, 1, 2, 2, 3, 0,
            # 其他面...
        ]
        
        Mesh::new(vertices, indices)
    }
}
```

## 音频系统

```valkyrie
# 音频引擎
class AudioEngine {
    sources: Vector<AudioSource>
    listener_position: Vec3
    master_volume: f32
}

impl AudioEngine {
    micro play_sound(&mut self, clip: AudioClip, volume: f32) {
        let mut source = AudioSource::new(clip)
        source.volume = volume
        source.play()
        self.sources.push(source)
    }
    
    micro play_sound_3d(&mut self, clip: AudioClip, position: Vec3, volume: f32) {
        let mut source = AudioSource::new(clip)
        source.volume = volume
        source.set_spatial(position, 1.0, 50.0)
        source.play()
        self.sources.push(source)
    }
}
```

## 输入管理

```valkyrie
# 输入管理器
class InputManager {
    key_states: HashMap<Key, bool>
    mouse_position: Vec2
    gamepads: HashMap<GamepadId, GamepadState>
}

impl InputManager {
    micro is_key_pressed(&self, key: Key) -> bool {
        *self.key_states.get(&key).unwrap_or(&false)
    }
    
    micro get_mouse_position(&self) -> Vec2 {
        self.mouse_position
    }
    
    micro bind_action(&mut self, action: &str, binding: InputBinding) {
        # 绑定输入动作
    }
}
```

## 游戏示例

### 简单的 2D 平台游戏

```valkyrie
class PlatformGame {
    player_entity: EntityId
    camera: Camera2D
    level: Level
    score: u32
}

impl PlatformGame {
    micro update(&mut self, input: &InputManager, delta_time: f64) {
        # 处理玩家输入
        if input.is_key_pressed(Key::Space) {
            # 跳跃逻辑
        }
        
        # 更新游戏逻辑
        self.update_physics(delta_time)
        self.check_collisions()
        self.update_camera()
    }
    
    micro render(&self, renderer: &mut Renderer) {
        renderer.set_camera_2d(self.camera.position, self.camera.zoom)
        
        # 渲染游戏对象
        self.render_level(renderer)
        self.render_player(renderer)
        self.render_ui(renderer)
    }
}
```

Valkyrie 游戏开发框架提供了完整的工具链，从底层的渲染和物理系统到高级的游戏逻辑组织，支持快速原型开发和高性能游戏制作。