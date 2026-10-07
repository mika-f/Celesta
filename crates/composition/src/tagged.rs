//! Deserializes the enums tagged by a `"type"` field without buffering them.
//!
//! serde's derived `#[serde(tag = "type")]` deserializer reads each object
//! into an intermediate tree before it picks the variant, and a group
//! layer's children are copied through that tree again at every level. A
//! React frame's scene is hundreds of KiB of such objects, and both
//! `@celesta/react` and these types' `Serialize` write `type` first.
//!
//! Each enum derives an externally tagged mirror with
//! `#[serde(remote = "...")]` and deserializes it from a [`TypeTagged`]
//! deserializer, which reads the variant from the `type` field and hands the
//! rest of the object straight to the variant's fields. Fields before a later
//! `type` are buffered as JSON values, so hand-written objects still load.

use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, IntoDeserializer, MapAccess, VariantAccess,
    Visitor,
};
use serde_json::Value;
use std::fmt;

/// Presents an object tagged by `type` as the externally tagged enum a
/// `remote` mirror's `deserialize` asks for.
pub(crate) struct TypeTagged<D>(pub(crate) D);

impl<'de, D: Deserializer<'de>> Deserializer<'de> for TypeTagged<D> {
    type Error = D::Error;

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.deserialize_map(TagVisitor { name, visitor })
    }

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.0.deserialize_any(visitor)
    }

    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct identifier ignored_any
    }
}

struct TagVisitor<V> {
    name: &'static str,
    visitor: V,
}

impl<'de, V: Visitor<'de>> Visitor<'de> for TagVisitor<V> {
    type Value = V::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "a {} object with a \"type\" field", self.name)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut before = Vec::new();
        while let Some(key) = map.next_key_seed(KeySeed)? {
            match key {
                Key::Type => return self.visitor.visit_enum(Tagged { before, map }),
                Key::Other(key) => before.push((key, map.next_value::<Value>()?)),
            }
        }
        Err(de::Error::missing_field("type"))
    }
}

enum Key {
    Type,
    Other(String),
}

/// Reads a key, allocating only for one other than `type`.
struct KeySeed;

impl<'de> DeserializeSeed<'de> for KeySeed {
    type Value = Key;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Key, D::Error> {
        deserializer.deserialize_identifier(self)
    }
}

impl Visitor<'_> for KeySeed {
    type Value = Key;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a field name")
    }

    fn visit_str<E: de::Error>(self, key: &str) -> Result<Key, E> {
        Ok(if key == "type" {
            Key::Type
        } else {
            Key::Other(key.to_owned())
        })
    }

    fn visit_string<E: de::Error>(self, key: String) -> Result<Key, E> {
        Ok(if key == "type" {
            Key::Type
        } else {
            Key::Other(key)
        })
    }
}

/// An object whose next value is its `type`.
struct Tagged<A> {
    before: Vec<(String, Value)>,
    map: A,
}

impl<'de, A: MapAccess<'de>> EnumAccess<'de> for Tagged<A> {
    type Error = A::Error;
    type Variant = Fields<A>;

    fn variant_seed<S: DeserializeSeed<'de>>(
        mut self,
        seed: S,
    ) -> Result<(S::Value, Fields<A>), A::Error> {
        let variant = self.map.next_value_seed(seed)?;
        let fields = Fields {
            before: self.before.into_iter(),
            pending: None,
            map: self.map,
        };
        Ok((variant, fields))
    }
}

/// The fields of an object other than its `type`: any buffered before it,
/// then the rest of the object.
struct Fields<A> {
    before: std::vec::IntoIter<(String, Value)>,
    pending: Option<Value>,
    map: A,
}

/// Hands a field name to `seed`, rejecting a second `type` as serde's
/// derived tagged enums do.
struct NotType<K>(K);

impl<'de, K: DeserializeSeed<'de>> DeserializeSeed<'de> for NotType<K> {
    type Value = K::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<K::Value, D::Error> {
        deserializer.deserialize_identifier(self)
    }
}

impl<'de, K: DeserializeSeed<'de>> Visitor<'de> for NotType<K> {
    type Value = K::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a field name")
    }

    fn visit_str<E: de::Error>(self, key: &str) -> Result<K::Value, E> {
        if key == "type" {
            return Err(E::duplicate_field("type"));
        }
        self.0
            .deserialize(IntoDeserializer::<E>::into_deserializer(key))
    }
}

impl<'de, A: MapAccess<'de>> VariantAccess<'de> for Fields<A> {
    type Error = A::Error;

    fn unit_variant(mut self) -> Result<(), A::Error> {
        // Like serde's tagged unit variants, ignore any other fields.
        while self
            .next_entry::<de::IgnoredAny, de::IgnoredAny>()?
            .is_some()
        {}
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, A::Error> {
        seed.deserialize(de::value::MapAccessDeserializer::new(self))
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, A::Error> {
        visitor.visit_map(self)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, A::Error> {
        visitor.visit_map(self)
    }
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for Fields<A> {
    type Error = A::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, A::Error> {
        let Some((key, value)) = self.before.next() else {
            return self.map.next_key_seed(NotType(seed));
        };
        self.pending = Some(value);
        seed.deserialize(IntoDeserializer::<A::Error>::into_deserializer(key))
            .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, A::Error> {
        match self.pending.take() {
            Some(value) => seed.deserialize(value).map_err(de::Error::custom),
            None => self.map.next_value_seed(seed),
        }
    }
}
