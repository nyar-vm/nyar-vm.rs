# Unit System

Valkyrie provides a powerful compile-time unit system that ensures the correctness of physical quantity calculations through macros and the type system, preventing errors from mismatched units.

## Basic Unit Definitions

### SI Base Units

```valkyrie
# Base unit macros
let mass = 1kg        # Kilogram
let length = 1m       # Meter
let time = 1s         # Second
let current = 1A      # Ampere
let temperature = 1K  # Kelvin
let amount = 1mol     # Mole
let luminosity = 1cd  # Candela

# Using base units
let distance: Length = 100m
let duration: Time = 5s
let weight: Mass = 2.5kg
let temp: Temperature = 273.15K
```

### Derived Units

```valkyrie
# Area units
let area1 = 1m²       # Square meter
let area2 = 1m * 1m   # Equivalent form

# Volume units
let volume1 = 1m³     # Cubic meter
let volume2 = 1m * 1m * 1m  # Equivalent form

# Velocity units
let velocity1 = 1m/s  # Meters per second
let velocity2 = 1m / 1s  # Equivalent form

# Acceleration units
let acceleration = 1m/s²  # Meters per second squared

# Force units
let force1 = 1N       # Newton
let force2 = 1kg * 1m/s²  # Equivalent definition

# Energy units
let energy1 = 1J      # Joule
let energy2 = 1N * 1m # Equivalent definition
let energy3 = 1kg * 1m²/s²  # Base unit representation

# Power units
let power1 = 1W       # Watt
let power2 = 1J/s     # Equivalent definition
```

## Unit Type System

### Dimensional Types

```valkyrie
# Dimensional type definitions
type Length = Quantity⟨[1, 0, 0, 0, 0, 0, 0]⟩     # [L, M, T, I, Θ, N, J]
type Mass = Quantity⟨[0, 1, 0, 0, 0, 0, 0]⟩
type Time = Quantity⟨[0, 0, 1, 0, 0, 0, 0]⟩
type Current = Quantity⟨[0, 0, 0, 1, 0, 0, 0]⟩
type Temperature = Quantity⟨[0, 0, 0, 0, 1, 0, 0]⟩
type Amount = Quantity⟨[0, 0, 0, 0, 0, 1, 0]⟩
type Luminosity = Quantity⟨[0, 0, 0, 0, 0, 0, 1]⟩

type Area = Quantity⟨[2, 0, 0, 0, 0, 0, 0]⟩
type Volume = Quantity⟨[3, 0, 0, 0, 0, 0, 0]⟩
type Velocity = Quantity⟨[1, 0, -1, 0, 0, 0, 0]⟩
type Acceleration = Quantity⟨[1, 0, -2, 0, 0, 0, 0]⟩
type Force = Quantity⟨[1, 1, -2, 0, 0, 0, 0]⟩
type Energy = Quantity⟨[2, 1, -2, 0, 0, 0, 0]⟩
type Power = Quantity⟨[2, 1, -3, 0, 0, 0, 0]⟩
type Voltage = Quantity⟨[2, 1, -3, -1, 0, 0, 0]⟩
type Resistance = Quantity⟨[2, 1, -3, -2, 0, 0, 0]⟩

# Using dimensional types
micro calculate_kinetic_energy(mass: Mass, velocity: Velocity) -> Energy {
    0.5 * mass * velocity * velocity
}

micro calculate_power(energy: Energy, time: Time) -> Power {
    energy / time
}
```

### Unit Conversion

```valkyrie
# Length unit conversion
let meter = 1m
let kilometer = 1km     # 1000m
let centimeter = 1cm    # 0.01m
let millimeter = 1mm    # 0.001m
let inch = 1inch        # 0.0254m
let foot = 1ft          # 0.3048m
let yard = 1yd          # 0.9144m
let mile = 1mile        # 1609.344m

# Automatic conversion
let distance1: Length = 5km
let distance2: Length = 3000m
let total_distance = distance1 + distance2  # 8000m

# Explicit conversion
let km_value = distance1.to(km)  # 5.0
let m_value = distance1.to(m)    # 5000.0
```

### Mass Units

```valkyrie
# Mass units
let kilogram = 1kg
let gram = 1g           # 0.001kg
let ton = 1t            # 1000kg
let pound = 1lb         # 0.453592kg
let ounce = 1oz         # 0.0283495kg

# Mass calculations
let total_mass = 2kg + 500g  # 2.5kg
let density = total_mass / (1m³)  # Density type
```

### Time Units

```valkyrie
# Time units
let second = 1s
let minute = 1min       # 60s
let hour = 1h           # 3600s
let day = 1day          # 86400s
let week = 1week        # 604800s
let year = 1year        # 31557600s (365.25 days)

# Time calculations
let duration = 2h + 30min + 15s  # 9015s
let frequency = 1 / duration      # Frequency type
```

## Compound Unit Calculations

### Physical Formulas

```valkyrie
# Newton's Second Law: F = ma
micro newtons_second_law(mass: Mass, acceleration: Acceleration) -> Force {
    mass * acceleration
}

# Kinetic Energy: E = 1/2 * m * v²
micro kinetic_energy(mass: Mass, velocity: Velocity) -> Energy {
    0.5 * mass * velocity.pow(2)
}

# Gravitational Potential Energy: E = mgh
micro gravitational_potential_energy(mass: Mass, height: Length, g: Acceleration) -> Energy {
    mass * g * height
}

# Power Formula: P = W/t
micro power_from_work(work: Energy, time: Time) -> Power {
    work / time
}

# Ohm's Law: V = IR
micro ohms_law(current: Current, resistance: Resistance) -> Voltage {
    current * resistance
}
```

### Electrical Units

```valkyrie
# Electrical base units
let current = 1A        # Ampere
let voltage = 1V        # Volt
let resistance = 1Ω     # Ohm
let capacitance = 1F    # Farad
let inductance = 1H     # Henry
let charge = 1C         # Coulomb

# Electrical calculations
let power_electrical = voltage * current  # Electric power
let energy_stored = 0.5 * capacitance * voltage.pow(2)  # Capacitor energy storage
let magnetic_energy = 0.5 * inductance * current.pow(2)  # Inductor energy storage
```

### Thermodynamic Units

```valkyrie
# Temperature units
let kelvin = 1K
let celsius = 1°C       # Relative temperature
let fahrenheit = 1°F    # Relative temperature

# Heat units
let joule = 1J
let calorie = 1cal      # 4.184J
let btu = 1BTU          # 1055.06J

# Thermodynamic calculations
```
