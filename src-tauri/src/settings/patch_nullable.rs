use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Patch field: absent → no change; JSON `null` → clear; value → set.
pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

pub fn serialize<S, T>(value: &Option<Option<T>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    match value {
        None => serializer.serialize_none(),
        Some(inner) => inner.serialize(serializer),
    }
}

pub fn is_absent<T>(value: &Option<Option<T>>) -> bool {
    value.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct LanguagePatch {
        #[serde(default, deserialize_with = "deserialize")]
        language: Option<Option<String>>,
    }

    #[test]
    fn absent_field_leaves_patch_unset() {
        let patch: LanguagePatch = serde_json::from_str("{}").unwrap();
        assert!(patch.language.is_none());
    }

    #[test]
    fn json_null_clears_value() {
        let patch: LanguagePatch = serde_json::from_str(r#"{"language":null}"#).unwrap();
        assert_eq!(patch.language, Some(None));
    }

    #[test]
    fn json_string_sets_value() {
        let patch: LanguagePatch = serde_json::from_str(r#"{"language":"ru"}"#).unwrap();
        assert_eq!(patch.language, Some(Some("ru".to_string())));
    }
}
