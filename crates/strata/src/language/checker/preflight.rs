use mantle_artifact::{
    MAX_ENUM_VARIANTS_PER_TYPE, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS,
};

use super::authority::validate_authority_declarations;
use super::state_space::StateSpace;
use super::steps::check_step_shape;
use super::symbols::SemanticIndex;
use super::types::CheckedTypeInterner;
use super::validate_count;
use crate::language::ast::Module;
use crate::language::checked::CheckedProcessId;
use crate::language::diagnostic::{Error, Result};

pub(super) fn validate_enum_variant_counts(module: &Module) -> Result<()> {
    for enum_decl in &module.enums {
        validate_count(
            &format!("enum {} variant_count", enum_decl.name),
            enum_decl.variants.len(),
            0,
            MAX_ENUM_VARIANTS_PER_TYPE,
        )?;
    }
    Ok(())
}

pub(super) fn validate_process_declarations_before_message_cases(
    module: &Module,
    semantic_index: &SemanticIndex,
    entry_process: CheckedProcessId,
) -> Result<()> {
    let mut validation_types = CheckedTypeInterner::new(module, semantic_index);
    for (process_index, process) in module.processes.iter().enumerate() {
        validate_count(
            &format!("process {} mailbox_bound", process.name),
            process.mailbox_bound,
            1,
            MAX_MAILBOX_BOUND,
        )?;
        let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
        if msg_enum.variants.is_empty() {
            return Err(Error::new(format!(
                "enum {} must declare at least one variant",
                msg_enum.name
            )));
        }
        validate_count(
            &format!("process {} message_count", process.name),
            msg_enum.variants.len(),
            1,
            MAX_MESSAGE_VARIANTS_PER_PROCESS,
        )?;
        let _ = StateSpace::new(module, semantic_index, process, &mut validation_types)?;
        validate_authority_declarations(module, semantic_index, process, entry_process)?;
        let process_id = CheckedProcessId::from_index(process_index)?;
        for step in &process.steps {
            check_step_shape(module, process, process_id, semantic_index, step)?;
        }
    }
    Ok(())
}
