use crate::language::ast::Identifier;

use super::{
    CheckedCapabilityDescriptor, CheckedComponentId, CheckedComponentInstanceId,
    CheckedPortBindingId, CheckedPortId, CheckedProcessId, CheckedProtocolId, CheckedTypeRef,
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
    import_ports: Vec<CheckedPortId>,
    required_authority: CheckedCapabilityDescriptor,
}

impl CheckedComponent {
    pub(in crate::language) fn new(
        debug_name: Identifier,
        export_port: CheckedPortId,
        import_ports: Vec<CheckedPortId>,
        required_authority: CheckedCapabilityDescriptor,
    ) -> Self {
        Self {
            debug_name,
            export_port,
            import_ports,
            required_authority,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn export_port(&self) -> CheckedPortId {
        self.export_port
    }

    pub(in crate::language) fn import_ports(&self) -> &[CheckedPortId] {
        &self.import_ports
    }

    pub(in crate::language) fn required_authority(&self) -> CheckedCapabilityDescriptor {
        self.required_authority
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedComposition {
    debug_name: Identifier,
    component_instances: Vec<CheckedComponentInstance>,
    port_bindings: Vec<CheckedPortBinding>,
}

impl CheckedComposition {
    pub(in crate::language) fn new(
        debug_name: Identifier,
        component_instances: Vec<CheckedComponentInstance>,
        port_bindings: Vec<CheckedPortBinding>,
    ) -> Self {
        Self {
            debug_name,
            component_instances,
            port_bindings,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn component_instances(&self) -> &[CheckedComponentInstance] {
        &self.component_instances
    }

    pub(in crate::language) fn port_bindings(&self) -> &[CheckedPortBinding] {
        &self.port_bindings
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedComponentInstance {
    debug_name: Identifier,
    component: CheckedComponentId,
}

impl CheckedComponentInstance {
    pub(in crate::language) fn new(debug_name: Identifier, component: CheckedComponentId) -> Self {
        Self {
            debug_name,
            component,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn component(&self) -> CheckedComponentId {
        self.component
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedPortBinding {
    id: CheckedPortBindingId,
    importer: CheckedComponentInstanceId,
    imported_port: CheckedPortId,
    exporter: CheckedComponentInstanceId,
    exported_port: CheckedPortId,
}

impl CheckedPortBinding {
    pub(in crate::language) fn new(
        id: CheckedPortBindingId,
        importer: CheckedComponentInstanceId,
        imported_port: CheckedPortId,
        exporter: CheckedComponentInstanceId,
        exported_port: CheckedPortId,
    ) -> Self {
        Self {
            id,
            importer,
            imported_port,
            exporter,
            exported_port,
        }
    }

    pub(in crate::language) fn id(&self) -> CheckedPortBindingId {
        self.id
    }

    pub(in crate::language) fn importer(&self) -> CheckedComponentInstanceId {
        self.importer
    }

    pub(in crate::language) fn imported_port(&self) -> CheckedPortId {
        self.imported_port
    }

    pub(in crate::language) fn exporter(&self) -> CheckedComponentInstanceId {
        self.exporter
    }

    pub(in crate::language) fn exported_port(&self) -> CheckedPortId {
        self.exported_port
    }
}
