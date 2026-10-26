"""Reference integrity, not certification that a referenced test passed."""
import hashlib
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


class CoreEvidence(unittest.TestCase):
    def test_traceability_points_to_real_tests_interfaces_and_frozen_migrations(self):
        data = json.loads((ROOT / 'docs/development/core-verification-matrix.json').read_text())
        self.assertEqual(data['schema_version'], 1)
        self.assertEqual(data['scope'], 'synthetic_core_only')
        self.assertRegex(data['review_base_sha'], r'^[0-9a-f]{40}$')
        self.assertEqual([row['id'] for row in data['milestones']],
                         [f'R{n:02}' for n in range(1, 11)])
        for row in data['milestones']:
            self.assertEqual(row['status'], 'implemented_verified')
            self.assertTrue(row['boundary'])
            self.assertTrue(row['tests'])
            for reference in row['sources'] + row['tests']:
                name, symbol = reference.split('#')
                path = ROOT / name
                self.assertTrue(path.resolve().is_relative_to(ROOT), reference)
                self.assertFalse(path.is_symlink(), reference)
                text = path.read_text()
                self.assertRegex(text, r'\b' + re.escape(symbol) + r'\b', reference)
                if reference in row['tests']:
                    self.assertRegex(text, r'#\[(?:tokio::)?test\]\s*(?:async\s+)?fn\s+'
                                     + re.escape(symbol) + r'\s*\(', reference)
            for adr in row['adrs']:
                matches = list((ROOT / 'docs/decisions').glob(adr + '-*.md'))
                self.assertEqual(len(matches), 1, adr)
                self.assertRegex(matches[0].read_text(),
                                 r'(?mi)^(?:-\s+)?(?:\*\*)?status:\s*(?:\*\*)?approved\b')
        for name, digest in data['published_migrations'].items():
            self.assertEqual(hashlib.sha256((ROOT / name).read_bytes()).hexdigest(), digest, name)
        gates = {gate['id']: gate['status'] for gate in data['production_gates']}
        self.assertEqual(gates, {'B': 'specified_not_implemented', 'C': 'externally_blocked',
                                 'D': 'specified_not_implemented', 'SUPPLY_CHAIN': 'partially_verified'})
