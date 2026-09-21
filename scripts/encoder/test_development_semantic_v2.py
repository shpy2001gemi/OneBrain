import copy
import unittest
from scripts.encoder.assess_development_semantic_v2 import assess


def record():
    claim = dict(subject='Máy', predicate='dùng', arguments=['pin hoặc điện'],
                 negation=[], frequency=[], time=[], alternatives=[{
                     'quote': 'pin hoặc điện', 'cue': 'hoặc', 'exclusivity': 'unspecified',
                     'branches': [{'surface': {'quote': 'pin'}}, {'surface': {'quote': 'điện'}}]}])
    return {'case': {'id': 'vi-alternative'}, 'seconds': 1,
            'job': {'state': 'draft_extracted', 'windows': [{'revisions': [{
                'draft': {'statements': [claim], 'unresolved': []},
                'validation': {'issues': [], 'uncovered': []}}]}]}}


class AssessorTests(unittest.TestCase):
    def test_structured_alternative(self):
        self.assertTrue(assess(record())['semantic_checks_pass'])

    def test_mechanical_success_does_not_hide_flattening_or_borrowed_predicate(self):
        for mutation in ('flatten', 'borrow'):
            r = record()
            c = r['job']['windows'][0]['revisions'][0]['draft']['statements'][0]
            if mutation == 'flatten':
                c['alternatives'] = []
            else:
                c['alternatives'][0]['branches'][1]['borrowed'] = {'quote': 'dùng'}
            self.assertFalse(assess(r)['semantic_checks_pass'])

    def test_grounding_failure_is_not_faithful_abstention(self):
        r = record()
        r['job']['windows'][0]['revisions'][0]['validation']['issues'] = ['absent_quote']
        self.assertFalse(assess(r)['semantic_checks_pass'])

    def test_host_state_alone_cannot_change_semantic_verdict(self):
        r = record()
        s = copy.deepcopy(r)
        s['job']['state'] = 'needs_review'
        self.assertEqual(assess(r)['semantic_checks_pass'], assess(s)['semantic_checks_pass'])


if __name__ == '__main__':
    unittest.main()
