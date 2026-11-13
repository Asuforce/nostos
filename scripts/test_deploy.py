import unittest
import os
from pathlib import Path
import subprocess
import tempfile
from check_deploy import check


class DeployContract(unittest.TestCase):
    def test_cron_with_http_disabled(self):
        check({"triggers": {"crons": ["0 0 * * *"]}})

    def test_http_after_cron_removal(self):
        check({"vars": {"HTTP_JOBS_ENABLED": "true"}, "triggers": {"crons": []}})

    def test_overlap_rejected(self):
        for enabled in ("true", True, "TRUE"):
            with self.assertRaises(ValueError):
                check({"vars": {"HTTP_JOBS_ENABLED": enabled}, "triggers": {"crons": ["0 0 * * *"]}})

    def test_secret_delivery_and_missing_secret(self):
        workflow = (Path(__file__).parents[1] / ".github/workflows/ci.yml").read_text()
        block = workflow.split("      - name: Set job token", 1)[1].split("      - run: npm run deploy", 1)[0]
        self.assertIn("NOSTOS_JOB_TOKEN: ${{ secrets.NOSTOS_JOB_TOKEN }}", block)
        script = block.split("        run: |\n", 1)[1]
        script = "\n".join(line[10:] for line in script.splitlines())
        with tempfile.TemporaryDirectory() as directory:
            mock = Path(directory) / "npx"
            mock.write_text('#!/bin/bash\n[[ "$*" == "wrangler secret put JOB_TOKEN" ]] || exit 2\ncat > "$CAPTURE"\necho should-be-hidden\n')
            mock.chmod(0o700)
            capture = Path(directory) / "capture"
            env = dict(os.environ, PATH=directory + ":" + os.environ["PATH"], CAPTURE=str(capture))
            missing = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script], env=dict(env, NOSTOS_JOB_TOKEN=""), capture_output=True)
            self.assertNotEqual(missing.returncode, 0)
            self.assertFalse(capture.exists())
            # Fixed fixture, never an actual credential.
            result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script], env=dict(env, NOSTOS_JOB_TOKEN="contract-fixture"), capture_output=True)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(capture.read_text(), "contract-fixture")
            self.assertEqual(result.stdout + result.stderr, b"")


if __name__ == "__main__":
    unittest.main()
