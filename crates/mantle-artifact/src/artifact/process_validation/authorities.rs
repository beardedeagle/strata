use std::collections::BTreeSet;

use super::*;
use crate::{MAX_AUTHORITIES_PER_PROCESS, MAX_SPAWN_SITES_PER_PROCESS};

const AUTHORITY_REFERENCE_WORD_BITS: usize = u64::BITS as usize;
const AUTHORITY_REFERENCE_WORDS: usize =
    MAX_AUTHORITIES_PER_PROCESS.div_ceil(AUTHORITY_REFERENCE_WORD_BITS);
const SPAWN_SITE_REFERENCE_WORDS: usize =
    MAX_SPAWN_SITES_PER_PROCESS.div_ceil(AUTHORITY_REFERENCE_WORD_BITS);

pub(in crate::artifact::process_validation) struct AuthorityUsage {
    referenced_spawn_sites: [u64; SPAWN_SITE_REFERENCE_WORDS],
    referenced_authorities: [u64; AUTHORITY_REFERENCE_WORDS],
}

impl AuthorityUsage {
    pub(in crate::artifact::process_validation) const fn new() -> Self {
        Self {
            referenced_spawn_sites: [0_u64; SPAWN_SITE_REFERENCE_WORDS],
            referenced_authorities: [0_u64; AUTHORITY_REFERENCE_WORDS],
        }
    }

    pub(in crate::artifact::process_validation) fn mark_spawn_site(
        &mut self,
        process_name: &str,
        spawn_site_index: usize,
    ) -> Result<()> {
        mark_reference(
            &mut self.referenced_spawn_sites,
            process_name,
            "spawn site",
            spawn_site_index,
        )
    }

    pub(in crate::artifact::process_validation) fn mark_authority(
        &mut self,
        process_name: &str,
        authority_index: usize,
    ) -> Result<()> {
        mark_reference(
            &mut self.referenced_authorities,
            process_name,
            "authority",
            authority_index,
        )
    }

    fn spawn_site_referenced(&self, spawn_site_index: usize) -> bool {
        reference_marked(&self.referenced_spawn_sites, spawn_site_index)
    }

    fn authority_referenced(&self, authority_index: usize) -> bool {
        reference_marked(&self.referenced_authorities, authority_index)
    }
}

impl ArtifactProcess {
    pub(super) fn validate_authorities(
        &self,
        artifact: &MantleArtifact,
        process_id: ProcessId,
    ) -> Result<()> {
        let mut authority_names = BTreeSet::new();
        let mut authority_descriptors = BTreeSet::new();
        for (authority_index, authority) in self.authorities.iter().enumerate() {
            validate_ident_field(
                &format!(
                    "process {} authority {authority_index} debug_name",
                    self.debug_name
                ),
                &authority.debug_name,
            )?;
            if !authority_names.insert(authority.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "process {} duplicates authority {}",
                    self.debug_name, authority.debug_name
                )));
            }
            if !authority_descriptors.insert(authority.descriptor) {
                return Err(Error::new(format!(
                    "process {} duplicates {}",
                    self.debug_name,
                    authority_descriptor_label(authority.descriptor)
                )));
            }
            match authority.descriptor {
                ArtifactCapabilityDescriptor::Spawn { target } => {
                    artifact.processes.get(target.index()).ok_or_else(|| {
                        Error::new(format!(
                            "process {} authority {} targets undefined process id {}",
                            self.debug_name,
                            authority.debug_name,
                            target.as_u32()
                        ))
                    })?;
                    if target == artifact.entry_process {
                        return Err(Error::new(format!(
                            "process {} authority {} targets entry process id {}",
                            self.debug_name,
                            authority.debug_name,
                            target.as_u32()
                        )));
                    }
                    if target == process_id {
                        return Err(Error::new(format!(
                            "process {} authority {} targets itself, which is not supported",
                            self.debug_name, authority.debug_name
                        )));
                    }
                }
                ArtifactCapabilityDescriptor::PortConnect { port } => {
                    artifact.ports.get(port.index()).ok_or_else(|| {
                        Error::new(format!(
                            "process {} authority {} targets undefined port id {}",
                            self.debug_name,
                            authority.debug_name,
                            port.as_u32()
                        ))
                    })?;
                }
                ArtifactCapabilityDescriptor::ProtocolBoundary { .. }
                | ArtifactCapabilityDescriptor::ComponentExport { .. } => {
                    return Err(Error::new(format!(
                        "process {} authority {} uses a boundary-table-only capability",
                        self.debug_name, authority.debug_name
                    )));
                }
            }
        }

        for (spawn_site_index, spawn_site) in self.spawn_sites.iter().enumerate() {
            artifact
                .processes
                .get(spawn_site.target.index())
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} spawn site {spawn_site_index} targets undefined process id {}",
                        self.debug_name,
                        spawn_site.target.as_u32()
                    ))
                })?;
            match spawn_site.kind {
                ArtifactSpawnKind::DynamicLocal => {
                    if spawn_site.supervisor.is_some() || spawn_site.child.is_some() {
                        return Err(Error::new(format!(
                            "process {} dynamic spawn site {spawn_site_index} carries supervisor ids",
                            self.debug_name
                        )));
                    }
                    let authority_id = spawn_site.authority.ok_or_else(|| {
                        Error::new(format!(
                            "process {} dynamic spawn site {spawn_site_index} has no authority id",
                            self.debug_name
                        ))
                    })?;
                    let authority_index = authority_id.index();
                    let authority = self.authorities.get(authority_index).ok_or_else(|| {
                        Error::new(format!(
                            "process {} spawn site {spawn_site_index} references undefined authority id {}",
                            self.debug_name,
                            authority_id.as_u32()
                        ))
                    })?;
                    match authority.descriptor {
                        ArtifactCapabilityDescriptor::Spawn { target }
                            if target == spawn_site.target => {}
                        ArtifactCapabilityDescriptor::Spawn { target } => {
                            return Err(Error::new(format!(
                                "process {} spawn site {spawn_site_index} targets process id {}, but authority id {} targets {}",
                                self.debug_name,
                                spawn_site.target.as_u32(),
                                authority_id.as_u32(),
                                target.as_u32()
                            )));
                        }
                        _ => {
                            return Err(Error::new(format!(
                                "process {} spawn site {spawn_site_index} authority id {} is not a spawn capability",
                                self.debug_name,
                                authority_id.as_u32()
                            )));
                        }
                    }
                }
                ArtifactSpawnKind::LexicalSupervisorChild => {
                    if spawn_site.authority.is_some() {
                        return Err(Error::new(format!(
                            "process {} lexical supervisor child spawn site {spawn_site_index} carries dynamic authority",
                            self.debug_name
                        )));
                    }
                    if spawn_site.supervisor.is_none() || spawn_site.child.is_none() {
                        return Err(Error::new(format!(
                            "process {} lexical supervisor child spawn site {spawn_site_index} must carry supervisor and child ids",
                            self.debug_name
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_authority_usage(&self, usage: &AuthorityUsage) -> Result<()> {
        for (spawn_site_index, spawn_site) in self.spawn_sites.iter().enumerate() {
            match spawn_site.kind {
                ArtifactSpawnKind::DynamicLocal
                    if !usage.spawn_site_referenced(spawn_site_index) =>
                {
                    return Err(Error::new(format!(
                        "process {} declares unused dynamic spawn site {spawn_site_index}",
                        self.debug_name
                    )));
                }
                ArtifactSpawnKind::LexicalSupervisorChild
                    if !self.supervisor_spawn_site_referenced(spawn_site_index) =>
                {
                    return Err(Error::new(format!(
                        "process {} declares unused lexical supervisor child spawn site {spawn_site_index}",
                        self.debug_name
                    )));
                }
                _ => {}
            }
        }
        for (authority_index, authority) in self.authorities.iter().enumerate() {
            if !usage.authority_referenced(authority_index) {
                return Err(Error::new(format!(
                    "process {} declares unused authority {}",
                    self.debug_name, authority.debug_name
                )));
            }
        }

        Ok(())
    }

    fn supervisor_spawn_site_referenced(&self, spawn_site_index: usize) -> bool {
        self.supervisor_plans.iter().any(|plan| {
            plan.children
                .iter()
                .any(|child| child.spawn_site.index() == spawn_site_index)
        })
    }
}

fn authority_descriptor_label(descriptor: ArtifactCapabilityDescriptor) -> &'static str {
    match descriptor {
        ArtifactCapabilityDescriptor::Spawn { .. } => "spawn authority descriptor",
        ArtifactCapabilityDescriptor::PortConnect { .. } => "port authority descriptor",
        ArtifactCapabilityDescriptor::ProtocolBoundary { .. }
        | ArtifactCapabilityDescriptor::ComponentExport { .. } => "authority descriptor",
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
