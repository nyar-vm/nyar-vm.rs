# 自动微分 (Automatic Differentiation)

Valkyrie 提供了强大的自动微分系统，支持前向模式和反向模式自动微分，为深度学习和科学计算提供精确高效的梯度计算。

## 基本概念

### 可微分变量

```valkyrie
use autodiff::*

# 创建可微分变量
let x = Variable::new(2.0)
let y = Variable::new(3.0)

# 基本运算
let z = x * y + x.pow(2)

# 计算梯度
let grad = z.backward()
let dx = grad.get(x)  # dz/dx
let dy = grad.get(y)  # dz/dy

print("dz/dx = {}, dz/dy = {}", dx, dy)
```

### 计算图

```valkyrie
# 构建计算图
let mut graph = ComputationGraph::new()

let x = graph.variable(2.0, requires_grad: true)
let y = graph.variable(3.0, requires_grad: true)

# 前向传播
let z1 = graph.add(x, y)      # z1 = x + y
let z2 = graph.mul(z1, x)     # z2 = z1 * x = (x + y) * x
let output = graph.sin(z2)    # output = sin((x + y) * x)

# 反向传播
graph.backward(output)

# 获取梯度
let grad_x = x.grad()
let grad_y = y.grad()
```

## 前向模式自动微分

```valkyrie
# 前向模式 - 适合输入维度较少的情况
class ForwardDual {
    value: f64
    derivative: f64
}

impl ForwardDual {
    micro new(value: f64, derivative: f64) -> Self {
        Self { value, derivative }
    }
    
    micro variable(value: f64) -> Self {
        Self::new(value, 1.0)  # 种子向量
    }
    
    micro constant(value: f64) -> Self {
        Self::new(value, 0.0)
    }
}

# 运算符重载
imply ForwardDual {
    infix `+`(self, other: Self) -> Self {
        new Self {
            value: self.value + other.value,
            derivative: self.derivative + other.derivative
        }
    }
    
    infix `*`(self, other: Self) -> Self {
        new Self {
            value: self.value * other.value,
            derivative: self.derivative * other.value + self.value * other.derivative
        }
    }
}

# 使用前向模式
let x = ForwardDual::variable(2.0)
let y = ForwardDual::constant(3.0)
let result = x * x + x * y  # f(x) = x² + 3x

print("f(2) = {}, f'(2) = {}", result.value, result.derivative)
```

## 反向模式自动微分

```valkyrie
# 反向模式 - 适合输出维度较少的情况
class ReverseTape {
    operations: Vec<Operation>
    variables: Vec<Variable>
}

union Operation {
    Add { inputs: [usize; 2] output: usize }
    Mul { inputs: [usize; 2] output: usize }
    Sin { input: usize output: usize }
    Exp { input: usize output: usize }
}

impl ReverseTape {
    micro new() -> Self {
        Self {
            operations: Vec::new(),
            variables: Vec::new(),
        }
    }
    
    micro variable(&mut self, value: f64) -> VariableId {
        let id = self.variables.len()
        self.variables.push(Variable::new(value))
        VariableId(id)
    }
    
    micro add(&mut self, a: VariableId, b: VariableId) -> VariableId {
        let output = self.variable(self.variables[a.0].value + self.variables[b.0].value)
        self.operations.push(Operation::Add {
            inputs: [a.0, b.0],
            output: output.0
        })
        output
    }
    
    micro backward(&mut self, output: VariableId) {
        # 初始化梯度
        let mut gradients = vec![0.0; self.variables.len()]
        gradients[output.0] = 1.0
        
        # 反向遍历操作
        for op in self.operations.iter().rev() {
            match op {
                Operation::Add { inputs, output } => {
                    gradients[inputs[0]] += gradients[*output]
                    gradients[inputs[1]] += gradients[*output]
                }
                Operation::Mul { inputs, output } => {
                    let [a, b] = *inputs
                    gradients[a] += gradients[*output] * self.variables[b].value
                    gradients[b] += gradients[*output] * self.variables[a].value
                }
                # ... 其他操作
            }
        }
        
        # 存储梯度
        for (i, grad) in gradients.iter().enumerate() {
            self.variables[i].gradient = *grad
        }
    }
}
```

## 高阶导数

```valkyrie
# 计算高阶导数
let x = Variable::new(2.0)
let y = x.pow(4) + 3.0 * x.pow(3) + 2.0 * x.pow(2) + x + 1.0

# 一阶导数
let dy_dx = y.grad(x)

# 二阶导数
let d2y_dx2 = dy_dx.grad(x)

# 三阶导数
let d3y_dx3 = d2y_dx2.grad(x)

print("f'(x) = {}", dy_dx.eval_at(x, 2.0))
print("f''(x) = {}", d2y_dx2.eval_at(x, 2.0))
print("f'''(x) = {}", d3y_dx3.eval_at(x, 2.0))
```

## 向量化自动微分

```valkyrie
# 向量和矩阵的自动微分
let x = VectorVariable::new([1.0, 2.0, 3.0])
let W = MatrixVariable::new([
    [0.1, 0.2, 0.3],
    [0.4, 0.5, 0.6]
])
let b = VectorVariable::new([0.1, 0.2])

# 线性变换
let y = W.matmul(x) + b

# 非线性激活
let z = y.sigmoid()

# 损失函数
let target = VectorVariable::new([0.8, 0.3])
let loss = (z - target).pow(2).sum()

# 计算梯度
loss.backward()

let grad_W = W.grad()  # 权重梯度
let grad_b = b.grad()  # 偏置梯度
let grad_x = x.grad()  # 输入梯度
```

## 神经网络层的自动微分

```valkyrie
# 全连接层
class LinearLayer {
    weight: MatrixVariable
    bias: VectorVariable
}

impl LinearLayer {
    micro new(input_size: usize, output_size: usize) -> Self {
        Self {
            weight: MatrixVariable::random([output_size, input_size]),
            bias: VectorVariable::zeros(output_size),
        }
    }
    
    micro forward(&self, input: VectorVariable) -> VectorVariable {
        self.weight.matmul(input) + self.bias
    }
}

# 激活函数
trait Activation {
    micro forward(&self, x: VectorVariable) -> VectorVariable
}

class ReLU;
imply ReLU: Activation {
    micro forward(&self, x: VectorVariable) -> VectorVariable {
        x.max(VectorVariable::zeros(x.len()))
    }
}

class Sigmoid;
imply Sigmoid: Activation {
    micro forward(&self, x: VectorVariable) -> VectorVariable {
        1.0 / (1.0 + (-x).exp())
    }
}

# 多层感知机
class MLP {
    layers: Vec<LinearLayer>
    activations: Vec<Box<dyn Activation>>
}

imply MLP {
    micro forward(&self, mut x: VectorVariable) -> VectorVariable {
        for (layer, activation) in zip(self.layers, self.activations) {
            x = layer.forward(x)
            x = activation.forward(x)
        }
        x
    }
}
```

## 卷积层的自动微分

```valkyrie
# 卷积操作
class Conv2D {
    kernel: TensorVariable  # [out_channels, in_channels, kernel_h, kernel_w]
    bias: VectorVariable
    stride: [usize; 2]
    padding: [usize; 2]
}

imply Conv2D {
    micro forward(&self, input: TensorVariable) -> TensorVariable {
        # input: [batch, in_channels, height, width]
        let output = input.conv2d(self.kernel, self.stride, self.padding)
        output + self.bias.unsqueeze([0, 2, 3])  # 广播偏置
    }
}

# 池化层
class MaxPool2D {
    kernel_size: [usize; 2]
    stride: [usize; 2]
}

imply MaxPool2D {
    micro forward(&self, input: TensorVariable) -> TensorVariable {
        input.max_pool2d(self.kernel_size, self.stride)
    }
}
```

## 损失函数

```valkyrie
# 均方误差损失
micro mse_loss(predictions: VectorVariable, targets: VectorVariable) -> Variable {
    (predictions - targets).pow(2).mean()
}

# 交叉熵损失
micro cross_entropy_loss(logits: VectorVariable, targets: VectorVariable) -> Variable {
    let softmax = logits.softmax()
    -(targets * softmax.log()).sum()
}

# 二元交叉熵损失
micro binary_cross_entropy_loss(predictions: VectorVariable, targets: VectorVariable) -> Variable {
    -(targets * predictions.log() + (1.0 - targets) * (1.0 - predictions).log()).mean()
}
```

## 优化器集成

```valkyrie
# SGD优化器
class SGD {
    learning_rate: f64
    momentum: f64
    velocity: HashMap<VariableId, Tensor>
}

imply SGD {
    micro step(&mut self, parameters: &[Variable]) {
        for param in parameters {
            if let Some(grad) = param.grad() {
                # 动量更新
                let velocity = self.velocity.entry(param.id())
                    .or_insert_with(|| Tensor::zeros_like(param.data()));
                
                *velocity = self.momentum * velocity.clone() + grad
                
                # 参数更新
                param.data_mut().sub_assign(self.learning_rate * velocity)
                
                # 清零梯度
                param.zero_grad()
            }
        }
    }
}

# Adam优化器
class Adam {
    learning_rate: f64
    beta1: f64
    beta2: f64
    epsilon: f64
    t: i32  # 时间步
    m: HashMap<VariableId, Tensor>  # 一阶矩估计
    v: HashMap<VariableId, Tensor>  # 二阶矩估计
}

impl Adam {
    micro step(&mut self, parameters: &[Variable]) {
        self.t += 1
        
        for param in parameters {
            if let Some(grad) = param.grad() {
                let m = self.m.entry(param.id())
                    .or_insert_with(|| Tensor::zeros_like(param.data()));
                let v = self.v.entry(param.id())
                    .or_insert_with(|| Tensor::zeros_like(param.data()));
                
                # 更新偏置一阶矩估计
                *m = self.beta1 * m.clone() + (1.0 - self.beta1) * grad
                
                # 更新偏置二阶矩估计
                *v = self.beta2 * v.clone() + (1.0 - self.beta2) * grad.pow(2)
                
                # 偏置校正
                let m_hat = m.clone() / (1.0 - self.beta1.pow(self.t as f64))
                let v_hat = v.clone() / (1.0 - self.beta2.pow(self.t as f64))
                
                # 参数更新
                param.data_mut().sub_assign(
                    self.learning_rate * m_hat / (v_hat.sqrt() + self.epsilon)
                )
                
                param.zero_grad()
            }
        }
    }
}
```

## 训练配置与循环

```valkyrie
# 训练配置类
class TrainingConfig {
    epochs: usize
    batch_size: usize
    learning_rate: f64
    optimizer_type: String
}

imply TrainingConfig {
    micro default() -> Self {
        Self {
            epochs: 10,
            batch_size: 32,
            learning_rate: 0.001,
            optimizer_type: "Adam",
        }
    }
}

# 为 MLP 实现训练方法
imply MLP {
    micro train(&mut self, 
                config: TrainingConfig, 
                train_data: &[(VectorVariable, VectorVariable)],
                optimizer: &mut dyn Optimizer) {
        for epoch in 0..config.epochs {
            let mut total_loss = 0.0
            
            for (input, target) in train_data {
                # 前向传播
                let prediction = self.forward(input.clone())
                let loss = mse_loss(prediction, target.clone())
                
                # 反向传播
                loss.backward()
                
                # 参数更新
                optimizer.step(self.parameters())
                
                total_loss += loss.value()
            }
            
            print("Epoch {}: Loss = {}", epoch, total_loss / train_data.len() as f64)
        }
    }
}
```

## 神经网络类型集成

基于自动微分系统，Valkyrie 提供了专门的神经网络类型，简化深度学习模型的构建和训练：

```valkyrie
# 神经网络类型定义
neural LinearRegression {
    weights: TensorVariable,
    bias: Variable,
    
    new(input_size: usize) {
        self.weights = TensorVariable::random([input_size])
        self.bias = Variable::new(0.0)
    }
    
    forward(self, input: TensorVariable) -> Variable {
        input.dot(self.weights) + self.bias
    }
    
    loss(self, predicted: Variable, target: Variable) -> Variable {
        (predicted - target).pow(2).mean()
    }
}

# 多层神经网络
neural MultiLayerPerceptron {
    layers: [LinearLayer],
    activation: ActivationFunction,
    
    new(layer_sizes: [usize], activation: ActivationFunction) {
        self.layers = []
        self.activation = activation
        
        for i in 0..layer_sizes.len() - 1 {
            let layer = LinearLayer::new(layer_sizes[i], layer_sizes[i + 1])
            self.layers.push(layer)
        }
    }
    
    forward(self, mut input: TensorVariable) -> TensorVariable {
        for layer in self.layers {
            input = layer.forward(input)
            input = self.activation.apply(input)
        }
        input
    }
    
    # 自动微分支持的反向传播
    backward(mut self, loss_gradient: TensorVariable) {
        # 梯度会自动通过计算图传播
        loss_gradient.backward()
    }
}
```

## 性能优化

### 计算图优化

```valkyrie
# 计算图融合
class GraphOptimizer {
    fusion_rules: Vec<FusionRule>
}

imply GraphOptimizer {
    micro optimize(&self, graph: &mut ComputationGraph) {
        # 算子融合
        self.fuse_operations(graph)
        
        # 内存优化
        self.optimize_memory(graph)
        
        # 并行化
        self.parallelize(graph)
    }
    
    micro fuse_operations(&self, graph: &mut ComputationGraph) {
        # 融合连续的线性操作
        # 例如：MatMul + Add -> FusedLinear
        for rule in &self.fusion_rules {
            rule.apply(graph)
        }
    }
}
```

### 内存管理

```valkyrie
# 梯度检查点
class GradientCheckpointing {
    checkpoint_layers: Vec<usize>
}

imply GradientCheckpointing {
    micro forward_with_checkpointing(&self, model: &MLP, input: TensorVariable) -> TensorVariable {
        let mut activations = vec![input]
        let mut checkpoints = HashMap::new()
        
        for (i, layer) in model.layers.iter().enumerate() {
            let output = layer.forward(activations.last().unwrap().clone())
            
            if self.checkpoint_layers.contains(&i) {
                checkpoints.insert(i, output.detach())  # 分离计算图
            }
            
            activations.push(output)
        }
        
        activations.into_iter().last().unwrap()
    }
}
```

## 最佳实践

### 1. 数值稳定性

```valkyrie
# 数值稳定的softmax
micro stable_softmax(logits: TensorVariable) -> TensorVariable {
    let max_logits = logits.max(dim: -1, keepdim: true)
    let shifted = logits - max_logits
    let exp_shifted = shifted.exp()
    exp_shifted / exp_shifted.sum(dim: -1, keepdim: true)
}

# 数值稳定的log-sum-exp
micro log_sum_exp(x: TensorVariable) -> Variable {
    let max_x = x.max()
    max_x + (x - max_x).exp().sum().log()
}
```

### 2. 梯度裁剪

```valkyrie
# 梯度范数裁剪
micro clip_grad_norm(parameters: &[Variable], max_norm: f64) {
    let total_norm = parameters.iter()
        .filter_map(|p| p.grad())
        .map(|g| g.norm().pow(2))
        .sum::<f64>()
        .sqrt()
    
    if total_norm > max_norm {
        let clip_coef = max_norm / total_norm
        for param in parameters {
            if let Some(grad) = param.grad_mut() {
                *grad *= clip_coef
            }
        }
    }
}
```

### 3. 内存效率

```valkyrie
# 就地操作减少内存分配
micro efficient_update(param: &mut TensorVariable, grad: &TensorVariable, lr: f64) {
    param.sub_assign(lr * grad)  # 就地更新，避免临时张量
}

# 梯度累积
class GradientAccumulator {
    accumulated_steps: usize
    target_steps: usize
}

impl GradientAccumulator {
    micro accumulate_and_step(&mut self, loss: Variable, optimizer: &mut dyn Optimizer, parameters: &[Variable]) {
        # 缩放损失
        let scaled_loss = loss / self.target_steps as f64
        scaled_loss.backward()
        
        self.accumulated_steps += 1
        
        if self.accumulated_steps >= self.target_steps {
            optimizer.step(parameters)
            self.accumulated_steps = 0
        }
    }
}
```

Valkyrie 的自动微分系统为深度学习提供了强大而高效的梯度计算能力，支持复杂的神经网络架构和训练策略，同时保持了良好的性能和数值稳定性。