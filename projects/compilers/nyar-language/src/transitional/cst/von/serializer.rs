use std::collections::BTreeMap;

use serde::{
    Serialize,
    ser::{
        self, Impossible, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple, SerializeTupleStruct,
        SerializeTupleVariant,
    },
};

use super::{
    error::{VonError, VonSerdeError},
    value::VonValue,
};

pub struct VonSerializer;

impl ser::Serializer for VonSerializer {
    type Ok = VonValue;
    type Error = VonSerdeError;
    type SerializeSeq = VonSerializeSeq;
    type SerializeTuple = VonSerializeSeq;
    type SerializeTupleStruct = VonSerializeSeq;
    type SerializeTupleVariant = VonSerializeTupleVariant;
    type SerializeMap = VonSerializeMap;
    type SerializeStruct = VonSerializeMap;
    type SerializeStructVariant = VonSerializeStructVariant;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Bool(value))
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Number(i64::from(value)))
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Number(i64::from(value)))
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Number(i64::from(value)))
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Number(value))
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Number(i64::from(value)))
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Number(i64::from(value)))
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Number(i64::from(value)))
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        let number = i64::try_from(value).map_err(|_| VonSerdeError::new("VON 不支持超过 i64 范围的整数"))?;
        Ok(VonValue::Number(number))
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok, Self::Error> {
        Err(VonSerdeError::new("VON 不支持浮点数"))
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok, Self::Error> {
        Err(VonSerdeError::new("VON 不支持浮点数"))
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::String(value.to_string()))
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::String(value.to_owned()))
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Array(value.iter().map(|item| VonValue::Number(i64::from(*item))).collect()))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Null)
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Null)
    }

    fn serialize_unit_variant(self, _name: &'static str, _variant_index: u32, variant: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::String(variant.to_owned()))
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        let mut members = BTreeMap::new();
        members.insert(variant.to_owned(), value.serialize(self)?);
        Ok(VonValue::Object(members))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(VonSerializeSeq { values: Vec::with_capacity(len.unwrap_or(0)) })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(VonSerializeTupleVariant { variant: variant.to_owned(), values: Vec::with_capacity(len) })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(VonSerializeMap { entries: BTreeMap::new(), next_key: None })
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct, Self::Error> {
        self.serialize_map(None)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(VonSerializeStructVariant { variant: variant.to_owned(), entries: BTreeMap::new() })
    }

    fn is_human_readable(&self) -> bool {
        true
    }
}

pub struct VonSerializeSeq {
    values: Vec<VonValue>,
}

impl SerializeSeq for VonSerializeSeq {
    type Ok = VonValue;
    type Error = VonSerdeError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.values.push(value.serialize(VonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Array(self.values))
    }
}

impl SerializeTuple for VonSerializeSeq {
    type Ok = VonValue;
    type Error = VonSerdeError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

impl SerializeTupleStruct for VonSerializeSeq {
    type Ok = VonValue;
    type Error = VonSerdeError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

pub struct VonSerializeTupleVariant {
    variant: String,
    values: Vec<VonValue>,
}

impl SerializeTupleVariant for VonSerializeTupleVariant {
    type Ok = VonValue;
    type Error = VonSerdeError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.values.push(value.serialize(VonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let mut members = BTreeMap::new();
        members.insert(self.variant, VonValue::Array(self.values));
        Ok(VonValue::Object(members))
    }
}

pub struct VonSerializeMap {
    entries: BTreeMap<String, VonValue>,
    next_key: Option<String>,
}

impl SerializeMap for VonSerializeMap {
    type Ok = VonValue;
    type Error = VonSerdeError;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.next_key = Some(key.serialize(VonKeySerializer)?);
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        let key = self.next_key.take().ok_or_else(|| VonSerdeError::new("serialize_value 在 serialize_key 之前调用"))?;
        self.entries.insert(key, value.serialize(VonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(VonValue::Object(self.entries))
    }
}

impl SerializeStruct for VonSerializeMap {
    type Ok = VonValue;
    type Error = VonSerdeError;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.entries.insert(key.to_owned(), value.serialize(VonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeMap::end(self)
    }
}

pub struct VonSerializeStructVariant {
    variant: String,
    entries: BTreeMap<String, VonValue>,
}

impl SerializeStructVariant for VonSerializeStructVariant {
    type Ok = VonValue;
    type Error = VonSerdeError;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.entries.insert(key.to_owned(), value.serialize(VonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let mut members = BTreeMap::new();
        members.insert(self.variant, VonValue::Object(self.entries));
        Ok(VonValue::Object(members))
    }
}

struct VonKeySerializer;

impl ser::Serializer for VonKeySerializer {
    type Ok = String;
    type Error = VonSerdeError;
    type SerializeSeq = Impossible<String, VonSerdeError>;
    type SerializeTuple = Impossible<String, VonSerdeError>;
    type SerializeTupleStruct = Impossible<String, VonSerdeError>;
    type SerializeTupleVariant = Impossible<String, VonSerdeError>;
    type SerializeMap = Impossible<String, VonSerdeError>;
    type SerializeStruct = Impossible<String, VonSerdeError>;
    type SerializeStructVariant = Impossible<String, VonSerdeError>;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持浮点数"))
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持浮点数"))
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_owned())
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Self::Ok, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持字节数组"))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持 null"))
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持 unit"))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(self, _name: &'static str, _variant_index: u32, variant: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(variant.to_owned())
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(VonSerdeError::new("VON 对象键不支持复杂枚举值"))
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持数组"))
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持元组"))
    }

    fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持元组结构"))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持复杂枚举值"))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持对象"))
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持结构体"))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(VonSerdeError::new("VON 对象键不支持复杂枚举值"))
    }

    fn is_human_readable(&self) -> bool {
        true
    }
}

pub fn to_value<T>(value: &T) -> Result<VonValue, VonError>
where
    T: Serialize,
{
    value.serialize(VonSerializer).map_err(VonError::Serialize)
}
