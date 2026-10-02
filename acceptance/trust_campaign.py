#!/usr/bin/env python3
"""Repeat focused static analysis against local daemons without editing them.

Run after cargo build --locked --bin rqlens. Output must be a new directory.
No array sorting or finding/status/telemetry removal is allowed in comparisons.
Project commands are limited to the configured semantic profile and helper builds;
this campaign does not run daemon tests, coverage, benchmarks or daemon services.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PROJECTS = ['app-daemon', 'bar-daemon', 'bt-daemon', 'clip-daemon', 'nm-daemon', 'update-daemon', 'daemon-framework']
TOOLS = {
    'hotspots': 'hotspots.json', 'clones': 'clones.json',
    'escape-hatches': 'rust_escape_hatches.json', 'type-health': 'type_health.json',
    'reliability': 'reliability_findings.json', 'test-quality': 'test_quality.json',
    'api-health': 'api_health.json', 'correctness': 'correctness_review.json',
    'locality': 'locality_metrics.json', 'leverage': 'leverage_metrics.json',
    'module-cohesion': 'module_cohesion.json', 'architecture-rules': 'architecture_rules.json',
    'map': 'map.json',
}
OPTIONAL_TOOLS = {'source-expansion': 'source_expansion.json'}
ALL_TOOLS = TOOLS | OPTIONAL_TOOLS

spec = importlib.util.spec_from_file_location('daemon_campaign', ROOT / 'scripts/daemon-campaign.py')
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def source_state(root):
    paths = subprocess.check_output(['git', '-C', str(root), 'ls-files', '-z', '--cached', '--others', '--exclude-standard']).split(b'\0')
    digest = hashlib.sha256()
    for raw in sorted(set(paths)):
        if raw:
            path = root / os.fsdecode(raw)
            digest.update(raw + b'\0')
            digest.update(path.read_bytes() if path.is_file() else b'<missing>')
    return {**helper.revision(root), 'files_sha256': digest.hexdigest()}


def normalize(value):
    if isinstance(value, dict):
        return {k: normalize(v) for k, v in value.items() if k not in {'generated_at', 'measured_at'}}
    if isinstance(value, list):
        return [normalize(v) for v in value]
    return value


def differences(a, b, path='$'):
    if type(a) is not type(b):
        return [path]
    if isinstance(a, dict):
        return [p for k in sorted(a.keys() | b.keys()) for p in
                ([path + '.' + k] if k not in a or k not in b else differences(a[k], b[k], path + '.' + k))]
    if isinstance(a, list):
        return ([path + '.length'] if len(a) != len(b) else []) + [p for i, (x, y) in enumerate(zip(a, b)) for p in differences(x, y, f'{path}[{i}]')]
    return [] if a == b else [path]


def command(args, cwd, log, timeout=900):
    start = time.monotonic()
    with log.open('w') as output:
        process = subprocess.Popen(args, cwd=cwd, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            code = None
    return {'command': list(map(str, args)), 'exit_code': code, 'seconds': round(time.monotonic() - start, 3), 'log': str(log)}


def worker(args):
    root = (ROOT.parent / args.inside).resolve()
    dest = args.output / root.name
    dest.mkdir()
    before = source_state(root)
    text = helper.override((root / 'rqlens.toml').read_text(), '', 'project_root', str(root))
    text = helper.override(text, '', 'output_dir', str(dest / 'analysis'))
    manifest = tomllib.loads(text).get('rust', {}).get('helper_manifest')
    if manifest:
        text = helper.override(text, 'rust', 'helper_manifest', str((root / manifest).resolve()))
    config = dest / 'rqlens.toml'
    config.write_text(text)
    commands = []
    for repetition in (1, 2):
        for tool in args.tools:
            commands.append(command([str(args.binary), 'measure', tool, '--config', str(config)], root, dest / f'{repetition}-{tool}.log'))
        shutil.copytree(dest / 'analysis', dest / f'run-{repetition}')
        save(dest / 'commands.json', commands)
    comparisons = {}
    for tool in args.tools:
        name = ALL_TOOLS[tool]
        paths = [dest / f'run-{i}' / name for i in (1, 2)]
        if not all(p.is_file() for p in paths):
            comparisons[name] = {'equal': False, 'reason': 'missing output'}
            continue
        docs = [json.loads(p.read_text()) for p in paths]
        diffs = differences(*[normalize(d) for d in docs])
        comparisons[name] = {'equal': not diffs, 'difference_count': len(diffs), 'first_differences': diffs[:20],
                             'confidence': docs[1].get('measurement_confidence'), 'records': len(docs[1].get('records', []))}
    after = source_state(root)
    passed = before == after and all(c['exit_code'] == 0 for c in commands) and all(c['equal'] for c in comparisons.values())
    save(dest / 'summary.json', {'project': root.name, 'before': before, 'after': after, 'source_stable': before == after,
                                'passed': passed, 'commands': commands, 'comparisons': comparisons})
    return 0 if passed else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/rqlens')
    parser.add_argument('--projects', nargs='+', choices=PROJECTS, default=PROJECTS)
    parser.add_argument('--tools', nargs='+', choices=ALL_TOOLS, default=list(TOOLS))
    parser.add_argument('--inside', choices=PROJECTS, help=argparse.SUPPRESS)
    args = parser.parse_args()
    args.output, args.binary = args.output.resolve(), args.binary.resolve()
    if args.inside:
        return worker(args)
    args.output.mkdir(parents=True, exist_ok=False)
    frozen = args.output / 'rqlens'
    shutil.copy2(args.binary, frozen)
    save(args.output / 'provenance.json', {'lens': source_state(ROOT), 'binary_sha256': hashlib.sha256(frozen.read_bytes()).hexdigest(),
                                         'normalization': ['generated_at', 'measured_at'], 'tools': args.tools})
    statuses = []
    for name in args.projects:
        root = ROOT.parent / name
        before = source_state(root)  # Capture BEFORE Nix setup as well as measurement.
        argv = [sys.executable, str(Path(__file__).resolve()), '--output', str(args.output), '--binary', str(frozen), '--inside', name, '--tools', *args.tools]
        if name != 'update-daemon':
            argv = ['nix', 'develop', '--offline', '--no-write-lock-file', str(root), '--command', *argv]
        result = command(argv, ROOT, args.output / f'{name}.log', timeout=7200)
        result.update(project=name, source_stable_including_setup=before == source_state(root))
        statuses.append(result)
        save(args.output / 'status.json', statuses)
        print(name, result['exit_code'], 'unchanged:', result['source_stable_including_setup'], flush=True)
    return 0 if all(s['exit_code'] == 0 and s['source_stable_including_setup'] for s in statuses) else 1


if __name__ == '__main__':
    sys.exit(main())
