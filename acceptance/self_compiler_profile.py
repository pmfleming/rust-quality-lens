#!/usr/bin/env python3
"""Experimental self-only compiler cfg capture, not a measurement producer.

Runs cargo check (including dependency builds, never tests/services) in a fresh
isolated target directory. Records actual primary-package rustc argv and probes
that compiler with the same arguments plus --print=cfg. No owner/hygiene claims.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile

from trust_campaign import source_state

ROOT = Path(__file__).resolve().parent.parent
CAPTURE_ENV = 'RQLENS_SELF_CFG_CAPTURE'
WRAPPERS = ('RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER')


def primary_invocation(arguments, environment):
    return environment.get('CARGO_PRIMARY_PACKAGE') == '1' and '--crate-name' in arguments


def option(arguments, name):
    values = [arguments[i + 1] for i, arg in enumerate(arguments[:-1]) if arg == name]
    if len(values) != 1:
        raise ValueError(f'expected exactly one {name}')
    return values[0]


def unit_key(arguments, environment):
    manifest = Path(environment['CARGO_MANIFEST_DIR']).resolve()
    sources = [arg for arg in arguments if arg.endswith('.rs') and Path(arg).is_file()]
    if len(sources) != 1:
        raise ValueError('expected exactly one explicit Rust source input')
    return (str(manifest), str(Path(sources[0]).resolve()), '--test' in arguments)


def wrapper(arguments):
    # Cargo supplies the compiler as argv[0]. Reject nested wrappers rather than
    # silently claiming their arguments describe rustc itself.
    compiler, *flags = arguments
    destination = Path(os.environ[CAPTURE_ENV])
    record = None
    if primary_invocation(flags, os.environ):
        resolved = shutil.which(compiler)
        if resolved is None or Path(resolved).resolve() != Path(os.environ['RQLENS_SELF_RUSTC']).resolve():
            raise ValueError('unexpected compiler or nested rustc wrapper')
        probe = subprocess.run([compiler, *flags, '--print=cfg'], capture_output=True, text=True, timeout=60)
        record = {
            'compiler': compiler, 'arguments': flags, 'cwd': os.getcwd(),
            'package': os.environ['CARGO_PKG_NAME'],
            'unit': unit_key(flags, os.environ),
            'crate_name': option(flags, '--crate-name'),
            'cfg_exit_code': probe.returncode, 'cfg': probe.stdout.splitlines(),
            'cfg_stderr': probe.stderr,
            'environment': {key: value for key, value in os.environ.items()
                            if key in ('CARGO_ENCODED_RUSTFLAGS', 'RUSTFLAGS', 'RUSTC_BOOTSTRAP', 'CARGO_BUILD_TARGET')
                            or key.startswith('CARGO_CFG_') or key.startswith('CARGO_FEATURE_')},
        }
    result = subprocess.run([compiler, *flags])
    if record is not None:
        record['compile_exit_code'] = result.returncode
        # One file per invocation; no shared JSON append race across Cargo jobs.
        with tempfile.NamedTemporaryFile(mode='w', dir=destination, suffix='.json', delete=False) as output:
            json.dump(record, output, indent=2)
    return result.returncode


def expected_units(metadata):
    members = set(metadata['workspace_members'])
    expected = set()
    for package in metadata['packages']:
        if package['id'] not in members:
            continue
        for target in package['targets']:
            if target.get('required-features'):
                raise ValueError('feature-gated targets need a separately specified profile')
            kinds = set(target['kind'])
            manifest = str(Path(package['manifest_path']).resolve().parent)
            source = str(Path(target['src_path']).resolve())
            if kinds & {'lib', 'bin', 'proc-macro', 'rlib', 'dylib', 'staticlib', 'cdylib'}:
                expected.add((manifest, source, False))
                if target['test']:
                    expected.add((manifest, source, True))
            elif kinds == {'test'}:
                expected.add((manifest, source, True))
            elif kinds != {'custom-build'}:
                raise ValueError(f'unsupported self target kinds: {sorted(kinds)}')
    return expected


def validate_coverage(records, expected):
    actual = set()
    for record in records:
        if record['compile_exit_code'] or record['cfg_exit_code'] or not record['cfg']:
            raise ValueError('failed compile or empty/failed compiler cfg probe')
        key = tuple(record['unit'])
        if key in actual:
            raise ValueError('duplicate compile context requires explicit disambiguation')
        actual.add(key)
        if ('test' in record['cfg']) != key[2]:
            raise ValueError('compiler cfg disagrees with test compilation mode')
    if actual != expected or not expected:
        raise ValueError(f'missing or unexpected contexts: {sorted(expected - actual)}, {sorted(actual - expected)}')


def checked_cargo(command, environment, stdout, stderr):
    with subprocess.Popen(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, start_new_session=True) as process:
        try:
            return process.wait(timeout=600)
        except BaseException:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            raise


def run(output):
    if os.name != 'posix':
        raise ValueError('this experimental launcher currently supports POSIX only')
    if any(os.environ.get(key) for key in WRAPPERS):
        raise ValueError('existing compiler wrappers are not supported by this self experiment')
    compiler = shutil.which('rustc')
    if compiler is None:
        raise ValueError('rustc unavailable; use the pinned repository dev shell')
    output = output.resolve()
    if not output.is_relative_to((ROOT / 'target').resolve()):
        raise ValueError('self experiment output must be under target/')
    output.mkdir(parents=True, exist_ok=False)
    captures = output / 'invocations'
    captures.mkdir()
    before = source_state(ROOT)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--offline', '--no-deps', '--format-version=1'], cwd=ROOT))
    expected = expected_units(metadata)
    # Cargo invokes wrappers directly; this launcher keeps the active Python.
    launcher = output / 'rustc-wrapper'
    launcher.write_text(f'#!{sys.executable}\nimport runpy, sys\nsys.path.insert(0, {str(ROOT / "acceptance")!r})\nrunpy.run_path({str(Path(__file__).resolve())!r}, run_name="__main__")\n')
    launcher.chmod(0o700)
    environment = os.environ | {CAPTURE_ENV: str(captures), 'RQLENS_SELF_RUSTC': compiler,
                               'RUSTC_WRAPPER': str(launcher), 'CARGO_TARGET_DIR': str(output / 'build')}
    command = ['cargo', 'check', '--workspace', '--all-targets', '--locked', '--offline', '--message-format=json']
    with (output / 'cargo.jsonl').open('w') as stdout, (output / 'cargo.log').open('w') as stderr:
        exit_code = checked_cargo(command, environment, stdout, stderr)
    records = [json.loads(path.read_text()) for path in captures.glob('*.json')]
    after = source_state(ROOT)
    result = {'scope': 'self-only compiler cfg observation experiment; no ownership or metric completeness',
              'command': command, 'exit_code': exit_code, 'before': before, 'after': after,
              'compiler_version': subprocess.check_output([compiler, '-vV'], text=True),
              'script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'records': sorted(records, key=lambda row: row['unit']), 'validated': False}
    try:
        if exit_code or before != after:
            raise ValueError('compilation failed or project inputs changed')
        validate_coverage(records, expected)
        result['validated'] = True
    finally:
        (output / 'profiles.json').write_text(json.dumps(result, indent=2) + '\n')
    return 0


if __name__ == '__main__':
    if CAPTURE_ENV in os.environ:
        raise SystemExit(wrapper(sys.argv[1:]))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    raise SystemExit(run(parser.parse_args().output))
