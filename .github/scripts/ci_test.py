import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('ci', Path(__file__).with_name('ci.py'))
ci = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci)
REAL_ROOT = ci.ROOT


class RepositoryChecks(unittest.TestCase):
    def setUp(self):
        environment = patch.dict(os.environ, {'CI_BASE_SHA': '', 'CI_HEAD_SHA': ''})
        environment.start()
        self.addCleanup(environment.stop)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        ci.ROOT = Path(self.temp.name)
        self.addCleanup(setattr, ci, 'ROOT', REAL_ROOT)
        subprocess.run(['git', 'init', '-q', str(ci.ROOT)], check=True)
        for name in ('LICENSE', '.github/CODEOWNERS', '.github/integration-policy.json'):
            self.write(name, (REAL_ROOT / name).read_text())

    def write(self, name, text):
        path = ci.ROOT / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        subprocess.run(['git', 'add', '--', name], cwd=ci.ROOT, check=True)

    def test_documentation_only_has_no_fake_application_checks(self):
        data = ci.inventory()
        self.assertFalse(data['rust'])
        self.assertFalse(data['web'])
        ci.repository_check()

    def test_missing_document_link_fails(self):
        self.write('README.md', '[Missing](absent.md)')
        with self.assertRaisesRegex(ValueError, 'missing relative link'):
            ci.repository_check()

    def test_examples_and_external_links_are_not_local_targets(self):
        self.write('README.md', '[External](https://example.com)\n```md\n[Example](absent.md)\n```\n')
        ci.repository_check()

    def test_unclosed_fence_fails(self):
        self.write('README.md', '```python\nexample\n')
        with self.assertRaisesRegex(ValueError, 'unclosed'):
            ci.repository_check()

    def test_codeowners_policy_drift_fails(self):
        self.write('.github/CODEOWNERS', '* @someone-else\n')
        with self.assertRaisesRegex(ValueError, 'CODEOWNERS'):
            ci.repository_check()

    def test_license_removal_fails(self):
        self.write('LICENSE', 'Different terms')
        with self.assertRaisesRegex(ValueError, 'MIT'):
            ci.repository_check()

    def test_graph_artifacts_fail(self):
        self.write('graphify-out/graph.json', '{}')
        with self.assertRaisesRegex(ValueError, 'Graphify'):
            ci.repository_check()

    def test_rust_needs_a_lockfile(self):
        self.write('Cargo.toml', '[package]\nname="example"\nversion="0.1.0"')
        with self.assertRaisesRegex(ValueError, 'lockfile'):
            ci.inventory()

    def test_rust_needs_a_pinned_toolchain(self):
        self.write('Cargo.toml', '[workspace]')
        self.write('Cargo.lock', '')
        with self.assertRaisesRegex(ValueError, 'rust-toolchain'):
            ci.inventory()
        self.write('rust-toolchain.toml', '[toolchain]\nchannel="stable"')
        with self.assertRaisesRegex(ValueError, 'exact stable'):
            ci.inventory()
        self.write('rust-toolchain.toml', '[toolchain]\nchannel="1.90.0"')
        self.assertTrue(ci.inventory()['rust'])

    def test_workspace_members_do_not_duplicate_rust_jobs(self):
        self.write('Cargo.toml', '[workspace]\nmembers=["crates/example"]')
        self.write('crates/example/Cargo.toml', '[package]\nname="example"\nversion="0.1.0"')
        self.write('Cargo.lock', '')
        self.write('rust-toolchain.toml', '[toolchain]\nchannel="1.90.0"')
        self.assertEqual(ci.inventory()['rust_manifests'], ['Cargo.toml'])

    def test_unreviewed_node_service_location_fails(self):
        self.write('apps/backend/package.json', '{}')
        with self.assertRaisesRegex(ValueError, 'Unreviewed Node'):
            ci.inventory()

    def test_frontend_cannot_silently_skip_scripts_or_lockfile(self):
        self.write('apps/web/package.json', '{}')
        with self.assertRaisesRegex(ValueError, 'Frontend requires scripts'):
            ci.inventory()
        package = {'scripts': {script: 'example' for script in
            ('lint', 'typecheck', 'test:ci', 'build', 'verify:dist', 'check:browser-assets')}}
        self.write('apps/web/package.json', json.dumps(package))
        with self.assertRaisesRegex(ValueError, 'lockfile'):
            ci.inventory()
        self.write('apps/web/package-lock.json', '{}')
        self.assertTrue(ci.inventory()['web'])

    def test_sql_migrations_need_an_explicit_replay_gate(self):
        self.write('migrations/0001.sql', 'CREATE TABLE example (id INTEGER PRIMARY KEY);')
        with self.assertRaisesRegex(ValueError, 'replay gate'):
            ci.inventory()

    def test_migration_replay_cannot_hide_in_a_skipped_rust_job(self):
        self.write('migrations/0001.sql', 'CREATE TABLE example (id INTEGER PRIMARY KEY);')
        self.write('.github/scripts/check-migrations.sh', 'exit 0\n')
        with self.assertRaisesRegex(ValueError, 'Rust workspace'):
            ci.inventory()


if __name__ == '__main__':
    unittest.main()
