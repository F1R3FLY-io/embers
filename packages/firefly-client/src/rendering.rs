use std::any::type_name_of_val;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RhoValue {
    Tuple(Vec<RhoValue>),
    List(Vec<RhoValue>),
    Map(BTreeMap<String, RhoValue>),

    Nil,
    Bool(bool),
    Number(i64),
    String(String),
    Uri(String),
}

fn display_iterable<T, F>(values: T, f: &mut fmt::Formatter<'_>, mut format: F) -> fmt::Result
where
    T: IntoIterator,
    F: FnMut(&mut fmt::Formatter<'_>, T::Item) -> fmt::Result,
{
    values
        .into_iter()
        .enumerate()
        .try_fold((), |_, (i, entry)| {
            if i > 0 {
                f.write_str(", ")?;
            }
            format(f, entry)
        })
}

impl fmt::Display for RhoValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("Nil"),
            Self::Bool(b) => b.fmt(f),
            Self::Number(number) => number.fmt(f),
            Self::String(string) => write!(f, "\"{string}\""),
            Self::Uri(string) => write!(f, "`{string}`"),
            Self::Tuple(values) => {
                f.write_str("(")?;
                display_iterable(values, f, |f, entry| entry.fmt(f))?;
                f.write_str(")")
            }
            Self::List(values) => {
                f.write_str("[")?;
                display_iterable(values, f, |f, entry| entry.fmt(f))?;
                f.write_str("]")
            }
            Self::Map(map) => {
                f.write_str("{")?;
                display_iterable(map, f, |f, (k, v)| {
                    write!(f, "\"{k}\"")?;
                    f.write_str(": ")?;
                    v.fmt(f)
                })?;
                f.write_str("}")
            }
        }
    }
}

fn escape_rho_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("type {0} not supported")]
    NoSuported(&'static str),

    #[error("type {0} not supported as map key")]
    NoSuportedMapKey(&'static str),

    #[error("other error: {0}")]
    Custom(String),
}

impl serde::ser::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: fmt::Display,
    {
        Self::Custom(msg.to_string())
    }
}

pub struct Serializer;

impl serde::Serializer for Serializer {
    type Ok = RhoValue;
    type Error = Error;

    type SerializeSeq = SerializeSeq;
    type SerializeTuple = SerializeSeq;
    type SerializeTupleStruct = SerializeSeq;

    type SerializeTupleVariant = SerializeTupleVariant;

    type SerializeMap = SerializeMap;
    type SerializeStruct = SerializeMap;

    type SerializeStructVariant = SerializeStructVariant;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        Ok(Self::Ok::Bool(v))
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        Ok(Self::Ok::Number(v))
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::NoSuported(type_name_of_val(&v)))
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(v.into())
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::NoSuported(type_name_of_val(&v)))
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::NoSuported(type_name_of_val(&v)))
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::NoSuported(type_name_of_val(&v)))
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::NoSuported(type_name_of_val(&v)))
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        Ok(Self::Ok::String(escape_rho_string(v)))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::NoSuported(type_name_of_val(&v)))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(Self::Ok::Nil)
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(Self::Ok::Tuple(Default::default()))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(Self::Ok::String(variant.into()))
    }

    fn serialize_newtype_struct<T>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
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
        T: ?Sized + serde::Serialize,
    {
        let value = value.serialize(self)?;
        let mut values = BTreeMap::new();
        values.insert(variant.into(), value);
        Ok(Self::Ok::Map(values))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(SerializeSeq {
            items: Vec::with_capacity(len.unwrap_or(0)),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(SerializeTupleVariant {
            name: variant.into(),
            items: Vec::with_capacity(len),
        })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(SerializeMap {
            map: Default::default(),
            next_key: None,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(SerializeStructVariant {
            name: variant.into(),
            map: Default::default(),
        })
    }
}

pub struct SerializeSeq {
    items: Vec<RhoValue>,
}

impl serde::ser::SerializeSeq for SerializeSeq {
    type Ok = RhoValue;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let value = value.serialize(Serializer)?;
        self.items.push(value);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Self::Ok::List(self.items))
    }
}

impl serde::ser::SerializeTuple for SerializeSeq {
    type Ok = RhoValue;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Self::Ok::Tuple(self.items))
    }
}

impl serde::ser::SerializeTupleStruct for SerializeSeq {
    type Ok = RhoValue;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        serde::ser::SerializeTuple::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeTuple::end(self)
    }
}

pub struct SerializeTupleVariant {
    name: String,
    items: Vec<RhoValue>,
}

impl serde::ser::SerializeTupleVariant for SerializeTupleVariant {
    type Ok = RhoValue;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let value = value.serialize(Serializer)?;
        self.items.push(value);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let mut object = BTreeMap::new();
        object.insert(self.name, Self::Ok::Tuple(self.items));
        Ok(Self::Ok::Map(object))
    }
}

pub struct SerializeMap {
    map: BTreeMap<String, RhoValue>,
    next_key: Option<String>,
}

impl serde::ser::SerializeMap for SerializeMap {
    type Ok = RhoValue;
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let Self::Ok::String(key) = key.serialize(Serializer)? else {
            return Err(Self::Error::NoSuportedMapKey(type_name_of_val(key)));
        };
        self.next_key = Some(key);
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        // Panic because serde_json does the same
        let key = self
            .next_key
            .take()
            .expect("serialize_value called before serialize_key");
        let value = value.serialize(Serializer)?;
        self.map.insert(key, value);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Self::Ok::Map(self.map))
    }
}

impl serde::ser::SerializeStruct for SerializeMap {
    type Ok = RhoValue;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let value = value.serialize(Serializer)?;
        self.map.insert(key.into(), value);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        serde::ser::SerializeMap::end(self)
    }
}

pub struct SerializeStructVariant {
    name: String,
    map: BTreeMap<String, RhoValue>,
}

impl serde::ser::SerializeStructVariant for SerializeStructVariant {
    type Ok = RhoValue;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let value = value.serialize(Serializer)?;
        self.map.insert(key.into(), value);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let mut object = BTreeMap::new();
        object.insert(self.name, Self::Ok::Map(self.map));
        Ok(Self::Ok::Map(object))
    }
}

pub mod _dependencies {
    pub use {askama, serde};
}

#[macro_export]
macro_rules! template {
    (
        #[template(path = $path:literal)]
        $(#[$struct_attr:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_attr:meta])*
                $field_vis:vis $field:ident: $typ:ty
            ),* $(,)?
        }
    ) => {
        #[derive($crate::rendering::_dependencies::serde::Serialize)]
        $(#[$struct_attr])*
        $vis struct $name {
            $(
                $(#[$field_attr])*
                $field_vis $field: $typ
            ),*
        }

        impl $name {
            pub fn render(self) -> Result<String, $crate::rendering::Error> {
                #[derive($crate::rendering::_dependencies::askama::Template)]
                #[template(path = $path, escape = "none")]
                struct Template {
                    $($field: $crate::rendering::RhoValue),*
                }

                let template = Template {
                    $(
                        $field: $crate::rendering::_dependencies::serde::Serialize::serialize(
                            &self.$field,
                            $crate::rendering::Serializer
                        )?
                    ),*
                };

                $crate::rendering::_dependencies::askama::Template::render(&template)
                    .map_err($crate::rendering::_dependencies::serde::ser::Error::custom)
            }

            pub fn builder(self) -> Result<$crate::models::DeployDataBuilder, $crate::rendering::Error> {
                self.render().map($crate::models::DeployData::builder)
            }
        }
    };
}

pub use template;
