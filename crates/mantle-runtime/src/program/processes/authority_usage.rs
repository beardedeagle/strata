use super::*;

const AUTHORITY_REFERENCE_WORD_BITS: usize = u64::BITS as usize;
const AUTHORITY_REFERENCE_WORDS: usize =
    MAX_AUTHORITIES_PER_PROCESS.div_ceil(AUTHORITY_REFERENCE_WORD_BITS);
const SPAWN_SITE_REFERENCE_WORDS: usize =
    MAX_SPAWN_SITES_PER_PROCESS.div_ceil(AUTHORITY_REFERENCE_WORD_BITS);

pub(super) struct LoadedAuthorityUsage {
    referenced_spawn_sites: [u64; SPAWN_SITE_REFERENCE_WORDS],
    referenced_authorities: [u64; AUTHORITY_REFERENCE_WORDS],
}

impl LoadedAuthorityUsage {
    pub(super) const fn new() -> Self {
        Self {
            referenced_spawn_sites: [0_u64; SPAWN_SITE_REFERENCE_WORDS],
            referenced_authorities: [0_u64; AUTHORITY_REFERENCE_WORDS],
        }
    }

    pub(super) fn mark_spawn_site(
        &mut self,
        process_name: &str,
        spawn_site_index: usize,
    ) -> Result<()> {
        mark_reference(
            &mut self.referenced_spawn_sites,
            process_name,
            "loaded spawn site",
            spawn_site_index,
        )
    }

    pub(super) fn mark_authority(
        &mut self,
        process_name: &str,
        authority_index: usize,
    ) -> Result<()> {
        mark_reference(
            &mut self.referenced_authorities,
            process_name,
            "loaded authority",
            authority_index,
        )
    }

    pub(super) fn spawn_site_referenced(&self, spawn_site_index: usize) -> bool {
        reference_marked(&self.referenced_spawn_sites, spawn_site_index)
    }

    pub(super) fn authority_referenced(&self, authority_index: usize) -> bool {
        reference_marked(&self.referenced_authorities, authority_index)
    }
}

fn mark_reference<const N: usize>(
    references: &mut [u64; N],
    process_name: &str,
    reference_name: &str,
    reference_index: usize,
) -> Result<()> {
    let word_index = reference_index / AUTHORITY_REFERENCE_WORD_BITS;
    let bit_index = reference_index % AUTHORITY_REFERENCE_WORD_BITS;
    let Some(word) = references.get_mut(word_index) else {
        return Err(Error::new(format!(
            "process {process_name} {reference_name} id {reference_index} exceeds reference capacity"
        )));
    };
    *word |= 1_u64 << bit_index;
    Ok(())
}

fn reference_marked<const N: usize>(references: &[u64; N], reference_index: usize) -> bool {
    let word_index = reference_index / AUTHORITY_REFERENCE_WORD_BITS;
    let bit_index = reference_index % AUTHORITY_REFERENCE_WORD_BITS;
    references
        .get(word_index)
        .is_some_and(|word| (word & (1_u64 << bit_index)) != 0)
}
