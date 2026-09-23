# Einstein 操作符 (Einops)

Valkyrie 提供了强大的 Einstein 操作符，灵感来自于 einops 库，用于优雅地处理多维数组的重排、重塑和约简操作。通过直观的字符串表示法，可以轻松表达复杂的张量操作。

## 基本概念

Einstein 操作符使用字符串模式来描述张量操作，其中：
- 字母表示维度
- 空格分隔不同的维度组
- `()` 表示新增维度
- `...` 表示省略的维度

## 重排操作 (Rearrange)

```valkyrie
use valkyrie::tensor::einops::*

# 图像数据格式转换
let images = ArrayND::random([32, 224, 224, 3])  # NHWC格式

# 转换为NCHW格式
let nchw = rearrange(images, "n h w c -> n c h w")

# 将batch展平为序列
let sequence = rearrange(images, "n h w c -> (n h w) c")

# 创建图像块 (patches)
let patches = rearrange(images, "n (h p1) (w p2) c -> n (h w) (p1 p2 c)", 
                       p1=16, p2=16)  # 16x16 patches

# 多头注意力的头部重排
let attention_weights = ArrayND::random([8, 12, 64, 64])  # batch, heads, seq, seq
let reshaped = rearrange(attention_weights, "b h s1 s2 -> (b h) s1 s2")

# 时间序列重塑
let time_series = ArrayND::random([100, 24, 7])  # days, hours, features
let weekly = rearrange(time_series, "(w d) h f -> w (d h) f", w=14, d=7)
```

## 约简操作 (Reduce)

```valkyrie
# 全局平均池化
let features = ArrayND::random([32, 512, 7, 7])  # batch, channels, height, width
let global_avg = reduce(features, "n c h w -> n c", "mean")

# 沿特定轴求和
let batch_sum = reduce(features, "n c h w -> c h w", "sum")

# 多轴约简
let channel_stats = reduce(features, "n c h w -> c", "mean")  # 每个通道的均值
let spatial_max = reduce(features, "n c h w -> n c", "max")   # 空间最大值

# 注意力权重归一化
let attention_logits = ArrayND::random([8, 12, 64, 64])
let attention_weights = reduce(attention_logits, "b h i j -> b h i j", "softmax")

# 时间序列统计
let daily_avg = reduce(time_series, "d h f -> d f", "mean")     # 每日平均
let hourly_max = reduce(time_series, "d h f -> h f", "max")    # 每小时最大值
```

## 重复操作 (Repeat)

```valkyrie
# 广播操作
let bias = ArrayND::random([512])  # 偏置向量
let broadcasted = repeat(bias, "c -> n c h w", n=32, h=7, w=7)

# 数据增强 - 重复样本
let sample = ArrayND::random([224, 224, 3])
let augmented = repeat(sample, "h w c -> n h w c", n=8)  # 创建8个副本

# 位置编码重复
let pos_encoding = ArrayND::random([64, 512])  # seq_len, d_model
let batch_pos = repeat(pos_encoding, "s d -> n s d", n=32)  # 为整个batch重复

# 卷积核重复
let kernel = ArrayND::random([3, 3])  # 2D卷积核
let multi_channel = repeat(kernel, "h w -> c_out c_in h w", c_out=64, c_in=3)
```

## 复杂操作组合

```valkyrie
# Vision Transformer patch embedding
class PatchEmbedding {
    patch_size: Integer
    embed_dim: Integer
    
    forward(self, images: ArrayND) -> ArrayND {
        # 将图像分割为patches
        let patches = rearrange(images, 
            "n (h p1) (w p2) c -> n (h w) (p1 p2 c)",
            p1=self.patch_size, p2=self.patch_size)
        
        # 线性投影到embedding维度
        let embedded = self.linear(patches)  # n (h w) embed_dim
        return embedded
    }
}

# 多尺度特征融合
micro multi_scale_fusion(features: List<ArrayND>) -> ArrayND {
    let unified_features = []
    
    for (i, feat) in features.enumerate() {
        # 统一空间尺寸
        let resized = if i == 0 {
            feat
        } else {
            # 上采样到最大尺寸
            interpolate(feat, size=[features[0].shape()[2], features[0].shape()[3]])
        }
        
        # 重排为统一格式
        let rearranged = rearrange(resized, "n c h w -> n (h w) c")
        unified_features.push(rearranged)
    }
    
    # 沿通道维度拼接
    let fused = concatenate(unified_features, axis=2)
    return rearrange(fused, "n (h w) c -> n c h w", 
                    h=features[0].shape()[2], w=features[0].shape()[3])
}

# 自注意力机制
class MultiHeadAttention {
    num_heads: Integer
    head_dim: Integer
    
    forward(self, x: ArrayND) -> ArrayND {
        let n, s, d = x.shape()
        
        # 计算Q, K, V
        let qkv = self.qkv_proj(x)  # n s (3 * num_heads * head_dim)
        let qkv_reshaped = rearrange(qkv, 
            "n s (three h d) -> three n h s d", 
            three=3, h=self.num_heads, d=self.head_dim)
        
        let q, k, v = qkv_reshaped[0], qkv_reshaped[1], qkv_reshaped[2]
        
        # 计算注意力分数
        let scores = einsum("n h i d, n h j d -> n h i j", q, k) / sqrt(self.head_dim)
        let attention = softmax(scores, axis=-1)
        
        # 应用注意力
        let out = einsum("n h i j, n h j d -> n h i d", attention, v)
        
        # 重新组合多头输出
        let combined = rearrange(out, "n h s d -> n s (h d)")
        return self.out_proj(combined)
    }
}
```

## 高级模式匹配

```valkyrie
# 动态形状处理
micro adaptive_pooling(x: ArrayND, target_size: Tuple<Integer, Integer>) -> ArrayND {
    let n, c, h, w = x.shape()
    let th, tw = target_size
    
    # 自适应池化窗口大小
    let pool_h = h / th
    let pool_w = w / tw
    
    # 使用einops进行自适应池化
    let pooled = reduce(x, 
        "n c (th ph) (tw pw) -> n c th tw", 
        "mean", th=th, tw=tw, ph=pool_h, pw=pool_w)
    
    return pooled
}

# 序列到序列的注意力
micro seq2seq_attention(encoder_out: ArrayND, decoder_hidden: ArrayND) -> ArrayND {
    # encoder_out: [batch, enc_seq, hidden]
    # decoder_hidden: [batch, dec_seq, hidden]
    
    # 计算注意力权重
    let attention_scores = einsum("b i h, b j h -> b i j", decoder_hidden, encoder_out)
    let attention_weights = softmax(attention_scores, axis=-1)
    
    # 应用注意力
    let context = einsum("b i j, b j h -> b i h", attention_weights, encoder_out)
    
    return context
}

# 图卷积网络的邻接矩阵操作
micro graph_convolution(node_features: ArrayND, adjacency: ArrayND) -> ArrayND {
    # node_features: [batch, nodes, features]
    # adjacency: [batch, nodes, nodes]
    
    # 聚合邻居特征
    let aggregated = einsum("b i j, b j f -> b i f", adjacency, node_features)
    
    # 归一化
    let degree = reduce(adjacency, "b i j -> b i", "sum")
    let degree_expanded = repeat(degree, "b i -> b i f", f=node_features.shape()[2])
    
    let normalized = aggregated / (degree_expanded + 1e-8)
    return normalized
}
```

## 性能优化

```valkyrie
# 内存高效的操作
micro memory_efficient_attention(q: ArrayND, k: ArrayND, v: ArrayND, 
                                 chunk_size: Integer = 1024) -> ArrayND {
    let b, h, s, d = q.shape()
    let output = ArrayND::zeros([b, h, s, d])
    
    # 分块计算避免大矩阵乘法
    for i in 0..s step chunk_size {
        let end_i = min(i + chunk_size, s)
        let q_chunk = q.slice(2, i..end_i)  # [b, h, chunk, d]
        
        # 计算当前chunk的注意力
        let scores = einsum("b h i d, b h j d -> b h i j", q_chunk, k)
        let attention = softmax(scores, axis=-1)
        let out_chunk = einsum("b h i j, b h j d -> b h i d", attention, v)
        
        output.slice_mut(2, i..end_i).copy_from(out_chunk)
    }
    
    return output
}

# GPU优化的批量操作
micro batch_matrix_multiply(a: ArrayND, b: ArrayND) -> ArrayND {
    # 使用einops确保正确的批量维度对齐
    let a_reshaped = rearrange(a, "... i j -> (...) i j")
    let b_reshaped = rearrange(b, "... j k -> (...) j k")
    
    # 批量矩阵乘法
    let result = einsum("b i j, b j k -> b i k", a_reshaped, b_reshaped)
    
    # 恢复原始形状
    let original_shape = a.shape()[:-2] + [a.shape()[-2], b.shape()[-1]]
    return result.reshape(original_shape)
}
```

## 最佳实践

1. **清晰的维度命名**：使用有意义的字母表示不同维度
2. **一致的约定**：在整个项目中保持维度命名的一致性
3. **性能考虑**：对于大张量操作，考虑内存使用和计算效率
4. **类型安全**：利用 Valkyrie 的类型系统确保操作的正确性
5. **文档化**：为复杂的 einops 操作添加注释说明

Einstein 操作符让复杂的张量操作变得直观和可读，是深度学习和科学计算中不可或缺的工具。