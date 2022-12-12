# 机器学习

Valkyrie 为机器学习提供了基于 Array 和 ArrayND 的高效数值计算支持，专注于实用的机器学习算法和数据处理。

## 相关示例

- [网络爬虫](web-crawler.md) - 用于数据收集的高性能异步爬虫框架

## 核心数组类型

- `Array1D` - 一维数组，用于向量和特征处理
- `ArrayND` - 多维数组，用于矩阵运算和批量数据处理
- 内置异构计算支持，可在 CPU/GPU 间无缝切换

## 数据处理

### 基本数据操作

```valkyrie
# 创建和加载数据
let features = ArrayND::from_csv("data/features.csv")  # 特征矩阵
let labels = Array1D::from_csv("data/labels.csv")     # 标签向量

# 数据预处理
let normalized = features.normalize()  # 标准化
let centered = features - features.mean(axis=0)  # 中心化

# 数据分割
let (X_train, X_test, y_train, y_test) = train_test_split(
    features, labels, test_size=0.2, random_state=42
)
```

### 特征工程

```valkyrie
# 特征缩放
let scaled_features = features.standardize()  # 标准化 (z-score)
let normalized_features = features.normalize(min=0.0, max=1.0)  # 归一化

# 特征选择
let selected = features.select_by_variance(threshold=0.01)
let top_features = features.select_k_best(k=10, target=labels)

# 特征变换
let polynomial = features.polynomial_features(degree=2)
let log_transformed = features.log_transform()
```

## 机器学习算法

### 线性模型

```valkyrie
# 线性回归 - 使用矩阵运算
let 𝐗 = X_train.add_bias_column()  # 添加偏置列
let 𝐰 = (𝐗ᵀ · 𝐗)⁻¹ · 𝐗ᵀ · y_train  # 最小二乘解
let predictions = X_test.add_bias_column() · 𝐰

# 岭回归 - 带正则化
let λ = 0.1
let 𝐈 = ArrayND::eye(𝐗.shape()[1])
let 𝐰_ridge = (𝐗ᵀ · 𝐗 + λ * 𝐈)⁻¹ · 𝐗ᵀ · y_train

# 逻辑回归 - sigmoid激活
let σ = |z: ArrayND| 1.0 / (1.0 + (-z).exp())  # sigmoid函数
let logits = X_test · 𝐰
let probabilities = σ(logits)
```

### 聚类算法

```valkyrie
# K-Means 聚类 - 使用数组操作
let k = 3
let centroids = ArrayND::random([k, X_train.shape()[1]])  # 随机初始化质心

for iteration in 0..100 {
    # 计算距离矩阵
    let distances = X_train.cdist(centroids)  # [n_samples, k]
    let assignments = distances.argmin(axis=1)  # 最近质心索引
    
    # 更新质心
    for cluster in 0..k {
        let mask = assignments.eq(cluster)
        let cluster_points = X_train.masked_select(mask)
        if cluster_points.shape()[0] > 0 {
            centroids.row_mut(cluster).copy_from(cluster_points.mean(axis=0))
        }
    }
}
```

### K-Means 聚类

```valkyrie
# K-Means 聚类 - 使用数组操作
let k = 3
let centroids = ArrayND::random([k, X_train.shape()[1]])  # 随机初始化质心

for iteration in 0..100 {
    # 计算距离矩阵
    let distances = X_train.cdist(centroids)  # [n_samples, k]
    let assignments = distances.argmin(axis=1)  # 最近质心索引
    
    # 更新质心
    for cluster in 0..k {
        let mask = assignments.eq(cluster)
        let cluster_points = X_train.masked_select(mask)
        if cluster_points.shape()[0] > 0 {
            centroids.row_mut(cluster).copy_from(cluster_points.mean(axis=0))
        }
    }
}
```

## 降维算法

```valkyrie
# 主成分分析 (PCA) - 使用矩阵分解
let 𝐗_centered = X_train - μ  # 中心化，μ = X_train.mean(axis=0)
let Σ = 𝐗_centeredᵀ · 𝐗_centered / (X_train.shape()[0] - 1)  # 协方差矩阵
let (λ, 𝐕) = Σ.eig()  # 特征分解：λ为特征值，𝐕为特征向量

# 选择前k个主成分
let k = 2
let 𝐕_k = 𝐕.slice([.., 0..k])  # 前k个特征向量
let 𝐗_pca = 𝐗_centered · 𝐕_k  # 投影到主成分空间
```

## 模型评估

```valkyrie
# 基本评估指标 - 使用数组计算
let correct = y_true.eq(y_pred).sum()  # 正确预测数量
let accuracy = correct.to_f64() / y_true.len() as f64

# 回归指标
let ε = y_true - y_pred  # 残差
let MSE = (ε² ).mean()   # 均方误差
let MAE = |ε|.mean()     # 平均绝对误差
let RMSE = √MSE          # 均方根误差
```

## 数组操作示例

```valkyrie
# 数据分割 - 使用数组索引
let n_samples = X.shape()[0]
let train_size = (n_samples as f64 * 0.8) as usize
let indices = ArrayND::arange(n_samples).shuffle()  # 随机打乱索引

let train_indices = indices.slice([0..train_size])
let test_indices = indices.slice([train_size..])

let X_train = X.index_select(train_indices, axis=0)
let X_test = X.index_select(test_indices, axis=0)
let y_train = y.index_select(train_indices, axis=0)
let y_test = y.index_select(test_indices, axis=0)
```
        
        # 当缓冲区满时进行批量更新
## 总结

本文档展示了如何使用 Valkyrie 的 Array 和 ArrayND 类型进行机器学习任务。重点关注数组操作的实际使用，包括：

- 数据预处理和特征工程
- 基本机器学习算法的数组实现
- 聚类和降维的矩阵运算
- 模型评估的数组计算
- 数据分割和索引操作

通过这些示例，您可以了解如何利用 Valkyrie 的内置数组功能来构建机器学习应用。
for (name, (mean, std)) in results {
    print("{}: {:.3} (+/- {:.3})", name, mean, std * 2)
}
```

### 3. 特征工程自动化

```valkyrie
# 自动特征工程
let feature_engineer = AutoFeatureEngineering::new()
    .polynomial_features(degree: 2)
    .interaction_features(true)
    .log_transform(columns: vec!["income", "age"])
    .binning(column: "age", bins: 5)

let engineered_features = feature_engineer.fit_transform(X)
```

Valkyrie 的机器学习特性提供了完整的工具链，从数据预处理到模型部署，支持传统机器学习和现代深度学习方法，为数据科学家和机器学习工程师提供了强大而灵活的开发环境。