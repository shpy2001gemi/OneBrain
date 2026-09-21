import copy
import unittest
from .assess_development_draft import assess


class DevelopmentAssessmentTests(unittest.TestCase):
    def test_reciprocal_contrast_is_same_meaning_but_wrong_pair_is_not(self):
        def claim(subject):
            return dict(subject=subject,predicate='runs',arguments=[],frequency=[],negation=[],condition=[],time=[],location=[],modality=[],approximation=[],numbers=[],relations=[])
        claims=[claim('A'),claim('B'),claim('C')]
        wanted=copy.deepcopy(claims)
        wanted[1]['relations']=[{'subject':'A','kind':'contrast','quote':'but'}]
        claims[0]['relations']=[{'statement':1,'kind':'contrast','quote':'but'}]
        claims[1]['relations']=[{'statement':0,'kind':'contrast','quote':'but'}]
        def record():
            return {'case':{'id':'test'},'model':'test','seconds':1,'job':{'state':'draft_ready','windows':[{'revisions':[{'draft':{'statements':claims,'unresolved':[]},'validation':{'issues':[],'uncovered':[]}}]}]}}
        self.assertTrue(assess(record(),{'claims':wanted})['semantic_checks_pass'])
        claims[0]['relations'][0]['statement']=2
        self.assertFalse(assess(record(),{'claims':wanted})['semantic_checks_pass'])
        claims[0]['relations'][0]['statement']=1
        claims[1]['negation']=['not']
        self.assertFalse(assess(record(),{'claims':wanted})['semantic_checks_pass'])


if __name__=='__main__':
    unittest.main()
