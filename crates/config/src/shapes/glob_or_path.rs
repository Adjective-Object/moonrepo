use super::portable_path::{FilePath, GlobPath, is_glob_like};
use super::*;
use moon_common::path::standardize_separators;
use schematic::{ParseError, Schema, SchemaBuilder, Schematic, schema::UnionType};
use serde::{Deserialize, Serialize, Serializer};
use std::str::FromStr;

/// A glob pattern or file path.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(untagged, expecting = "Expected a glob pattern or file path")]
pub enum GlobOrPath {
    File(FilePath),
    Glob(GlobPath),
}

impl GlobOrPath {
    pub fn create_uri(value: &str) -> Result<Uri, ParseError> {
        // Always use forward slashes
        let mut value = standardize_separators(value);

        // Convert literal paths to a URI
        if !value.contains("://") {
            if is_glob_like(&value) {
                value = format!("glob://{}", value.replace("?", "__QM__"));
            } else {
                value = format!("file://{value}");
            }
        }

        Uri::parse(&value)
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, ParseError> {
        Self::from_str(value.as_ref())
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::File(value) => value.as_str(),
            Self::Glob(value) => value.as_str(),
        }
    }

    pub fn is_glob(&self) -> bool {
        matches!(self, Self::Glob(_))
    }
}

impl FromStr for GlobOrPath {
    type Err = ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        // URI formats
        let uri = Self::create_uri(value)?;

        match uri.scheme.as_str() {
            "file" => Ok(Self::File(FileInput::from_uri(uri)?.file)),
            "glob" => Ok(Self::Glob(GlobInput::from_uri(uri)?.glob)),
            other => Err(ParseError::new(format!(
                "glob or path protocol `{other}://` is not supported, expected `file://` or `glob://`"
            ))),
        }
    }
}

impl Schematic for GlobOrPath {
    fn schema_name() -> Option<String> {
        Some("GlobOrPath".into())
    }

    fn build_schema(mut schema: SchemaBuilder) -> Schema {
        schema.union(UnionType::new_any([
            schema.infer::<String>(),
            schema.infer::<FileInput>(),
            schema.infer::<GlobInput>(),
        ]))
    }
}

impl Serialize for GlobOrPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            GlobOrPath::File(input) => input.serialize(serializer),
            GlobOrPath::Glob(input) => input.serialize(serializer),
        }
    }
}
