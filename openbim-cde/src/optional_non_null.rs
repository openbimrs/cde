//! Presence-aware support for optional, non-null JSON properties.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// An optional JSON object property that rejects explicit `null`.
///
/// `Option<T>` cannot represent this contract because Serde maps both an
/// omitted property and an explicit `null` to `None`. Use this type on a field
/// together with `#[serde(default, skip_serializing_if =
/// "OptionalNonNull::is_missing")]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum OptionalNonNull<T> {
    /// The property was absent and will be omitted when embedded in a model.
    #[default]
    Missing,
    /// The property was present with a non-null value.
    Present(T),
}

impl<T> OptionalNonNull<T> {
    /// Return whether the property is absent.
    #[must_use]
    pub const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }

    /// Borrow the present value, if any.
    #[must_use]
    pub const fn as_ref(&self) -> Option<&T> {
        match self {
            Self::Missing => None,
            Self::Present(value) => Some(value),
        }
    }

    /// Convert into an ordinary option after wire parsing.
    #[must_use]
    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Missing => None,
            Self::Present(value) => Some(value),
        }
    }
}

impl<T> From<T> for OptionalNonNull<T> {
    fn from(value: T) -> Self {
        Self::Present(value)
    }
}

impl<T: Serialize> Serialize for OptionalNonNull<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Missing => Err(serde::ser::Error::custom(
                "a missing optional property must be skipped by its containing model",
            )),
            Self::Present(value) => {
                let wire_value = serde_json::to_value(value).map_err(serde::ser::Error::custom)?;
                if wire_value.is_null() {
                    return Err(serde::ser::Error::custom(
                        "an optional non-null property cannot contain null",
                    ));
                }
                wire_value.serialize(serializer)
            }
        }
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for OptionalNonNull<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire_value = serde_json::Value::deserialize(deserializer)?;
        if wire_value.is_null() {
            return Err(serde::de::Error::custom(
                "an optional non-null property cannot contain null",
            ));
        }
        T::deserialize(wire_value)
            .map(Self::Present)
            .map_err(serde::de::Error::custom)
    }
}

impl<T: fmt::Display> fmt::Display for OptionalNonNull<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => formatter.write_str("<missing>"),
            Self::Present(value) => value.fmt(formatter),
        }
    }
}
