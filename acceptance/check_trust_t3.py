#!/usr/bin/env python3
"""Check abstention and repeatability in the current seven-project syntax profile.

This checks recorded artifacts; it does not certify compiler expansion or replace
native CLI regressions. Future fully supported profiles need new expectations.
"""
import argparse
import hashlib
import json
from pathlib import Path
from collections import Counter

from trust_campaign import PROJECTS, TOOLS, ALL_TOOLS, normalize

UNKNOWN = ('method_count', 'impl_block_count', 'impl_file_count', 'structural_risk', 'structural_score')
OBSERVED = ('observed_method_count', 'observed_impl_block_count', 'observed_impl_file_count')
SEMANTIC = ('locality_metrics.json', 'leverage_metrics.json', 'module_cohesion.json', 'architecture_rules.json', 'map.json')
GENERATED = {'ComputeUsage', 'StorageUsage', 'NetworkUsage', 'EnergyUsage', 'ResourceAvailability'}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def partial(confidence):
    return confidence['complete'] is False and confidence['partial'] is True


def validate_types(document):
    confidence = document['measurement_confidence']
    require(partial(confidence), 'expected an explicitly partial type inventory')
    require(confidence.get('unsupported_pattern') or confidence.get('missing_input'), 'missing abstention reason')
    capabilities = [c for c in document['capabilities'] if c['capability'] == 'project-inputs']
    require(len(capabilities) == 1 and capabilities[0]['status'] == 'partial', 'capability claims complete inputs')
    for row in document['records']:
        require(partial(row['measurement_confidence']), 'row claims complete type inventory')
        require(all(row[field] is None for field in UNKNOWN), 'unknown totals/scores became numeric')
        require(all(type(row[field]) is int and row[field] >= 0 for field in OBSERVED), 'invalid source observations')


def validate_semantics(document):
    confidence = document['measurement_confidence']
    identity = confidence['semantic_identity']
    if not identity['available'] and identity['reference_count']:
        require(identity['complete'] is False, 'unavailable semantics claimed complete resolution')
    if not identity['complete']:
        require(partial(confidence), 'incomplete semantics did not reach the envelope')
        require(confidence.get('missing_input'), 'semantic abstention lost its explanation')
        require(document['capabilities'][0]['status'] == 'partial', 'semantic capability claims completion')
        nested = document.get('data', {}).get('measurement_confidence')
        if nested is not None:
            require(partial(nested), 'payload contradicts partial semantic envelope')


def validated_tools(commands):
    measured = Counter()
    for entry in commands:
        command = entry['command']
        require(entry['exit_code'] == 0 and len(command) >= 3 and command[1] == 'measure', 'missing/failed measurement command')
        measured[command[2]] += 1
    require(set(TOOLS) <= set(measured) <= set(ALL_TOOLS), 'missing required or unknown tools')
    require(all(count == 2 for count in measured.values()), 'each selected tool must run exactly twice')
    return {tool: filename for tool, filename in ALL_TOOLS.items() if tool in measured}


def check(campaign):
    load = lambda path: json.loads(path.read_text())
    provenance = load(campaign / 'provenance.json')
    require(hashlib.sha256((campaign / 'rqlens').read_bytes()).hexdigest() == provenance['binary_sha256'], 'executable hash mismatch')
    status = load(campaign / 'status.json')
    require(len(status) == len(PROJECTS) and {s['project'] for s in status} == set(PROJECTS) and all(s['exit_code'] == 0 and s['source_stable_including_setup'] for s in status), 'campaign failed or modified project sources')
    results = []
    for project in PROJECTS:
        directory = campaign / project
        summary = load(directory / 'summary.json')
        require(summary['passed'] and summary['before'] == summary['after'], f'{project}: failed/changed inputs')
        tools = validated_tools(summary['commands'])
        for filename in tools.values():
            first, second = (load(directory / f'run-{n}' / filename) for n in (1, 2))
            require(normalize(first) == normalize(second), f'{project}: nondeterministic {filename}')
            for document in (first, second):
                if filename == 'type_health.json':
                    validate_types(document)
                if filename in SEMANTIC:
                    validate_semantics(document)
        document = load(directory / 'run-2/type_health.json')
        if project == 'app-daemon':
            reasons = document['measurement_confidence']['unsupported_pattern']
            require(any('unexpanded item macro usage_fields!' in reason for reason in reasons), 'app macro omission lost its explicit reason')
            require(GENERATED.isdisjoint(row['type_name'] for row in document['records']), 'update expectations for newly expanded generated types')
        results.append({'project': project, 'revision': summary['before']['head'], 'source_sha256': summary['before']['files_sha256'], 'source_stable': True, 'commands_passed': len(summary['commands']), 'artifacts_equal': len(tools), 'partial_type_rows': len(document['records']), 'type_abstention_reasons': len(document['measurement_confidence']['unsupported_pattern'])})
    return {'status': 'passed', 'binary_sha256': provenance['binary_sha256'], 'analyzer_source': provenance['lens'], 'results': results}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    print(json.dumps(check(parser.parse_args().campaign), indent=2))
