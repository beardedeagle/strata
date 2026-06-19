use std::fmt::{self, Write as _};
use std::io;

use mantle_artifact::ArtifactValue;

use crate::event::{
    RUNTIME_TRACE_SCHEMA_ID, RUNTIME_TRACE_SCHEMA_VERSION, RuntimeBranchPath,
    RuntimeEventCompositionContext, RuntimeLoopContext, RuntimeProcessId,
};

pub(super) struct JsonStr<'a>(pub(super) &'a str);

impl fmt::Display for JsonStr<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_json_escaped(formatter, self.0)
    }
}

pub(super) struct TraceSchemaJson(pub(super) Option<RuntimeEventCompositionContext>);

impl fmt::Display for TraceSchemaJson {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(context) = self.0 {
            write!(
                formatter,
                ",\"deployment_id\":{},\"composition_id\":{}",
                context.deployment_id, context.composition_id
            )?;
            if let Some(instance_id) = context.component_instance_id {
                write!(
                    formatter,
                    ",\"component_instance_id\":{}",
                    instance_id.as_u32()
                )?;
            }
        }
        write!(
            formatter,
            ",\"trace_schema\":\"{}\",\"trace_schema_version\":{}}}",
            JsonStr(RUNTIME_TRACE_SCHEMA_ID),
            RUNTIME_TRACE_SCHEMA_VERSION
        )
    }
}

fn write_json_escaped(output: &mut impl fmt::Write, value: &str) -> fmt::Result {
    for ch in value.chars() {
        match ch {
            '"' => output.write_str("\\\"")?,
            '\\' => output.write_str("\\\\")?,
            '\u{08}' => output.write_str("\\b")?,
            '\u{0c}' => output.write_str("\\f")?,
            '\n' => output.write_str("\\n")?,
            '\r' => output.write_str("\\r")?,
            '\t' => output.write_str("\\t")?,
            control if control.is_control() => write!(output, "\\u{:04x}", control as u32)?,
            other => output.write_char(other)?,
        }
    }
    Ok(())
}

pub(super) struct BranchPathJson<'a>(pub(super) &'a RuntimeBranchPath);

impl fmt::Display for BranchPathJson<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_char('[')?;
        for (index, segment) in self.0.segments().iter().enumerate() {
            if index > 0 {
                formatter.write_char(',')?;
            }
            write!(formatter, "{segment}")?;
        }
        formatter.write_char(']')
    }
}

pub(super) struct LoopContextJson(pub(super) Option<RuntimeLoopContext>);

impl fmt::Display for LoopContextJson {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(context) = self.0 {
            write!(
                formatter,
                ",\"loop_element_id\":{},\"loop_index\":{}",
                context.element_id.as_u32(),
                context.index
            )?;
        }
        Ok(())
    }
}

pub(super) struct OptionalU32Field<'a> {
    field: &'a str,
    value: Option<u32>,
}

impl<'a> OptionalU32Field<'a> {
    pub(super) const fn new(field: &'a str, value: Option<u32>) -> Self {
        Self { field, value }
    }
}

impl fmt::Display for OptionalU32Field<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(value) = self.value {
            write!(formatter, ",\"{}\":{}", self.field, value)?;
        }
        Ok(())
    }
}

pub(super) struct PayloadJson<'a>(pub(super) &'a Option<crate::program::RuntimePayload>);

impl fmt::Display for PayloadJson<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(payload) = self.0 {
            write!(
                formatter,
                ",\"payload_type_id\":{},\"payload\":\"{}\"",
                payload.ty.as_u32(),
                JsonValueLabel(&payload.value)
            )?;
            if let Some(process_ref) = payload.process_ref {
                write!(
                    formatter,
                    ",\"payload_process_id\":{},\"payload_pid\":{}",
                    process_ref.target_process.as_u32(),
                    process_ref.pid
                )?;
            }
        }
        Ok(())
    }
}

pub(super) struct JsonValueLabel<'a>(pub(super) &'a ArtifactValue);

impl fmt::Display for JsonValueLabel<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_value_label_json(formatter, self.0)
    }
}

fn write_value_label_json(output: &mut impl fmt::Write, value: &ArtifactValue) -> fmt::Result {
    match value {
        ArtifactValue::Atom(value) => write_json_escaped(output, value),
        ArtifactValue::String(value) => write_data_value_json(output, "String(", value.as_bytes()),
        ArtifactValue::Bytes(value) => write_data_value_json(output, "Bytes(", value),
        ArtifactValue::Scalar(value) => write!(output, "{}{}", value.value(), value.ty().suffix()),
        ArtifactValue::EnumVariant { variant, payload } => {
            write_json_escaped(output, variant)?;
            output.write_char('(')?;
            write_value_label_json(output, payload)?;
            output.write_char(')')
        }
        ArtifactValue::Record {
            constructor,
            fields,
        } => {
            write_json_escaped(output, constructor)?;
            output.write_char('{')?;
            for (index, field) in fields.iter().enumerate() {
                if index > 0 {
                    output.write_char(',')?;
                }
                write_json_escaped(output, &field.name)?;
                output.write_char(':')?;
                write_value_label_json(output, &field.value)?;
            }
            output.write_char('}')
        }
        ArtifactValue::List(items) => {
            output.write_str("List[")?;
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.write_char(',')?;
                }
                write_value_label_json(output, item)?;
            }
            output.write_char(']')
        }
        ArtifactValue::Map(entries) => {
            output.write_str("Map[")?;
            for (index, entry) in entries.iter().enumerate() {
                if index > 0 {
                    output.write_char(',')?;
                }
                write_value_label_json(output, &entry.key)?;
                output.write_str("=>")?;
                write_value_label_json(output, &entry.value)?;
            }
            output.write_char(']')
        }
        ArtifactValue::ProcessRef { type_id, pid } => {
            write!(output, "type{}#{pid}", type_id.as_u32())
        }
    }
}

fn write_data_value_json(output: &mut impl fmt::Write, prefix: &str, bytes: &[u8]) -> fmt::Result {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    output.write_str(prefix)?;
    for byte in bytes {
        output.write_char(HEX[usize::from(byte >> 4)] as char)?;
        output.write_char(HEX[usize::from(byte & 0x0f)] as char)?;
    }
    output.write_char(')')
}

pub(super) struct NullableU64(pub(super) Option<u64>);

impl fmt::Display for NullableU64 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(formatter, "{value}"),
            None => formatter.write_str("null"),
        }
    }
}

pub(super) struct NullableProcessId(pub(super) Option<RuntimeProcessId>);

impl fmt::Display for NullableProcessId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(formatter, "{}", value.as_u64()),
            None => formatter.write_str("null"),
        }
    }
}

#[derive(Default)]
pub(super) struct JsonLenCounter {
    pub(super) len: usize,
}

impl fmt::Write for JsonLenCounter {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.len = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        Ok(())
    }
}

pub(super) struct IoFmtWriter<'a, W: io::Write> {
    writer: &'a mut W,
    error: Option<io::Error>,
}

impl<'a, W: io::Write> IoFmtWriter<'a, W> {
    pub(super) fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            error: None,
        }
    }

    pub(super) fn take_error(&mut self) -> Option<io::Error> {
        self.error.take()
    }
}

impl<W: io::Write> fmt::Write for IoFmtWriter<'_, W> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.error.is_some() {
            return Err(fmt::Error);
        }
        if let Err(err) = self.writer.write_all(value.as_bytes()) {
            self.error = Some(err);
            return Err(fmt::Error);
        }
        Ok(())
    }
}
