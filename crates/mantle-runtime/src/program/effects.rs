use mantle_artifact::{ArtifactEffect, Error, MessageId, Result};

use super::LoadedAction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedEffectAuthority {
    effects: Vec<ArtifactEffect>,
}

impl LoadedEffectAuthority {
    pub(crate) fn from_artifact(effects: &[ArtifactEffect]) -> Self {
        Self {
            effects: effects.to_vec(),
        }
    }

    pub(crate) fn validate_actions(
        &self,
        process_name: &str,
        message: MessageId,
        actions: &[LoadedAction],
    ) -> Result<()> {
        let mut admitted = [false; 3];
        for &effect in &self.effects {
            let index = Self::effect_index(effect);
            if admitted[index] {
                return Err(Error::new(format!(
                    "process {process_name} transition {} admits duplicate effect {effect}",
                    message.as_u32()
                )));
            }
            admitted[index] = true;
        }

        let mut used = [false; 3];
        for action in actions {
            let effect = action.effect();
            let index = Self::effect_index(effect);
            if !admitted[index] {
                return Err(Error::new(format!(
                    "process {process_name} transition {} uses effect {effect} without admitted authority",
                    message.as_u32()
                )));
            }
            used[index] = true;
        }

        for &effect in &self.effects {
            if !used[Self::effect_index(effect)] {
                return Err(Error::new(format!(
                    "process {process_name} transition {} admits effect {effect} but no action uses it",
                    message.as_u32()
                )));
            }
        }

        Ok(())
    }

    fn effect_index(effect: ArtifactEffect) -> usize {
        match effect {
            ArtifactEffect::Emit => 0,
            ArtifactEffect::Spawn => 1,
            ArtifactEffect::Send => 2,
        }
    }
}
