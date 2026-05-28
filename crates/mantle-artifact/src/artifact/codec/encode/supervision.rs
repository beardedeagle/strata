use super::super::super::*;

pub(super) fn encode_spawn_sites(encoded: &mut String, prefix: &str, process: &ArtifactProcess) {
    encoded.push_str(&format!(
        "{prefix}.spawn_site_count={}\n",
        process.spawn_sites.len()
    ));
    for (spawn_site_index, spawn_site) in process.spawn_sites.iter().enumerate() {
        let spawn_site_prefix = format!("{prefix}.spawn_site.{spawn_site_index}");
        encoded.push_str(&format!(
            "{spawn_site_prefix}.target_process={}\n{spawn_site_prefix}.kind={}\n",
            spawn_site.target.as_u32(),
            spawn_site.kind.as_str()
        ));
        if let Some(authority) = spawn_site.authority {
            encoded.push_str(&format!(
                "{spawn_site_prefix}.authority={}\n",
                authority.as_u32()
            ));
        }
        if let Some(supervisor) = spawn_site.supervisor {
            encoded.push_str(&format!(
                "{spawn_site_prefix}.supervisor={}\n",
                supervisor.as_u32()
            ));
        }
        if let Some(child) = spawn_site.child {
            encoded.push_str(&format!(
                "{spawn_site_prefix}.supervisor_child={}\n",
                child.as_u32()
            ));
        }
    }
}

pub(super) fn encode_supervisor_plans(
    encoded: &mut String,
    prefix: &str,
    process: &ArtifactProcess,
) {
    encoded.push_str(&format!(
        "{prefix}.supervisor_count={}\n",
        process.supervisor_plans.len()
    ));
    for (supervisor_index, supervisor) in process.supervisor_plans.iter().enumerate() {
        let supervisor_prefix = format!("{prefix}.supervisor.{supervisor_index}");
        encoded.push_str(&format!(
            "{supervisor_prefix}.strategy={}\n{supervisor_prefix}.max_restarts={}\n{supervisor_prefix}.within_ms={}\n{supervisor_prefix}.child_count={}\n",
            supervisor.strategy.as_str(),
            supervisor.intensity.max_restarts,
            supervisor.intensity.within_ms,
            supervisor.children.len()
        ));
        for (child_index, child) in supervisor.children.iter().enumerate() {
            let child_prefix = format!("{supervisor_prefix}.child.{child_index}");
            encoded.push_str(&format!(
                "{child_prefix}.debug_name={}\n{child_prefix}.target_process={}\n{child_prefix}.mode={}\n{child_prefix}.spawn_site={}\n",
                child.debug_name,
                child.target.as_u32(),
                child.mode.as_str(),
                child.spawn_site.as_u32()
            ));
        }
    }
}
