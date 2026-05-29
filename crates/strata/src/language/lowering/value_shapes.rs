use mantle_artifact::{ArtifactEnumVariant, ArtifactTypeField, ArtifactValueShape};

use crate::language::checked::CheckedValueShape;

use super::lower_type_id;

pub(super) fn lower_value_shape(shape: &CheckedValueShape) -> ArtifactValueShape {
    match shape {
        CheckedValueShape::Atom => ArtifactValueShape::Atom,
        CheckedValueShape::Scalar(scalar) => ArtifactValueShape::Scalar { scalar: *scalar },
        CheckedValueShape::Record { fields } => ArtifactValueShape::Record {
            fields: fields
                .iter()
                .map(|field| ArtifactTypeField {
                    name: field.name.to_string(),
                    ty: lower_type_id(field.ty),
                })
                .collect(),
        },
        CheckedValueShape::Enum { variants } => ArtifactValueShape::Enum {
            variants: variants
                .iter()
                .map(|variant| ArtifactEnumVariant {
                    label: variant.name.to_string(),
                    payload_type: variant.payload_type.map(lower_type_id),
                })
                .collect(),
        },
        CheckedValueShape::List { element, capacity } => ArtifactValueShape::List {
            element: lower_type_id(*element),
            capacity: *capacity,
        },
        CheckedValueShape::Map {
            key,
            value,
            capacity,
        } => ArtifactValueShape::Map {
            key: lower_type_id(*key),
            value: lower_type_id(*value),
            capacity: *capacity,
        },
    }
}
