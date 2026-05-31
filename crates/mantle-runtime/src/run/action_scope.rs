use super::RuntimeLoopElement;
use super::effect_outcomes::RuntimeEffectOutcome;
use super::model::ActiveStep;
use super::process_refs::LocalProcessRefs;
use crate::event::{RuntimeBranchPath, RuntimeBranchScope};
use crate::executable::ExecutableActionPlan;
use crate::program::LoadedValueTemplate;

pub(super) struct BranchSelection<'a> {
    pub(super) step: &'a ActiveStep,
    pub(super) scope: RuntimeBranchScope,
    pub(super) branch_path: RuntimeBranchPath,
    pub(super) condition: &'a LoadedValueTemplate,
    pub(super) local_process_refs: &'a LocalProcessRefs,
    pub(super) loop_elements: &'a [RuntimeLoopElement<'a>],
    pub(super) effect_outcomes: &'a [RuntimeEffectOutcome],
}

#[derive(Clone, Copy)]
pub(super) struct RuntimeActionScope<'a, 'program> {
    pub(super) executable_actions: &'a [ExecutableActionPlan<'program>],
    pub(super) loop_elements: &'a [RuntimeLoopElement<'a>],
    pub(super) effect_outcomes: &'a [RuntimeEffectOutcome],
}
