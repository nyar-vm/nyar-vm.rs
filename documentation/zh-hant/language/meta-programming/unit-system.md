# 單位制系統

Valkyrie 提供了強大的編譯時單位制系統，通過宏和類型系統確保物理量計算的正確性，防止單位不匹配的錯誤。

## 基本單位定義

### SI 基本單位

```valkyrie
# 基本單位宏
let mass = 1kg        # 千克
let length = 1m       # 米
let time = 1s         # 秒
let current = 1A      # 安培
let temperature = 1K  # 開爾文
let amount = 1mol     # 摩爾
let luminosity = 1cd  # 坎德拉

# 使用基本單位
let distance: Length = 100m
let duration: Time = 5s
let weight: Mass = 2.5kg
let temp: Temperature = 273.15K
```

### 導出單位

```valkyrie
# 面積單位
let area1 = 1m²       # 平方米
let area2 = 1m * 1m   # 等價寫法

# 體積單位
let volume1 = 1m³     # 立方米
let volume2 = 1m * 1m * 1m  # 等價寫法

# 速度單位
let velocity1 = 1m/s  # 米每秒
let velocity2 = 1m / 1s  # 等價寫法

# 加速度單位
let acceleration = 1m/s²  # 米每秒平方

# 力單位
let force1 = 1N       # 牛頓
let force2 = 1kg * 1m/s²  # 等價定義

# 能量單位
let energy1 = 1J      # 焦耳
let energy2 = 1N * 1m # 等價定義
let energy3 = 1kg * 1m²/s²  # 基本單位表示

# 功率單位
let power1 = 1W       # 瓦特
let power2 = 1J/s     # 等價定義
```

## 單位類型系統

### 量綱類型

```valkyrie
# 量綱類型定義
type Length = Quantity⟨[1, 0, 0, 0, 0, 0, 0]⟩     # [L, M, T, I, Θ, N, J]
type Mass = Quantity⟨[0, 1, 0, 0, 0, 0, 0]⟩
type Time = Quantity⟨[0, 0, 1, 0, 0, 0, 0]⟩
type Area = Quantity⟨[2, 0, 0, 0, 0, 0, 0]⟩
type Volume = Quantity⟨[3, 0, 0, 0, 0, 0, 0]⟩
type Velocity = Quantity⟨[1, 0, -1, 0, 0, 0, 0]⟩
type Acceleration = Quantity⟨[1, 0, -2, 0, 0, 0, 0]⟩
type Force = Quantity⟨[1, 1, -2, 0, 0, 0, 0]⟩
type Energy = Quantity⟨[2, 1, -2, 0, 0, 0, 0]⟩
type Power = Quantity⟨[2, 1, -3, 0, 0, 0, 0]⟩

# 使用量綱類型
micro calculate_kinetic_energy(mass: Mass, velocity: Velocity) -> Energy {
    0.5 * mass * velocity * velocity
}

micro calculate_power(energy: Energy, time: Time) -> Power {
    energy / time
}
```

### 單位轉換

```valkyrie
# 長度單位轉換
let meter = 1m
let kilometer = 1km     # 1000m
let centimeter = 1cm    # 0.01m
let millimeter = 1mm    # 0.001m
let inch = 1inch        # 0.0254m
let foot = 1ft          # 0.3048m
let yard = 1yd          # 0.9144m
let mile = 1mile        # 1609.344m

# 自動轉換
let distance1: Length = 5km
let distance2: Length = 3000m
let total_distance = distance1 + distance2  # 8000m

# 顯式轉換
let km_value = distance1.to(km)  # 5.0
let m_value = distance1.to(m)    # 5000.0
```

### 質量單位

```valkyrie
# 質量單位
let kilogram = 1kg
let gram = 1g           # 0.001kg
let ton = 1t            # 1000kg
let pound = 1lb         # 0.453592kg
let ounce = 1oz         # 0.0283495kg

# 質量計算
let total_mass = 2kg + 500g  # 2.5kg
let density = total_mass / (1m³)  # 密度類型
```

### 時間單位

```valkyrie
# 時間單位
let second = 1s
let minute = 1min       # 60s
let hour = 1h           # 3600s
let day = 1day          # 86400s
let week = 1week        # 604800s
let year = 1year        # 31557600s (365.25 days)

# 時間計算
let duration = 2h + 30min + 15s  # 9015s
let frequency = 1 / duration      # 頻率類型
```

## 複合單位計算

### 物理公式

```valkyrie
# 牛頓第二定律: F = ma
micro newtons_second_law(mass: Mass, acceleration: Acceleration) -> Force {
    mass * acceleration
}

# 動能公式: E = 1/2 * m * v²
micro kinetic_energy(mass: Mass, velocity: Velocity) -> Energy {
    0.5 * mass * velocity.pow(2)
}

# 重力勢能: E = mgh
micro gravitational_potential_energy(mass: Mass, height: Length, g: Acceleration) -> Energy {
    mass * g * height
}

# 功率公式: P = W/t
micro power_from_work(work: Energy, time: Time) -> Power {
    work / time
}

# 歐姆定律: V = IR
micro ohms_law(current: Current, resistance: Resistance) -> Voltage {
    current * resistance
}
```

### 電學單位

```valkyrie
# 電學基本單位
let current = 1A        # 安培
let voltage = 1V        # 伏特
let resistance = 1Ω     # 歐姆
let capacitance = 1F    # 法拉
let inductance = 1H     # 亨利
let charge = 1C         # 庫侖

# 電學計算
let power_electrical = voltage * current  # 電功率
let energy_stored = 0.5 * capacitance * voltage.pow(2)  # 電容儲能
let magnetic_energy = 0.5 * inductance * current.pow(2)  # 電感儲能
```

### 熱力學單位

```valkyrie
# 溫度單位
let kelvin = 1K
let celsius = 1°C       # 相對溫度
let fahrenheit = 1°F    # 相對溫度

# 熱量單位
let joule = 1J
let calorie = 1cal      # 4.184J
let btu = 1BTU          # 1055.06J

# 熱力學計算
```
