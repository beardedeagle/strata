use std::process::Output;

use super::{GateHarness, assert_failure, assert_success};

impl GateHarness {
    pub(crate) fn authority_effect_build(&self, source: &str, artifact: &str) {
        self.remove_artifact(artifact);
        assert_success(
            self.command(
                &self.strata,
                ["authority-effects", "build", source, "--output", artifact],
                "strata authority-effects build",
            ),
            "strata authority-effects build",
        );
        assert!(
            self.root.join(artifact).exists(),
            "expected {}",
            self.root.join(artifact).display()
        );
    }

    pub(crate) fn authority_effect_admit(&self, artifact: &str, format: &str) -> Output {
        assert_success(
            self.command(
                &self.strata,
                ["authority-effects", "admit", artifact, "--format", format],
                "strata authority-effects admit",
            ),
            "strata authority-effects admit",
        )
    }

    pub(crate) fn authority_effect_admit_failure(&self, artifact: &str) -> Output {
        assert_failure(
            self.command(
                &self.strata,
                ["authority-effects", "admit", artifact],
                "strata authority-effects admit",
            ),
            "strata authority-effects admit",
        )
    }

    pub(crate) fn authority_effect_bind_runtime(
        &self,
        authority_effect_artifact: &str,
        runtime_artifact: &str,
        binding_artifact: &str,
        deny_spawn_authority: bool,
    ) {
        self.remove_artifact(binding_artifact);
        let mut args = Vec::with_capacity(7);
        args.extend([
            "authority-effects",
            "bind-runtime",
            authority_effect_artifact,
            runtime_artifact,
        ]);
        if deny_spawn_authority {
            args.push("--deny-spawn-authority");
        }
        args.extend(["--output", binding_artifact]);
        assert_success(
            self.command_slice(&self.strata, &args, "strata authority-effects bind-runtime"),
            "strata authority-effects bind-runtime",
        );
        assert!(
            self.root.join(binding_artifact).exists(),
            "expected {}",
            self.root.join(binding_artifact).display()
        );
    }

    pub(crate) fn authority_effect_bind_runtime_failure(
        &self,
        authority_effect_artifact: &str,
        runtime_artifact: &str,
        binding_artifact: &str,
        deny_spawn_authority: bool,
    ) -> Output {
        self.remove_artifact(binding_artifact);
        let mut args = Vec::with_capacity(7);
        args.extend([
            "authority-effects",
            "bind-runtime",
            authority_effect_artifact,
            runtime_artifact,
        ]);
        if deny_spawn_authority {
            args.push("--deny-spawn-authority");
        }
        args.extend(["--output", binding_artifact]);
        let output = assert_failure(
            self.command_slice(&self.strata, &args, "strata authority-effects bind-runtime"),
            "strata authority-effects bind-runtime",
        );
        assert!(
            !self.root.join(binding_artifact).exists(),
            "failed authority/effect bind-runtime must not leave {}",
            self.root.join(binding_artifact).display()
        );
        output
    }
}
