import copy
import json
import os
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch
from pathlib import Path

from self_compiler_profile import CAPTURE_ENV, expected_units, primary_invocation, validate_coverage, wrapper


class CompilerProfileChecks(unittest.TestCase):
    def records(self):
        return [{'unit': ['/workspace', '/workspace/src/lib.rs', test],
                 'cfg_exit_code': 0, 'compile_exit_code': 0,
                 'cfg': ['unix', 'test'] if test else ['unix']}
                for test in (False, True)]

    def test_metadata_demands_both_normal_and_test_contexts(self):
        target = {'kind': ['lib'], 'src_path': '/workspace/src/lib.rs', 'test': True}
        metadata = {'workspace_members': ['p'], 'packages': [
            {'id': 'p', 'manifest_path': '/workspace/Cargo.toml', 'targets': [target]},
            {'id': 'dependency', 'targets': []}]}
        self.assertEqual(expected_units(metadata), {
            (str(Path('/workspace').resolve()), str(Path('/workspace/src/lib.rs').resolve()), mode)
            for mode in (False, True)})
        target['required-features'] = ['unsupported-profile']
        with self.assertRaises(ValueError):
            expected_units(metadata)
        del target['required-features']
        target['kind'] = ['example']
        with self.assertRaises(ValueError):
            expected_units(metadata)

    def test_missing_duplicate_failed_and_mismatched_cfg_are_not_evidence(self):
        records = self.records()
        expected = {tuple(row['unit']) for row in records}
        validate_coverage(records, expected)
        for invalid in (records[:1], records + records[:1], []):
            with self.assertRaises(ValueError):
                validate_coverage(invalid, expected)
        for field, value in [('compile_exit_code', 1), ('cfg_exit_code', 1), ('cfg', []), ('cfg', ['unix', 'test'])]:
            invalid = copy.deepcopy(records)
            invalid[0][field] = value
            with self.assertRaises(ValueError):
                validate_coverage(invalid, expected)

    def test_path_resolved_compiler_is_probed_without_changing_compile_arguments(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'lib.rs'
            source.write_text('pub struct Example;')
            flags = ['--crate-name', 'example', str(source)]
            environment = {CAPTURE_ENV: directory, 'RQLENS_SELF_RUSTC': sys.executable,
                           'CARGO_PRIMARY_PACKAGE': '1', 'CARGO_PKG_NAME': 'example', 'CARGO_MANIFEST_DIR': directory}
            with patch.dict(os.environ, environment), patch('shutil.which', return_value=sys.executable), patch('subprocess.run') as execute:
                execute.side_effect = [SimpleNamespace(returncode=0, stdout='unix\n', stderr=''), SimpleNamespace(returncode=0)]
                self.assertEqual(wrapper(['rustc', *flags]), 0)
                self.assertEqual(execute.call_args_list[0].args[0], ['rustc', *flags, '--print=cfg'])
                self.assertEqual(execute.call_args_list[1].args[0], ['rustc', *flags])
            records = list(Path(directory).glob('*.json'))
            self.assertEqual(len(records), 1)
            record = json.loads(records[0].read_text())
            validate_coverage([record], {tuple(record['unit'])})

    def test_dependency_compiles_and_version_queries_are_not_primary_units(self):
        self.assertFalse(primary_invocation(['-vV'], {'CARGO_PRIMARY_PACKAGE': '1'}))
        self.assertFalse(primary_invocation(['--crate-name', 'dep'], {}))
        self.assertTrue(primary_invocation(['--crate-name', 'self'], {'CARGO_PRIMARY_PACKAGE': '1'}))


if __name__ == '__main__':
    unittest.main()
