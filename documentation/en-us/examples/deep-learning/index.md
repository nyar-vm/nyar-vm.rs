# 深度学习

Valkyrie 为深度学习提供了现代化的编程体验，专注于神经网络的构建和训练。本文档重点介绍深度学习特有的概念和技术。

## 核心特性

- **自动微分**: 内置自动微分系统，支持前向和反向模式
- **神经网络层**: 丰富的预定义层和自定义层支持
- **优化器**: 多种优化算法实现
- **损失函数**: 常用损失函数的高效实现

## 张量与梯度

### 可微分张量

```valkyrie
# 创建需要梯度的张量
let x = Tensor::random([32, 784]).requires_grad()
let w = Tensor::random([784, 128]).requires_grad()
let b = Tensor::zeros([128]).requires_grad()

# 前向传播
let y = x.matmul(&w) + &b
let activated = y.relu()
```

### 梯度计算

```valkyrie
# 计算损失函数的梯度
let loss = cross_entropy(&predictions, &targets)

# 反向传播计算梯度
loss.backward()

# 获取参数梯度
let grad_w = w.grad()  # 权重梯度
let grad_b = b.grad()  # 偏置梯度

# 梯度清零（为下次迭代准备）
w.zero_grad()
b.zero_grad()
```

## 神经网络层

### 全连接层

```valkyrie
# 创建全连接层
let linear = Linear::new(784, 128)  # 输入784维，输出128维

# 前向传播
let output = linear.forward(&input)

# 带激活函数的层
let hidden = linear.forward(&input).relu()
let output = output_layer.forward(&hidden).softmax()
```

### 卷积层

```valkyrie
# 2D卷积层
let conv = Conv2d::new(
    in_channels: 3,
    out_channels: 64,
    kernel_size: 3,
    stride: 1,
    padding: 1
)

# 卷积操作
let feature_maps = conv.forward(&images)  # [N, 3, H, W] -> [N, 64, H, W]

# 池化层
let pooled = feature_maps.max_pool2d(kernel_size: 2, stride: 2)
```
```

## 优化器

### SGD 优化器

```valkyrie
# 随机梯度下降
let mut optimizer = SGD::new(
    parameters: model.parameters(),
    learning_rate: 0.01,
    momentum: 0.9
)

# 优化步骤
optimizer.zero_grad()
loss.backward()
optimizer.step()
```

### Adam 优化器

```valkyrie
# Adam 优化器
let mut optimizer = Adam::new(
    parameters: model.parameters(),
    learning_rate: 0.001,
    beta1: 0.9,
    beta2: 0.999
)

# 学习率调度
let scheduler = StepLR::new(&optimizer, step_size: 30, gamma: 0.1)
scheduler.step()  # 每30个epoch降低学习率
```

## 损失函数

```valkyrie
# 交叉熵损失（分类）
let loss = cross_entropy(&logits, &targets)

# 均方误差损失（回归）
let loss = mse_loss(&predictions, &targets)

# 二元交叉熵损失
let loss = binary_cross_entropy(&sigmoid_output, &binary_targets)
```

## 深度学习模型

### 卷积神经网络

```valkyrie
# 构建CNN模型
let mut model = Sequential::new()
    .add(Conv2d::new(3, 32, 3))  # 输入通道3，输出通道32，卷积核3x3
    .add(ReLU::new())
    .add(MaxPool2d::new(2))       # 2x2最大池化
    .add(Conv2d::new(32, 64, 3))
    .add(ReLU::new())
    .add(MaxPool2d::new(2))
    .add(Flatten::new())
    .add(Linear::new(64 * 7 * 7, 128))
    .add(ReLU::new())
    .add(Linear::new(128, 10))    # 10类分类

# 前向传播
let predictions = model.forward(&images)
```

### 循环神经网络

```valkyrie
# LSTM网络
let lstm = LSTM::new(
    input_size: 100,
    hidden_size: 256,
    num_layers: 2,
    dropout: 0.2
)

# 处理序列数据
let (output, (hidden, cell)) = lstm.forward(&sequence, None)
let predictions = linear.forward(&output[:, -1, :])  # 使用最后时刻的输出
```
```

## 训练流程

### 完整训练循环

```valkyrie
# 训练函数
micro train_model(model: &mut Sequential, train_loader: &DataLoader, epochs: usize) {
    let mut optimizer = Adam::new(model.parameters(), 0.001)
    
    for epoch in 0..epochs {
        let mut total_loss = 0.0
        let mut num_batches = 0
        
        for (batch_x, batch_y) in train_loader {
            # 前向传播
            let predictions = model.forward(&batch_x)
            let loss = cross_entropy(&predictions, &batch_y)
            
            # 反向传播
            optimizer.zero_grad()
            loss.backward()
            optimizer.step()
            
            total_loss += loss.item()
            num_batches += 1
        }
        
        let avg_loss = total_loss / num_batches as f32
        print("Epoch {}: Average Loss = {:.4}", epoch + 1, avg_loss)
    }
}
```

### 模型评估

```valkyrie
# 评估函数
micro evaluate_model(model: &Sequential, test_loader: &DataLoader) -> f32 {
    model.eval()  # 设置为评估模式
    let mut correct = 0
    let mut total = 0
    
    with_no_grad(|| {
        for (batch_x, batch_y) in test_loader {
            let predictions = model.forward(&batch_x)
            let predicted_classes = predictions.argmax(dim: 1)
            
            correct += (predicted_classes == batch_y).sum().item() as usize
            total += batch_y.size(0)
        }
    })
    
    correct as f32 / total as f32
}
```
## 文档导航

### [自动微分](auto-differentiation.md)
详细介绍 Valkyrie 的自动微分系统，包括前向模式、反向模式和计算图构建。

### [Einstein 操作符](einstein-operators.md)
学习使用 Einstein 记号进行张量操作，包括重排、约简和复杂的张量变换。

## 总结

本文档展示了 Valkyrie 深度学习框架的核心功能：

- **自动微分**: 自动计算梯度，支持复杂的神经网络训练
- **神经网络层**: 丰富的预定义层，包括全连接层、卷积层、循环层等
- **优化器**: SGD、Adam 等优化算法的高效实现
- **损失函数**: 常用损失函数，支持分类和回归任务
- **训练流程**: 完整的模型训练和评估流程

Valkyrie 专注于提供直观易用的深度学习 API，让开发者能够快速构建和训练神经网络模型。