#![forbid(unsafe_code)]

#[path = "source_to_runtime_gates/support.rs"]
mod support;

#[path = "source_to_runtime_gates/authority_effect_binding.rs"]
mod authority_effect_binding;
#[path = "source_to_runtime_gates/basic_runtime.rs"]
mod basic_runtime;
#[path = "source_to_runtime_gates/boundary_contracts.rs"]
mod boundary_contracts;
#[path = "source_to_runtime_gates/component_composition.rs"]
mod component_composition;
#[path = "source_to_runtime_gates/local_supervision.rs"]
mod local_supervision;
#[path = "source_to_runtime_gates/payload_dispatch.rs"]
mod payload_dispatch;
#[path = "source_to_runtime_gates/process_refs_and_authority.rs"]
mod process_refs_and_authority;
#[path = "source_to_runtime_gates/runtime_branches.rs"]
mod runtime_branches;
#[path = "source_to_runtime_gates/runtime_effect_outcomes.rs"]
mod runtime_effect_outcomes;
#[path = "source_to_runtime_gates/runtime_loop_admission.rs"]
mod runtime_loop_admission;
#[path = "source_to_runtime_gates/runtime_loop_execution.rs"]
mod runtime_loop_execution;
#[path = "source_to_runtime_gates/source_data_primitives.rs"]
mod source_data_primitives;
#[path = "source_to_runtime_gates/source_functions.rs"]
mod source_functions;
#[path = "source_to_runtime_gates/source_units.rs"]
mod source_units;
#[path = "source_to_runtime_gates/state_collections.rs"]
mod state_collections;
