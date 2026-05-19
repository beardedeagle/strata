use super::*;

pub(super) fn static_map_key_template_value(
    artifact: &MantleArtifact,
    template: &ArtifactValueTemplate,
) -> Result<ArtifactValue> {
    template
        .evaluate_state_value(None, None, &|ty| artifact.type_entry(ty).cloned())
        .map(|value| value.value)
}

pub(super) fn is_static_map_key_template(template: &ArtifactValueTemplate) -> bool {
    match template {
        ArtifactValueTemplate::Literal { .. } => true,
        ArtifactValueTemplate::ReceivedPayload { .. }
        | ArtifactValueTemplate::CurrentStatePayload { .. }
        | ArtifactValueTemplate::EnumPayload { .. }
        | ArtifactValueTemplate::RecordField { .. }
        | ArtifactValueTemplate::ListElement { .. }
        | ArtifactValueTemplate::ListPrefixElement { .. }
        | ArtifactValueTemplate::ListRest { .. }
        | ArtifactValueTemplate::MapValue { .. }
        | ArtifactValueTemplate::MapRest { .. }
        | ArtifactValueTemplate::ProcessRef { .. }
        | ArtifactValueTemplate::LoopElement { .. }
        | ArtifactValueTemplate::Equality { .. }
        | ArtifactValueTemplate::BooleanNot { .. }
        | ArtifactValueTemplate::BooleanBinary { .. } => false,
        ArtifactValueTemplate::EnumVariant { payload, .. } => is_static_map_key_template(payload),
        ArtifactValueTemplate::Record { fields, .. } => fields
            .iter()
            .all(|field| is_static_map_key_template(&field.value)),
        ArtifactValueTemplate::List { items, .. } => items.iter().all(is_static_map_key_template),
        ArtifactValueTemplate::Map { entries, .. } => entries.iter().all(|entry| {
            is_static_map_key_template(&entry.key) && is_static_map_key_template(&entry.value)
        }),
    }
}
