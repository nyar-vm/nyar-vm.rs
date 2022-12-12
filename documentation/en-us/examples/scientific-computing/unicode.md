
## Unicode 希腊字母支持

Valkyrie 原生支持 Unicode 希腊字母，使数学表达式更加直观和符合学术标准：

```valkyrie
# 基础希腊字母变量
let α = 0.01      # alpha - 学习率
let β = 0.9       # beta - 动量参数
let γ = 0.99      # gamma - 折扣因子
let δ = 1e-6      # delta - 数值稳定性
let ε = 1e-8      # epsilon - 小量
let ζ = 0.1       # zeta - 正则化强度
let η = 0.001     # eta - 学习率调度
let θ = Matrix::random([784, 128])  # theta - 模型参数
let λ = 0.01      # lambda - 正则化系数
let μ = Array::zeros([128])         # mu - 均值
let ν = Array::ones([128])          # nu - 方差
let ξ = Random::normal(0.0, 1.0)    # xi - 随机变量
let π = 3.14159265359               # pi - 圆周率
let ρ = 0.95      # rho - 相关系数
let σ = 0.1       # sigma - 标准差
let τ = 1.0       # tau - 时间常数
let φ = 1.618     # phi - 黄金比例
let χ = Array::random([64])         # chi - 卡方分布
let ψ = Matrix::identity(128)       # psi - 波函数
let ω = 2.0 * π   # omega - 角频率

# 带下标的希腊字母
let β₁ = 0.9      # Adam优化器第一动量
let β₂ = 0.999    # Adam优化器第二动量
let σ₁ = 0.1      # 第一层标准差
let σ₂ = 0.05     # 第二层标准差
let θᵢ = Matrix::random([256, 128]) # 第i层参数
let μₜ = Array::zeros([128])        # t时刻均值
let νₜ = Array::zeros([128])        # t时刻方差

# 在神经网络中使用希腊字母
class NeuralNetwork {
    θ: Vector<Tensor>,  # 参数向量
    ∇θ: Vector<Tensor>, # 梯度向量
    μ: Vector<Tensor>,  # 动量项
    ν: Vector<Tensor>,  # 二阶动量项
}

impl NeuralNetwork {
    # 使用希腊字母的梯度下降
    micro gradient_descent(&mut self, α: f32) {
        for (θᵢ, ∇θᵢ) in self.θ.iter_mut().zip(&self.∇θ) {
            *θᵢ = θᵢ - α * ∇θᵢ
        }
    }
    
    # Adam 优化器
    micro adam_step(&mut self, α: f32, β₁: f32, β₂: f32, ε: f32, t: usize) {
        for i in 0..self.θ.len() {
            let m = β₁ * self.μ[i] + (1.0 - β₁) * self.∇θ[i]
            let v = β₂ * self.ν[i] + (1.0 - β₂) * self.∇θ[i].powi(2)
            
            let m̂ = m / (1.0 - β₁.powi(t as i32))
            let v̂ = v / (1.0 - β₂.powi(t as i32))
            
            self.θ[i] = self.θ[i] - α * m̂ / (v̂.sqrt() + ε)
            self.μ[i] = m
            self.ν[i] = v
        }
    }
    
    # 带正则化的损失函数
    micro regularized_loss(&self, ŷ: &Tensor, y: &Tensor, λ: f32) -> f32 {
        let ℒ = self.cross_entropy_loss(ŷ, y)
        let Ω = self.l2_regularization(λ)
        ℒ + Ω
    }
}

# 数学函数使用希腊字母
micro σ(x: f64) -> f64 {  # Sigmoid激活函数
    1.0 / (1.0 + (-x).exp())
}

micro φ(x: f64) -> f64 {  # 标准正态分布CDF
    0.5 * (1.0 + (x / 2.0_f64.sqrt()).erf())
}

micro ψ(x: &Array, θ: &Matrix) -> Array {  # 神经网络前向传播
    σ(x.matmul(θ))
}

# 损失函数
micro ℒ(ŷ: &Array, y: &Array) -> f64 {  # 交叉熵损失
    let n = y.len() as f64
    -((y * ŷ.log()).sum() + ((1.0 - y) * (1.0 - ŷ).log()).sum()) / n
}

# 正则化项
micro Ω(θ: &Matrix, λ: f64) -> f64 {  # L2正则化
    λ * (θ * θ).sum() / 2.0
}

# 梯度计算
micro ∇ℒ(θ: &Matrix, x: &Array, y: &Array) -> Matrix {  # 损失函数梯度
    let ŷ = ψ(x, θ)
    let δ = ŷ - y
    x.transpose().matmul(&δ)
}

# 物理常数使用希腊字母
const π = 3.14159265358979323846
const φ = 1.618033988749895  # 黄金比例
const γ = 0.5772156649015329  # 欧拉常数
const Δ = 4.669201609102990  # Feigenbaum常数

# 高斯分布
micro gaussian(x: f64, μ: f64, σ: f64) -> f64 {
    let coefficient = 1.0 / (σ * (2.0 * π).sqrt())
    let exponent = -0.5 * ((x - μ) / σ).powi(2)
    coefficient * exponent.exp()
}

# 统计分布参数
class BetaDistribution {
    α: f64,  # 形状参数1
    β: f64,  # 形状参数2
}

class GammaDistribution {
    α: f64,  # 形状参数
    β: f64,  # 率参数
}

class DirichletDistribution {
    α: Vector<f64>,  # 浓度参数向量
}

# 概率密度函数
impl BetaDistribution {
    micro pdf(&self, x: f64) -> f64 {
        let Β = gamma_function(self.α) * gamma_function(self.β) / gamma_function(self.α + self.β)
        x.powf(self.α - 1.0) * (1.0 - x).powf(self.β - 1.0) / Β
    }
}

# 优化算法中的希腊字母
class SGDOptimizer {
    α: f32  # 学习率
    μ: f32  # 动量系数
}

class AdamOptimizer {
    α: f32   # 学习率
    β₁: f32  # 一阶动量衰减率
    β₂: f32  # 二阶动量衰减率
    ε: f32   # 数值稳定性参数
}

class RMSpropOptimizer {
    α: f32  # 学习率
    ρ: f32  # 衰减率
    ε: f32  # 数值稳定性参数
}
```
