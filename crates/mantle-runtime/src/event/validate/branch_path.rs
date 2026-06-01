use mantle_artifact::Result;

use super::json::JsonLine;
use crate::event::{RUNTIME_BRANCH_PATH_CAPACITY, RuntimeBranchPathSegment};

pub(super) fn validate_branch_path(line: &JsonLine<'_>) -> Result<()> {
    line.required_bounded_u16_array("branch_path", RUNTIME_BRANCH_PATH_CAPACITY, |segment| {
        RuntimeBranchPathSegment::is_valid_encoded(segment)
            .then_some(())
            .ok_or("contains a segment outside Mantle runtime branch-path encoding")
    })
}
