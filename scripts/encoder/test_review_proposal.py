import copy
import json
import unittest
from .probe_review_proposal import ROOT, anchors, check_proposal, parse_proposal

class ReviewProposalTests(unittest.TestCase):
    def setUp(self):
        self.source='Đèn thường sáng.'
        self.schema=json.loads((ROOT/'review_proposal.schema.json').read_text(encoding='utf-8'))
        self.proposal={'statements':[{'subject':'Đèn','predicate':'sáng','arguments':[],'qualifiers':[{'kind':'frequency','quote':'thường'}],'numbers':[],'relations':[],'evidence':self.source}],'unresolved':[]}
    def test_utf8_anchors_and_repeated_mentions(self):
        self.assertEqual(anchors('Đèn Đèn','Đèn'),[{'start':0,'end':5},{'start':6,'end':11}])
        errors, matched=check_proposal(self.source,self.proposal,self.schema)
        self.assertEqual(errors,[])
        self.assertTrue(all(len(m['matches'])==1 for m in matched))
    def test_schema_and_fabricated_source_are_rejected(self):
        bad=copy.deepcopy(self.proposal)
        bad['verified']=True
        self.assertTrue(check_proposal(self.source,bad,self.schema)[0])
        bad=copy.deepcopy(self.proposal)
        bad['statements'][0]['subject']='invented'
        self.assertTrue(any('absent' in e for e in check_proposal(self.source,bad,self.schema)[0]))
    def test_self_references_and_omitted_evidence_fail(self):
        bad=copy.deepcopy(self.proposal)
        bad['statements'][0]['relations']=[{'statement':0,'kind':'cause','quote':'sáng'}]
        self.assertTrue(any('invalid target' in e for e in check_proposal(self.source,bad,self.schema)[0]))
        self.assertTrue(any('coverage' in e for e in check_proposal(self.source+' Quạt chạy.',self.proposal,self.schema)[0]))
    def test_mechanical_grounding_is_not_semantic_verification(self):
        bad=copy.deepcopy(self.proposal)
        bad['statements'][0]['qualifiers']=[]
        # Full evidence is insufficient to establish qualifier fidelity: all
        # mechanically valid reports still require independent semantic review.
        self.assertEqual(check_proposal(self.source,bad,self.schema)[0],[])
    def test_explicit_frequency_is_not_duplicated_inside_predicate(self):
        bad=copy.deepcopy(self.proposal)
        bad['statements'][0]['predicate']='thường sáng'
        self.assertTrue(any('duplicated' in e for e in check_proposal(self.source,bad,self.schema)[0]))
    def test_parser_rejects_duplicate_nonfinite_and_deep_outputs(self):
        for raw in ['{"statements":[],"statements":[]}', '{"x":NaN}', '['*33+'0'+']'*33]:
            with self.assertRaises(ValueError): parse_proposal(raw)

if __name__=='__main__': unittest.main()
