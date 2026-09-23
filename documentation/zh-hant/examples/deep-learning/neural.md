# 神经网络类型 (Neural)

神经网络类型是 Valkyrie 中专门为机器学习设计的特殊类类型。它提供了构建、训练和推理神经网络的高级抽象，简化了机器学习模型的开发过程。

## 基本神经网络定义

### 简单神经网络

```valkyrie
# 基本神经网络定义
neural LinearRegression {
    # 网络参数
    weights: Tensor<f32>,
    bias: f32,
    
    # 构造函数
    new(input_size: usize) {
        self.weights = Tensor::random([input_size])
        self.bias = 0.0
    }
    
    # 前向传播
    forward(self, input: Tensor<f32>) -> Tensor<f32> {
        input.dot(self.weights) + self.bias
    }
    
    # 损失函数
    loss(self, predicted: Tensor<f32>, target: Tensor<f32>) -> f32 {
        (predicted - target).pow(2).mean()
    }
}
```

### 多层神经网络

```valkyrie
neural MultiLayerPerceptron {
    layers: [Layer],
    activation: ActivationFunction,
    
    new(layer_sizes: [usize], activation: ActivationFunction) {
        self.layers = []
        self.activation = activation
        
        for i in 0..layer_sizes.len() - 1 {
            let layer = Layer::new(layer_sizes[i], layer_sizes[i + 1])
            self.layers.push(layer)
        }
    }
    
    forward(self, mut input: Tensor<f32>) -> Tensor<f32> {
        for layer in self.layers {
            input = layer.forward(input)
            input = self.activation.apply(input)
        }
        input
    }
    
    # 反向传播
    backward(mut self, loss_gradient: Tensor<f32>) {
        let mut grad = loss_gradient
        
        for layer in self.layers.reverse() {
            grad = layer.backward(grad)
        }
    }
}
```

## 卷积神经网络

```valkyrie
neural ConvolutionalNetwork {
    conv_layers: [ConvLayer],
    pool_layers: [PoolLayer],
    fc_layers: [FullyConnectedLayer],
    
    new(config: CNNConfig) {
        self.conv_layers = config.build_conv_layers()
        self.pool_layers = config.build_pool_layers()
        self.fc_layers = config.build_fc_layers()
    }
    
    forward(self, input: Tensor<f32>) -> Tensor<f32> {
        let mut x = input
        
        # 卷积和池化层
        for (conv, pool) in zip(self.conv_layers, self.pool_layers) {
            x = conv.forward(x)
            x = pool.forward(x)
        }
        
        # 展平
        x = x.flatten()
        
        # 全连接层
        for fc in self.fc_layers {
            x = fc.forward(x)
        }
        
        x
    }
    
    # 特征提取
    extract_features(self, input: Tensor<f32>) -> Tensor<f32> {
        let mut x = input
        
        for (conv, pool) in zip(self.conv_layers, self.pool_layers) {
            x = conv.forward(x)
            x = pool.forward(x)
        }
        
        x.flatten()
    }
}
```

## 循环神经网络

```valkyrie
neural RecurrentNetwork {
    hidden_size: usize,
    input_size: usize,
    output_size: usize,
    
    # RNN 参数
    w_ih: Tensor<f32>,  # input to hidden
    w_hh: Tensor<f32>,  # hidden to hidden
    w_ho: Tensor<f32>,  # hidden to output
    
    hidden_state: Tensor<f32>,
    
    new(input_size: usize, hidden_size: usize, output_size: usize) {
        self.input_size = input_size
        self.hidden_size = hidden_size
        self.output_size = output_size
        
        self.w_ih = Tensor::xavier_uniform([input_size, hidden_size])
        self.w_hh = Tensor::xavier_uniform([hidden_size, hidden_size])
        self.w_ho = Tensor::xavier_uniform([hidden_size, output_size])
        
        self.reset_hidden()
    }
    
    forward(mut self, input: Tensor<f32>) -> Tensor<f32> {
        # h_t = tanh(W_ih * x_t + W_hh * h_{t-1})
        self.hidden_state = tanh(
            input.matmul(self.w_ih) + self.hidden_state.matmul(self.w_hh)
        )
        
        # 输出
        self.hidden_state.matmul(self.w_ho)
    }
    
    reset_hidden(mut self) {
        self.hidden_state = Tensor::zeros([self.hidden_size])
    }
    
    # 序列处理
    forward_sequence(mut self, sequence: [Tensor<f32>]) -> [Tensor<f32>] {
        let mut outputs = []
        
        for input in sequence {
            let output = self.forward(input)
            outputs.push(output)
        }
        
        outputs
    }
}
```

## 训练和优化

### 训练器

```valkyrie
neural Trainer<N> where N: Neural {
    model: N,
    optimizer: Optimizer,
    loss_function: LossFunction,
    
    new(model: N, optimizer: Optimizer, loss_function: LossFunction) {
        self.model = model
        self.optimizer = optimizer
        self.loss_function = loss_function
    }
    
    # 单步训练
    train_step(mut self, input: Tensor<f32>, target: Tensor<f32>) -> f32 {
        # 前向传播
        let prediction = self.model.forward(input)
        
        # 计算损失
        let loss = self.loss_function.compute(prediction, target)
        
        # 反向传播
        let loss_grad = self.loss_function.gradient(prediction, target)
        self.model.backward(loss_grad)
        
        # 更新参数
        self.optimizer.step(self.model.parameters())
        
        loss
    }
    
    # 批量训练
    train_epoch(mut self, dataloader: DataLoader) -> f32 {
        let mut total_loss = 0.0
        let mut batch_count = 0
        
        for (input, target) in dataloader {
            let loss = self.train_step(input, target)
            total_loss += loss
            batch_count += 1
        }
        
        total_loss / batch_count as f32
    }
    
    # 验证
    validate(self, dataloader: DataLoader) -> f32 {
        let mut total_loss = 0.0
        let mut batch_count = 0
        
        for (input, target) in dataloader {
            let prediction = self.model.forward(input)
            let loss = self.loss_function.compute(prediction, target)
            total_loss += loss
            batch_count += 1
        }
        
        total_loss / batch_count as f32
    }
}
```

### 优化器

```valkyrie
neural SGDOptimizer {
    learning_rate: f32,
    momentum: f32,
    velocity: {String: Tensor<f32>},
    
    new(learning_rate: f32, momentum: f32 = 0.0) {
        self.learning_rate = learning_rate
        self.momentum = momentum
        self.velocity = {}
    }
    
    step(mut self, parameters: {String: Parameter}) {
        for (name, param) in parameters {
            if !self.velocity.contains_key(name) {
                self.velocity[name] = Tensor::zeros_like(param.gradient)
            }
            
            # 动量更新
            self.velocity[name] = self.momentum * self.velocity[name] + param.gradient
            
            # 参数更新
            param.data -= self.learning_rate * self.velocity[name]
            
            # 清零梯度
            param.gradient.zero_()
        }
    }
}

neural AdamOptimizer {
    learning_rate: f32,
    beta1: f32,
    beta2: f32,
    epsilon: f32,
    
    m: {String: Tensor<f32>},  # 一阶矩估计
    v: {String: Tensor<f32>},  # 二阶矩估计
    t: i32,  # 时间步
    
    new(learning_rate: f32 = 0.001, beta1: f32 = 0.9, beta2: f32 = 0.999, epsilon: f32 = 1e-8) {
        self.learning_rate = learning_rate
        self.beta1 = beta1
        self.beta2 = beta2
        self.epsilon = epsilon
        self.m = {}
        self.v = {}
        self.t = 0
    }
    
    step(mut self, parameters: {String: Parameter}) {
        self.t += 1
        
        for (name, param) in parameters {
            if !self.m.contains_key(name) {
                self.m[name] = Tensor::zeros_like(param.gradient)
                self.v[name] = Tensor::zeros_like(param.gradient)
            }
            
            # 更新偏置一阶矩估计
            self.m[name] = self.beta1 * self.m[name] + (1.0 - self.beta1) * param.gradient
            
            # 更新偏置二阶矩估计
            self.v[name] = self.beta2 * self.v[name] + (1.0 - self.beta2) * param.gradient.pow(2)
            
            # 偏置校正
            let m_hat = self.m[name] / (1.0 - self.beta1.pow(self.t as f32))
            let v_hat = self.v[name] / (1.0 - self.beta2.pow(self.t as f32))
            
            # 参数更新
            param.data -= self.learning_rate * m_hat / (v_hat.sqrt() + self.epsilon)
            
            # 清零梯度
            param.gradient.zero_()
        }
    }
}
```

## 预训练模型

```valkyrie
neural PretrainedModel {
    backbone: ConvolutionalNetwork,
    classifier: FullyConnectedLayer,
    
    # 加载预训练模型
    from_pretrained(model_path: String) -> Self {
        let checkpoint = load_checkpoint(model_path)
        let mut model = Self::new(checkpoint.config)
        model.load_state_dict(checkpoint.state_dict)
        model
    }
    
    # 微调
    fine_tune(mut self, num_classes: usize, freeze_backbone: bool = true) {
        if freeze_backbone {
            self.backbone.freeze_parameters()
        }
        
        # 替换分类器
        let feature_size = self.backbone.get_output_size()
        self.classifier = FullyConnectedLayer::new(feature_size, num_classes)
    }
    
    forward(self, input: Tensor<f32>) -> Tensor<f32> {
        let features = self.backbone.extract_features(input)
        self.classifier.forward(features)
    }
}
```

## 模型保存和加载

```valkyrie
neural ModelCheckpoint {
    # 保存模型
    save<N>(model: N, path: String) where N: Neural {
        let state_dict = model.state_dict()
        let config = model.get_config()
        
        let checkpoint = Checkpoint {
            state_dict,
            config,
            timestamp: now(),
        }
        
        serialize_to_file(checkpoint, path)
    }
    
    # 加载模型
    load<N>(path: String) -> N where N: Neural {
        let checkpoint: Checkpoint = deserialize_from_file(path)
        let mut model = N::new(checkpoint.config)
        model.load_state_dict(checkpoint.state_dict)
        model
    }
}
```

## 最佳实践

### 1. 模型设计原则

```valkyrie
# 模块化设计
neural ResNetBlock {
    conv1: ConvLayer,
    conv2: ConvLayer,
    shortcut: Option<ConvLayer>,
    
    new(in_channels: usize, out_channels: usize, stride: usize = 1) {
        self.conv1 = ConvLayer::new(in_channels, out_channels, 3, stride, 1)
        self.conv2 = ConvLayer::new(out_channels, out_channels, 3, 1, 1)
        
        if stride != 1 || in_channels != out_channels {
            self.shortcut = Some(ConvLayer::new(in_channels, out_channels, 1, stride, 0))
        } else {
            self.shortcut = None
        }
    }
    
    forward(self, input: Tensor<f32>) -> Tensor<f32> {
        let mut out = self.conv1.forward(input)
        out = relu(out)
        out = self.conv2.forward(out)
        
        let shortcut = if let Some(sc) = self.shortcut {
            sc.forward(input)
        } else {
            input
        }
        
        relu(out + shortcut)
    }
}
```

### 2. 数据预处理

```valkyrie
neural DataPreprocessor {
    mean: Tensor<f32>,
    std: Tensor<f32>,
    
    new(mean: [f32], std: [f32]) {
        self.mean = Tensor::from(mean)
        self.std = Tensor::from(std)
    }
    
    normalize(self, input: Tensor<f32>) -> Tensor<f32> {
        (input - self.mean) / self.std
    }
    
    denormalize(self, input: Tensor<f32>) -> Tensor<f32> {
        input * self.std + self.mean
    }
}
```

### 3. 模型评估

```valkyrie
neural ModelEvaluator {
    # 分类准确率
    accuracy(predictions: Tensor<f32>, targets: Tensor<i32>) -> f32 {
        let predicted_classes = predictions.argmax(dim: 1)
        let correct = (predicted_classes == targets).sum()
        correct as f32 / targets.len() as f32
    }
    
    # 混淆矩阵
    confusion_matrix(predictions: Tensor<f32>, targets: Tensor<i32>, num_classes: usize) -> Tensor<i32> {
        let predicted_classes = predictions.argmax(dim: 1)
        let mut matrix = Tensor::zeros([num_classes, num_classes])
        
        for (pred, target) in zip(predicted_classes, targets) {
            matrix[target as usize][pred as usize] += 1
        }
        
        matrix
    }
}
```

Neural 类型为 Valkyrie 提供了强大的机器学习能力，通过高级抽象简化了神经网络的构建和训练过程，同时保持了足够的灵活性来支持各种复杂的模型架构。