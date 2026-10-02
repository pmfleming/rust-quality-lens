import unittest

import trust_campaign as campaign


class TrustComparison(unittest.TestCase):
    def test_expensive_expansion_is_opt_in(self):
        self.assertNotIn('source-expansion', campaign.TOOLS)
        self.assertEqual(campaign.ALL_TOOLS['source-expansion'], 'source_expansion.json')

    def test_only_declared_timestamps_are_ignored(self):
        a = {'generated_at': 'before', 'records': [{'measured_at': 'before', 'value': 1}], 'complete': True}
        b = {'generated_at': 'after', 'records': [{'measured_at': 'after', 'value': 1}], 'complete': True}
        self.assertEqual(campaign.normalize(a), campaign.normalize(b))
        for key, value in [('complete', False), ('duration_ms', 2), ('cache_hit', True)]:
            changed = {**b, key: value}
            self.assertNotEqual(campaign.normalize(a), campaign.normalize(changed))

    def test_order_missing_results_and_semantic_changes_are_not_hidden(self):
        a = {'edges': [{'target': 'a'}, {'target': 'b'}]}
        b = {'edges': list(reversed(a['edges']))}
        self.assertTrue(campaign.differences(campaign.normalize(a), campaign.normalize(b)))
        self.assertTrue(campaign.differences(a, {}))
        self.assertTrue(campaign.differences({'value': None}, {'value': 0}))
        self.assertTrue(campaign.differences({'value': True}, {'value': 1}))
