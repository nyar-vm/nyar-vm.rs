use serde::de::{
    self, DeserializeOwned, IntoDeserializer, Visitor,
    value::{MapDeserializer, SeqDeserializer},
};

use super::{
    error::{VonError, VonSerdeError},
    parser::VonParser,
    value::VonValue,
};

pub struct VonDeserializer {
    input: VonValue,
}

impl VonDeserializer {
    pub fn new(input: VonValue) -> Self {
        Self { input }
    }
}

impl<'de> IntoDeserializer<'de, VonSerdeError> for VonValue {
    type Deserializer = VonDeserializer;

    fn into_deserializer(self) -> Self::Deserializer {
        VonDeserializer::new(self)
    }
}

impl<'de> de::Deserializer<'de> for VonDeserializer {
    type Error = VonSerdeError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::Null => visitor.visit_unit(),
            VonValue::Bool(value) => visitor.visit_bool(value),
            VonValue::Number(value) => visitor.visit_i64(value),
            VonValue::String(value) => visitor.visit_string(value),
            VonValue::Array(values) => {
                let deserializer = SeqDeserializer::new(values.into_iter());
                visitor.visit_seq(deserializer)
            }
            VonValue::Object(entries) => {
                let deserializer = MapDeserializer::new(entries.into_iter());
                visitor.visit_map(deserializer)
            }
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::Bool(value) => visitor.visit_bool(value),
            other => Err(invalid_value("bool", &other)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = deserialize_signed(self.input, "i8")?;
        let converted = i8::try_from(value).map_err(|_| VonSerdeError::new("整数超出 i8 范围"))?;
        visitor.visit_i8(converted)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = deserialize_signed(self.input, "i16")?;
        let converted = i16::try_from(value).map_err(|_| VonSerdeError::new("整数超出 i16 范围"))?;
        visitor.visit_i16(converted)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = deserialize_signed(self.input, "i32")?;
        let converted = i32::try_from(value).map_err(|_| VonSerdeError::new("整数超出 i32 范围"))?;
        visitor.visit_i32(converted)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i64(deserialize_signed(self.input, "i64")?)
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i128(i128::from(deserialize_signed(self.input, "i128")?))
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = deserialize_unsigned(self.input, "u8")?;
        let converted = u8::try_from(value).map_err(|_| VonSerdeError::new("整数超出 u8 范围"))?;
        visitor.visit_u8(converted)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = deserialize_unsigned(self.input, "u16")?;
        let converted = u16::try_from(value).map_err(|_| VonSerdeError::new("整数超出 u16 范围"))?;
        visitor.visit_u16(converted)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = deserialize_unsigned(self.input, "u32")?;
        let converted = u32::try_from(value).map_err(|_| VonSerdeError::new("整数超出 u32 范围"))?;
        visitor.visit_u32(converted)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u64(deserialize_unsigned(self.input, "u64")?)
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u128(u128::from(deserialize_unsigned(self.input, "u128")?))
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::Number(value) => visitor.visit_f32(value as f32),
            other => Err(invalid_value("f32", &other)),
        }
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::Number(value) => visitor.visit_f64(value as f64),
            other => Err(invalid_value("f64", &other)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::String(value) => {
                let mut chars = value.chars();
                let Some(first) = chars.next()
                else {
                    return Err(VonSerdeError::new("空字符串不能反序列化为 char"));
                };
                if chars.next().is_some() {
                    return Err(VonSerdeError::new("长度大于 1 的字符串不能反序列化为 char"));
                }
                visitor.visit_char(first)
            }
            other => Err(invalid_value("char", &other)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::String(value) => visitor.visit_string(value),
            other => Err(invalid_value("string", &other)),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::Array(values) => {
                let bytes = values
                    .into_iter()
                    .map(|value| match value {
                        VonValue::Number(number) => u8::try_from(number).map_err(|_| VonSerdeError::new("字节数组元素必须位于 0..=255")),
                        other => Err(invalid_value("u8", &other)),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                visitor.visit_byte_buf(bytes)
            }
            other => Err(invalid_value("bytes", &other)),
        }
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::Null => visitor.visit_none(),
            other => visitor.visit_some(VonDeserializer::new(other)),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::Null => visitor.visit_unit(),
            other => Err(invalid_value("unit", &other)),
        }
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::Array(values) => {
                let deserializer = SeqDeserializer::new(values.into_iter());
                visitor.visit_seq(deserializer)
            }
            other => Err(invalid_value("array", &other)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(self, _name: &'static str, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::Object(entries) => {
                let deserializer = MapDeserializer::new(entries.into_iter());
                visitor.visit_map(deserializer)
            }
            other => Err(invalid_value("object", &other)),
        }
    }

    fn deserialize_struct<V>(self, _name: &'static str, _fields: &'static [&'static str], visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(self, _name: &'static str, _variants: &'static [&'static str], visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.input {
            VonValue::String(variant) => visitor.visit_enum(VonEnumDeserializer { variant, value: None }),
            VonValue::Object(entries) => {
                let mut iter = entries.into_iter();
                let Some((variant, value)) = iter.next()
                else {
                    return Err(VonSerdeError::new("枚举对象不能为空"));
                };
                if iter.next().is_some() {
                    return Err(VonSerdeError::new("枚举对象只能包含一个变体"));
                }
                visitor.visit_enum(VonEnumDeserializer { variant, value: Some(value) })
            }
            other => Err(invalid_value("enum", &other)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn is_human_readable(&self) -> bool {
        true
    }
}

struct VonEnumDeserializer {
    variant: String,
    value: Option<VonValue>,
}

impl<'de> de::EnumAccess<'de> for VonEnumDeserializer {
    type Error = VonSerdeError;
    type Variant = VonVariantDeserializer;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.variant.into_deserializer())?;
        Ok((variant, VonVariantDeserializer { value: self.value }))
    }
}

struct VonVariantDeserializer {
    value: Option<VonValue>,
}

impl<'de> de::VariantAccess<'de> for VonVariantDeserializer {
    type Error = VonSerdeError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None | Some(VonValue::Null) => Ok(()),
            Some(other) => Err(invalid_value("unit variant", &other)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self.value.ok_or_else(|| VonSerdeError::new("缺少枚举变体的值"))?;
        seed.deserialize(VonDeserializer::new(value))
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Some(VonValue::Array(values)) => {
                let deserializer = SeqDeserializer::new(values.into_iter());
                visitor.visit_seq(deserializer)
            }
            Some(other) => Err(invalid_value("tuple variant", &other)),
            None => Err(VonSerdeError::new("缺少枚举变体的值")),
        }
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Some(VonValue::Object(entries)) => {
                let deserializer = MapDeserializer::new(entries.into_iter());
                visitor.visit_map(deserializer)
            }
            Some(other) => Err(invalid_value("struct variant", &other)),
            None => Err(VonSerdeError::new("缺少枚举变体的值")),
        }
    }
}

pub fn from_value<T>(value: VonValue) -> Result<T, VonError>
where
    T: DeserializeOwned,
{
    T::deserialize(VonDeserializer::new(value)).map_err(VonError::Deserialize)
}

pub fn from_str<T>(source: &str) -> Result<T, VonError>
where
    T: DeserializeOwned,
{
    let value = VonParser::parse(source)?;
    from_value(value)
}

fn deserialize_signed(value: VonValue, expected: &str) -> Result<i64, VonSerdeError> {
    match value {
        VonValue::Number(number) => Ok(number),
        other => Err(invalid_value(expected, &other)),
    }
}

fn deserialize_unsigned(value: VonValue, expected: &str) -> Result<u64, VonSerdeError> {
    match value {
        VonValue::Number(number) => u64::try_from(number).map_err(|_| VonSerdeError::new(format!("{} 不能从负数反序列化", expected))),
        other => Err(invalid_value(expected, &other)),
    }
}

fn invalid_value(expected: &str, value: &VonValue) -> VonSerdeError {
    VonSerdeError::new(format!("期望 {}，实际得到 {}", expected, value_kind(value)))
}

fn value_kind(value: &VonValue) -> &'static str {
    match value {
        VonValue::Null => "null",
        VonValue::Bool(_) => "bool",
        VonValue::Number(_) => "number",
        VonValue::String(_) => "string",
        VonValue::Array(_) => "array",
        VonValue::Object(_) => "object",
    }
}
