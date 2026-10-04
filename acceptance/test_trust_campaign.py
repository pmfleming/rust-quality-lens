import copy
import unittest

import trust_campaign as campaign
from self_refactor_campaign import normalized


class TrustComparison(unittest.TestCase):
    def test_expensive_expansion_is_opt_in(self):
        self.assertNotIn('source-expansion', campaign.TOOLS)
        self.assertEqual(campaign.ALL_TOOLS['source-expansion'], 'source_expansion.json')

    def test_comparison_ignores_only_declared_timestamps_without_mutation(self):
        a = {'generated_at': 'before', 'records': [{'measured_at': 'before', 'value': 1}], 'complete': True}
        b = {'generated_at': 'after', 'records': [{'measured_at': 'after', 'value': 1}], 'complete': True}
        for normalize in [campaign.normalize, normalized]:
            with self.subTest(normalizer=normalize.__module__):
                saved = copy.deepcopy(a)
                self.assertEqual(normalize(a), normalize(b))
                for key, value in [('complete', False), ('duration_ms', 2), ('cache_hit', True)]:
                    self.assertNotEqual(normalize(a), normalize({**b, key: value}))
                edges = {'edges': [{'target': 'a', 'value': None}, {'target': 'b', 'scope': 'unknown'}]}
                self.assertEqual(normalize(edges), edges)
                self.assertNotEqual(normalize(edges), normalize({'edges': list(reversed(edges['edges']))}))
                self.assertTrue(campaign.differences(normalize(edges), {}))
                self.assertTrue(campaign.differences(normalize({'value': None}), normalize({'value': 0})))
                self.assertTrue(campaign.differences(normalize({'value': True}), normalize({'value': 1})))
                self.assertEqual(a, saved)
