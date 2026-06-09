use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArtifactPrimitiveType {
    String,
    Bytes,
}

impl ArtifactPrimitiveType {
    pub const ALL: [Self; 2] = [Self::String, Self::Bytes];

    pub const fn source_name(self) -> &'static str {
        match self {
            Self::String => "String",
            Self::Bytes => "Bytes",
        }
    }

    pub const fn artifact_name(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Bytes => "bytes",
        }
    }

    pub fn parse_source_name(value: &str) -> Option<Self> {
        match value {
            "String" => Some(Self::String),
            "Bytes" => Some(Self::Bytes),
            _ => None,
        }
    }

    pub(crate) fn parse_artifact_name(value: &str) -> Result<Self> {
        match value {
            "string" => Ok(Self::String),
            "bytes" => Ok(Self::Bytes),
            _ => Err(Error::new(format!("invalid primitive type {value:?}"))),
        }
    }
}
