#!/usr/bin/env python3
"""Compatibility entry points and isolated-copy transport, using synthetic data."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / 'lib'))
import devtools_bridge


def load(path):
    spec = importlib.util.spec_from_file_location(path.stem, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class EntryPoints(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Build this source even if a caller has exported some older executable.
        with patch.dict(os.environ):
            os.environ.pop('RICHOS_DEVTOOLS_BIN', None)
            devtools_bridge.binary.cache_clear()
            cls.binary = devtools_bridge.binary()
        cls.environment = dict(os.environ, RICHOS_DEVTOOLS_BIN=cls.binary,
                               PYTHONDONTWRITEBYTECODE='1')

    def test_cargo_summary_import_and_cli_status(self):
        self.assertEqual(Path(self.binary).parent.name, 'release')
        log = ('Running fixture\ntest result: FAILED. 2 passed; 1 failed; 0 ignored;\n'
               'Doc-tests fixture\ntest result: ok. 4 passed; 0 failed; 0 ignored;\n')
        with patch.dict(os.environ, self.environment):
            result = load(HERE / 'rust-test-summary.py').summarize(log)
        self.assertEqual(result['ordinary']['passed'], 2)
        self.assertEqual(result['documentation']['passed'], 4)
        self.assertFalse(result['process_exit_verified'])
        with tempfile.TemporaryDirectory(prefix='devtools-cli-') as tmp:
            path = Path(tmp) / 'log.txt'
            path.write_text(log)
            run = subprocess.run([sys.executable, '-B', str(HERE / 'rust-test-summary.py'), str(path)],
                                 env=self.environment, capture_output=True, text=True)
        self.assertEqual(run.returncode, 1, run.stderr)
        self.assertEqual(json.loads(run.stdout), result)

    def test_golden_contracts(self):
        import cargo_identity
        manifest = HERE.parent / 'crates/richos-devtools/Cargo.toml'
        environment = cargo_identity.enable_environment(dict(os.environ))
        _, target = cargo_identity.target_dir(manifest.parent, environment)
        run = subprocess.run([cargo_identity.cargo_executable(environment), 'test', '--locked',
                              '--manifest-path', str(manifest), '--target-dir', str(target),
                              '--test', 'contracts'], env=environment, capture_output=True, text=True)
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn('2 passed; 0 failed', run.stdout)

    def test_rust_lints(self):
        import cargo_identity
        manifest = HERE.parent / 'crates/richos-devtools/Cargo.toml'
        environment = cargo_identity.enable_environment(dict(os.environ))
        _, target = cargo_identity.target_dir(manifest.parent, environment)
        run = subprocess.run([cargo_identity.cargo_executable(environment), 'clippy', '--locked',
                              '--manifest-path', str(manifest), '--target-dir', str(target),
                              '--all-targets', '--', '-D', 'warnings'],
                             env=environment, capture_output=True, text=True)
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)

    def test_private_home_does_not_hide_installed_toolchain(self):
        import cargo_identity
        cargo = cargo_identity.cargo_executable(os.environ)
        with tempfile.TemporaryDirectory(prefix='devtools-home-') as tmp:
            environment = dict(os.environ, HOME=tmp,
                               PATH=str(Path(cargo).parent) + os.pathsep + os.environ.get('PATH', ''))
            environment.pop('RICHOS_DEVTOOLS_BIN', None)
            run = subprocess.run([sys.executable, '-B', str(HERE / 'lib/devtools_bridge.py'), '--binary'],
                                 env=environment, capture_output=True, text=True)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertTrue(Path(run.stdout.strip()).is_file())
            self.assertFalse((Path(tmp) / '.claude').exists())

    def test_explicit_toolchain_locations_are_preserved(self):
        with tempfile.TemporaryDirectory(prefix='devtools-toolchain-') as tmp:
            root = Path(tmp)
            proxy = root / '.cargo/bin/cargo'
            proxy.parent.mkdir(parents=True)
            (proxy.parent / 'rustup').touch()
            (root / '.rustup').mkdir()
            given = {'HOME': 'isolated', 'CARGO_HOME': 'chosen-cargo', 'RUSTUP_HOME': 'chosen-rustup'}
            self.assertEqual(devtools_bridge.toolchain_environment(given, str(proxy)), given)
            inferred = devtools_bridge.toolchain_environment({'HOME': 'isolated'}, str(proxy))
            self.assertEqual(inferred['HOME'], 'isolated')
            self.assertEqual(inferred['CARGO_HOME'], str(root / '.cargo'))
            self.assertEqual(inferred['RUSTUP_HOME'], str(root / '.rustup'))

    def test_ledger_missing_and_malformed_are_not_empty_success(self):
        with tempfile.TemporaryDirectory(prefix='devtools-ledger-') as tmp:
            path = Path(tmp) / 'ledger.jsonl'
            with patch.dict(os.environ, self.environment):
                ledger = load(HERE / 'qa/ledger-timeline.py')
                rows, error = ledger.read(path)
                self.assertIsNone(rows)
                self.assertIn('no ledger', error)
                path.write_text('{broken\n')
                rows, error = ledger.read(path)
                self.assertIsNone(rows)
                self.assertIn('line 1', error)

    def test_caption_imports_preserve_normalization(self):
        with patch.dict(os.environ, self.environment):
            caption = load(HERE / 'qa/caption-wer.py')
            self.assertEqual(caption.norm("‘Café’s!’"), "café's")
            result = caption.compare('WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nhello world',
                                     'hello world')
        self.assertEqual(result['wer'], 0)

    def test_node_export_still_checks_source_files(self):
        with tempfile.TemporaryDirectory(prefix='devtools-wait-') as tmp:
            path = Path(tmp)
            (path / 'bad.js').write_text('page.waitForFunction(() => ready, { timeout: 9 });\n')
            (path / 'good.js').write_text('page.waitForFunction(() => ready, null, { timeout: 9 });\n')
            code = 'console.log(JSON.stringify(require(process.argv[1]).findMisplacedTimeouts(process.argv[2])))'
            run = subprocess.run(['node', '-e', code, str(HERE.parent / 'ui/tests/lib/wait-shape.js'), tmp],
                                 env=self.environment, capture_output=True, text=True)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertEqual(json.loads(run.stdout), ['bad.js:1'])

    def test_dictionary_generator_imports_in_engine_only_copy(self):
        source = HERE.parents[1] / 'engine/scripts/lib/dialect/dialect-table.py'
        with tempfile.TemporaryDirectory(prefix='devtools-engine-') as tmp:
            copy = Path(tmp) / 'richos/engine/scripts/lib/dialect/dialect-table.py'
            copy.parent.mkdir(parents=True)
            copy.write_bytes(source.read_bytes())
            with patch.dict(os.environ, self.environment):
                generator = load(copy)
                self.assertEqual(generator.split_varcon([]), ({}, {}, {}))
                with self.assertRaises(generator.Refused):
                    generator.read_varcon(copy, expected_sha256='wrong')


if __name__ == '__main__':
    unittest.main(verbosity=2)
