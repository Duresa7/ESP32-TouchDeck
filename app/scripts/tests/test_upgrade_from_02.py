import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "upgrade_from_02.ps1"
PWSH = shutil.which("pwsh")


@unittest.skipUnless(PWSH, "PowerShell is required")
class UpgradeRunnerBoundary(unittest.TestCase):
    def test_refuses_non_hosted_execution_before_creating_artifacts(self):
        scratch = SCRIPT.parents[2] / ".verify"
        scratch.mkdir(exist_ok=True)
        for actions, environment in (("false", "github-hosted"), ("true", "self-hosted"), ("", "")):
            with self.subTest(actions=actions, environment=environment), tempfile.TemporaryDirectory(dir=scratch) as root:
                artifact = Path(root) / "must-not-exist"
                result = subprocess.run(
                    [PWSH, "-NoProfile", "-File", str(SCRIPT), "-ArtifactDir", str(artifact)],
                    env={**os.environ, "GITHUB_ACTIONS": actions, "RUNNER_ENVIRONMENT": environment},
                    capture_output=True, text=True, timeout=30,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("requires a disposable GitHub-hosted runner", result.stderr)
                self.assertFalse(artifact.exists())


if __name__ == "__main__":
    unittest.main()
