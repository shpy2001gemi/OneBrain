import copy
import unittest

from .probe_staged_draft import apply_edits, validate


def claim(source='Đèn thường sáng.', **changes):
    result = {'subject': 'Đèn', 'predicate': 'sáng', 'arguments': [],
              'frequency': ['thường'], 'negation': [], 'condition': [],
              'time': [], 'location': [], 'modality': [], 'approximation': [],
              'numbers': [], 'relations': [], 'evidence': source}
    result.update(changes)
    return {'statements': [result], 'unresolved': []}


class StagedDraftTests(unittest.TestCase):
    def test_evidence_alone_cannot_hide_an_omitted_role(self):
        source = 'Đèn thường sáng.'
        self.assertEqual(validate(source, claim()), ([], []))
        errors, missing = validate(source, claim(frequency=[]))
        self.assertEqual(errors, [])
        self.assertEqual(missing, ['thường'])

    def test_missing_proposition_is_visible_despite_whole_source_evidence(self):
        source = 'Đèn thường sáng. Quạt chạy.'
        errors, missing = validate(source, claim(source))
        self.assertEqual(errors, [])
        self.assertEqual(missing, ['. Quạt chạy.'])

    def test_grounded_field_repair_keeps_other_fields_exact(self):
        source = 'Đèn thường sáng.'
        draft = claim(predicate='thường sáng', frequency=[])
        review = {'edits': [
            {'statement': 0, 'field': 'predicate', 'before': 'thường sáng', 'after': 'sáng'},
            {'statement': 0, 'field': 'frequency', 'before': '', 'after': 'thường'}
        ], 'missing': [], 'unresolved': []}
        result = apply_edits(source, draft, review)
        self.assertEqual(result, claim())
        self.assertEqual(draft['statements'][0]['predicate'], 'thường sáng')

    def test_stale_unknown_invented_or_meaning_deleting_edits_are_rejected(self):
        source = 'Đèn thường sáng.'
        base = {'edits': [{'statement': 0, 'field': 'frequency',
                           'before': 'thường', 'after': ''}],
                'missing': [], 'unresolved': []}
        mutations = [{}, {'field': 'save'}, {'before': 'luôn'},
                     {'after': 'luôn'}, {'statement': 4}]
        for mutation in mutations:
            review = copy.deepcopy(base)
            review['edits'][0].update(mutation)
            with self.assertRaises(ValueError):
                apply_edits(source, claim(), review)

    def test_review_source_is_checked_even_when_no_edit_is_proposed(self):
        with self.assertRaises(ValueError):
            apply_edits('Đèn thường sáng.', claim(),
                        {'edits': [], 'missing': ['invented'], 'unresolved': []})

    def test_checked_noop_preserves_draft_and_partial_coverage_repair_is_allowed(self):
        source = 'Đèn thường sáng.'
        review = {'edits': [{'statement': 0, 'field': 'frequency',
                             'before': 'thường', 'after': 'thường'}],
                  'missing': [], 'unresolved': []}
        self.assertEqual(apply_edits(source, claim(), review), claim())
        source = 'Đèn thường sáng tại kho.'
        draft = claim(source, frequency=[], location=[])
        review['edits'][0]['before'] = ''
        fixed = apply_edits(source, draft, review)
        self.assertEqual(validate(source, fixed)[1], ['tại kho.'])


if __name__ == '__main__':
    unittest.main()
