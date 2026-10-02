import copy
import unittest

from check_trust_t3 import OBSERVED, UNKNOWN, TOOLS, ALL_TOOLS, validate_semantics, validate_types, validated_tools


class AbstentionChecks(unittest.TestCase):
    def fixture(self):
        confidence = {'complete': False, 'partial': True, 'unsupported_pattern': ['unexpanded macro']}
        row = {'measurement_confidence': copy.deepcopy(confidence), **dict.fromkeys(UNKNOWN), **dict.fromkeys(OBSERVED, 0)}
        return {'measurement_confidence': confidence, 'capabilities': [{'capability': 'project-inputs', 'status': 'partial'}], 'records': [row]}

    def test_unknown_totals_cannot_be_zero_or_absent(self):
        document = self.fixture()
        validate_types(document)
        document['records'][0]['method_count'] = 0
        with self.assertRaises(ValueError):
            validate_types(document)
        del document['records'][0]['method_count']
        with self.assertRaises(KeyError):
            validate_types(document)

    def test_optional_tools_cannot_replace_or_duplicate_required_runs(self):
        def commands(tools):
            return [{'command': ['rqlens', 'measure', tool], 'exit_code': 0} for tool in tools for _ in range(2)]
        self.assertEqual(validated_tools(commands(TOOLS)), TOOLS)
        full = commands(ALL_TOOLS)
        self.assertEqual(validated_tools(full), ALL_TOOLS)
        for invalid in (full[2:], full[:-1], full + full[:1], commands(['source-expansion'])):
            with self.assertRaises(ValueError):
                validated_tools(invalid)
        for field, value in [('exit_code', 1), ('command', ['rqlens', 'review', 'hotspots']), ('command', ['rqlens', 'measure', 'unknown'])]:
            invalid = copy.deepcopy(full)
            invalid[0][field] = value
            with self.assertRaises(ValueError):
                validated_tools(invalid)

    def test_capability_cannot_claim_completion(self):
        document = self.fixture()
        document['capabilities'][0]['status'] = 'completed'
        with self.assertRaises(ValueError):
            validate_types(document)

    def test_semantic_status_reaches_payload_and_envelope(self):
        document = self.fixture()
        confidence = document['measurement_confidence']
        confidence.update(missing_input=['unavailable semantics'], semantic_identity={'available': False, 'reference_count': 1, 'complete': False})
        document['data'] = {'measurement_confidence': copy.deepcopy(confidence)}
        validate_semantics(document)
        document['data']['measurement_confidence'].update(complete=True, partial=False)
        with self.assertRaises(ValueError):
            validate_semantics(document)
        confidence['semantic_identity']['complete'] = True
        with self.assertRaises(ValueError):
            validate_semantics(document)


if __name__ == '__main__':
    unittest.main()
