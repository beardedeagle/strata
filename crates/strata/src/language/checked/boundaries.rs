use crate::language::ast::Identifier;

use super::{
    CheckedCapabilityDescriptor, CheckedPortId, CheckedProcessId, CheckedProtocolId, CheckedTypeRef,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedProtocol {
    debug_name: Identifier,
    message_type: CheckedTypeRef,
    required_authority: CheckedCapabilityDescriptor,
}

impl CheckedProtocol {
    pub(in crate::language) fn new(
        debug_name: Identifier,
        message_type: CheckedTypeRef,
        required_authority: CheckedCapabilityDescriptor,
    ) -> Self {
        Self {
            debug_name,
            message_type,
            required_authority,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn message_type(&self) -> &CheckedTypeRef {
        &self.message_type
    }

    pub(in crate::language) fn required_authority(&self) -> CheckedCapabilityDescriptor {
        self.required_authority
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedPort {
    debug_name: Identifier,
    protocol: CheckedProtocolId,
    target_process: CheckedProcessId,
    required_authority: CheckedCapabilityDescriptor,
}

impl CheckedPort {
    pub(in crate::language) fn new(
        debug_name: Identifier,
        protocol: CheckedProtocolId,
        target_process: CheckedProcessId,
        required_authority: CheckedCapabilityDescriptor,
    ) -> Self {
        Self {
            debug_name,
            protocol,
            target_process,
            required_authority,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn protocol(&self) -> CheckedProtocolId {
        self.protocol
    }

    pub(in crate::language) fn target_process(&self) -> CheckedProcessId {
        self.target_process
    }

    pub(in crate::language) fn required_authority(&self) -> CheckedCapabilityDescriptor {
        self.required_authority
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedComponent {
    debug_name: Identifier,
    export_port: CheckedPortId,
    required_authority: CheckedCapabilityDescriptor,
}

impl CheckedComponent {
    pub(in crate::language) fn new(
        debug_name: Identifier,
        export_port: CheckedPortId,
        required_authority: CheckedCapabilityDescriptor,
    ) -> Self {
        Self {
            debug_name,
            export_port,
            required_authority,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn export_port(&self) -> CheckedPortId {
        self.export_port
    }

    pub(in crate::language) fn required_authority(&self) -> CheckedCapabilityDescriptor {
        self.required_authority
    }
}
