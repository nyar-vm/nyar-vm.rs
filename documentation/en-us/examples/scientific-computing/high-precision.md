# 高精度浮点数

Valkyrie 提供了多种高精度数值类型，用于需要极高精度的科学计算场景。

## 基本高精度类型

### Integer 和 Decimal 类型

```valkyrie
# 原生大整数支持
let big_int: Integer = 123456789012345678901234567890
let another_big: Integer = 999999999999999999999999999999999999999

# 原生高精度十进制数
let pi: Decimal = 3.141592653589793238462643383279502884197
let e: Decimal = 2.718281828459045235360287471352662497757

# 金融专用 d128 类型
let price: d128 = 19.99
let interest_rate: d128 = 0.05

# 基本运算
let sum = pi + e
let product = pi * e
let power = pi.pow(e)

# 字符串解析方式（可选）
let parsed_decimal = Decimal::parse("123.456789")
let parsed_integer = Integer::parse("987654321098765432109876543210")
```

### 金融计算示例

```valkyrie
# 十进制精确计算，避免二进制浮点误差
let price: Decimal = 19.99
let tax_rate: Decimal = 0.08
let total = price * (1.0 + tax_rate)

# 使用 d128 进行金融计算
let principal: d128 = 10000.00
let interest_rate: d128 = 0.05
let years = 10

let compound_interest = principal * (1.0 + interest_rate).pow(years)

# 类型限制示例
# let overflow: i128 = 12222222222222222222222222222222  # 编译错误：超出i128范围
let big_number: Integer = 12222222222222222222222222222222  # 正确：使用Integer类型
```

## 数学函数库

### 三角函数

```valkyrie
use math::high_precision::*

let angle = pi / 4.0  # π/4

# 高精度三角函数
let sin_val = sin(angle)
let cos_val = cos(angle)
let tan_val = tan(angle)

# 反三角函数
let asin_val = asin(sin_val)
let acos_val = acos(cos_val)
let atan_val = atan(tan_val)
```

### 指数和对数函数

```valkyrie
let x: Decimal = 2.0

# 指数函数
let exp_x = exp(x)  # e^x
let exp2_x = exp2(x)  # 2^x
let exp10_x = exp10(x)  # 10^x

# 对数函数
let ln_x = ln(x)  # 自然对数
let log2_x = log2(x)  # 以2为底
let log10_x = log10(x)  # 以10为底
```

### 特殊函数

```valkyrie
# 伽马函数
let gamma_val = gamma(1.5: Decimal)

# 贝塞尔函数
let bessel_j0 = bessel_j0(1.0: Decimal)
let bessel_y0 = bessel_y0(1.0: Decimal)

# 椭圆积分
let elliptic_k = elliptic_k(0.5: Decimal)
let elliptic_e = elliptic_e(0.5: Decimal)
```

## 数值积分

```valkyrie
use numerical::integration::*

# 定义被积函数
let f = { $x: Decimal -> exp(-$x * $x) }  # e^(-x²)

# 高斯积分
let gauss_result = gauss_legendre(f, 0.0: Decimal, 1.0: Decimal, 64)

# 自适应积分
let adaptive_result = adaptive_simpson(f, 0.0: Decimal, 1.0: Decimal, 1e-15: Decimal)

# 多重积分
let double_integral = { $x: Decimal, $y: Decimal -> $x * $x + $y * $y }
let result_2d = integrate_2d(double_integral, 
    0.0: Decimal, 1.0: Decimal,
    0.0: Decimal, 1.0: Decimal)
```

## 微分方程求解

```valkyrie
use numerical::ode::*

# 定义微分方程 dy/dx = -y
let ode_func = { $x: Decimal, $y: Decimal -> -$y }

# 初始条件
let x0: Decimal = 0.0
let y0: Decimal = 1.0
let x_end: Decimal = 5.0

# 龙格-库塔方法
let solution = runge_kutta_4(ode_func, x0, y0, x_end, 1000)

# 高阶微分方程组
class LorenzSystem {
    sigma: Decimal,
    rho: Decimal,
    beta: Decimal,
}

impl LorenzSystem {
    micro equations(self, t: Decimal, state: [Decimal; 3]) -> [Decimal; 3] {
        let [x, y, z] = state
        [
            self.sigma * (y - x),
            x * (self.rho - z) - y,
            x * y - self.beta * z
        ]
    }
}

let lorenz = LorenzSystem {
    sigma: 10.0,
    rho: 28.0,
    beta: 8.0 / 3.0
}

let initial_state: [Decimal; 3] = [1.0, 1.0, 1.0]
let lorenz_solution = solve_ode_system(lorenz.equations, 0.0: Decimal, initial_state, 20.0: Decimal, 10000)
```

## 线性代数

```valkyrie
use linalg::high_precision::*

# 高精度矩阵
let matrix = Matrix::new([
    [1.0: Decimal, 2.0: Decimal],
    [3.0: Decimal, 4.0: Decimal]
])

# 矩阵运算
let det = matrix.determinant()
let inv = matrix.inverse()
let eigenvalues = matrix.eigenvalues()

# 线性方程组求解
let A = Matrix::new([
    [2.0: Decimal, 1.0: Decimal],
    [1.0: Decimal, 3.0: Decimal]
])
let b = Vector::new([5.0: Decimal, 6.0: Decimal])
let x = A.solve(b)  # 求解 Ax = b
```

## 统计计算

```valkyrie
use statistics::high_precision::*

# 高精度统计函数
let data: [Decimal] = [1.0, 2.0, 3.0, 4.0, 5.0]

let mean = calculate_mean(data)
let variance = calculate_variance(data)
let std_dev = calculate_std_deviation(data)

# 概率分布
let normal_dist = NormalDistribution::new(
    0.0: Decimal,  # 均值
    1.0: Decimal   # 标准差
)

let test_value: Decimal = 1.5
let pdf_value = normal_dist.pdf(test_value)
let cdf_value = normal_dist.cdf(test_value)
```

## 性能优化

### 精度控制

```valkyrie
# 动态精度调整
class AdaptivePrecision {
    min_precision: u32,
    max_precision: u32,
    tolerance: Decimal,
}

impl AdaptivePrecision {
    micro compute_with_adaptive_precision<F>(self, f: F, x: Decimal) -> Decimal 
    where F: Fn(Decimal) -> Decimal {
        let mut precision = self.min_precision
        let mut prev_result: Decimal = 0.0
        
        loop {
            let x_prec = x.with_precision(precision)
            let result = f(x_prec)
            
            if precision > self.min_precision {
                let diff = (result - prev_result).abs()
                if diff < self.tolerance {
                    return result
                }
            }
            
            if precision >= self.max_precision {
                return result
            }
            
            prev_result = result
            precision *= 2
        }
    }
}
```

### 并行计算

```valkyrie
use parallel::*

# 并行数值积分
micro parallel_integration(f: impl Fn(Decimal) -> Decimal + Sync, 
                       a: Decimal, b: Decimal, n_threads: usize) -> Decimal {
    let chunk_size = (b - a) / n_threads as Decimal
    
    let results = (0..n_threads).into_par_iter().map(|i| {
        let start = a + (i as Decimal) * chunk_size
        let end = start + chunk_size
        gauss_legendre(f, start, end, 32)
    }).collect::<Vector<_>>()
    
    results.into_iter().sum()
}
```

## 最佳实践

### 1. 精度选择

```valkyrie
# 根据问题需求选择合适的精度
let financial_precision = 128  # 金融计算
let scientific_precision = 256  # 科学计算
let research_precision = 1024   # 研究级计算
```

### 2. 误差控制

```valkyrie
# 相对误差和绝对误差控制
class ErrorControl {
    absolute_tolerance: Decimal,
    relative_tolerance: Decimal,
}

impl ErrorControl {
    micro check_convergence(self, current: Decimal, previous: Decimal) -> bool {
        let abs_error = (current - previous).abs()
        let rel_error = abs_error / current.abs()
        
        abs_error < self.absolute_tolerance || rel_error < self.relative_tolerance
    }
}
```

### 3. 内存管理

```valkyrie
# 避免不必要的高精度计算
micro optimize_computation(x: f64) -> Decimal {
    # 先用普通精度估算
    let estimate = x.sin()
    
    # 只在需要时使用高精度
    if estimate.abs() > 0.1 {
        (x as Decimal).sin()
    } else {
        # 在接近零的地方使用高精度
        let high_prec_x = (x as Decimal).with_precision(256)
        high_prec_x.sin()
    }
}
```

高精度浮点数为 Valkyrie 提供了处理要求极高精度的科学计算问题的能力，适用于金融计算、天体力学、量子计算等领域。