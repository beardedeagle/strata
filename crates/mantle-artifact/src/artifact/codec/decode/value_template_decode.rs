use super::*;

pub(super) fn decode_if_else_template(
    fields: &mut ArtifactFields,
    prefix: &str,
    depth: usize,
) -> Result<ArtifactValueTemplate> {
    Ok(ArtifactValueTemplate::IfElse {
        ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
        condition: Box::new(decode_value_template(
            fields,
            &format!("{prefix}.condition"),
            depth + 1,
        )?),
        then_value: Box::new(decode_value_template(
            fields,
            &format!("{prefix}.then"),
            depth + 1,
        )?),
        else_value: Box::new(decode_value_template(
            fields,
            &format!("{prefix}.else"),
            depth + 1,
        )?),
    })
}
