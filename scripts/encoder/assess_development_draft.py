"""Post-run checks for five public development cases; never part of model input.

These are deliberately narrow development oracles, not blind model qualification.
They inspect meaning-bearing fields separately from the host's draft state.
"""
import argparse
import json
import sys
from pathlib import Path


def assess(record, expected):
    job = record['job']
    issues, claims = [], []
    for window in job['windows']:
        if not window['revisions']:
            issues.append('no draft for window')
            continue
        latest = window['revisions'][-1]
        claims.extend(latest['draft']['statements'])
        issues.extend(latest['validation']['issues'])
        issues.extend('uncovered: '+q for q in latest['validation']['uncovered'])
        if latest['draft']['unresolved']:
            issues.append('draft has unresolved content')
    if len(claims) != len(expected['claims']):
        issues.append('claim count differs')
    for wanted in expected['claims']:
        matches = [c for c in claims if c['subject'] == wanted['subject']]
        if len(matches) != 1:
            issues.append(f"subject {wanted['subject']}: missing or duplicated")
            continue
        actual = matches[0]
        fields = {field: [] for field in ('arguments','frequency','negation','condition','time','location','modality','approximation','numbers','relations')}
        fields.update(wanted)
        for field, value in fields.items():
            if field == 'relations':
                relations = []
                for rel in actual[field]:
                    if rel['kind'] == 'contrast':
                        continue
                    index = rel['statement']
                    relations.append({'subject':claims[index]['subject'] if 0 <= index < len(claims) else None,
                                      'kind':rel['kind'],'quote':rel['quote']})
                matched = relations == [r for r in value if r['kind'] != 'contrast']
            else:
                matched = actual[field] == value
            if not matched:
                issues.append(f"{wanted['subject']}.{field}: expected development meaning differs")
    # Contrast is symmetric. Two reciprocal links assert the same contrast,
    # unlike cause/condition whose direction must remain exact above.
    def contrasts(rows, indexed):
        edges = set()
        for row in rows:
            for rel in row['relations']:
                if rel['kind'] != 'contrast':
                    continue
                target = rel['subject'] if not indexed else (rows[rel['statement']]['subject'] if 0 <= rel['statement'] < len(rows) else '<invalid>')
                edges.add((tuple(sorted((row['subject'],target))),rel['quote']))
        return edges
    if contrasts(claims,True) != contrasts(expected['claims'],False):
        issues.append('contrast connects different claims or loses source connective')
    return {'case':record['case']['id'],'model':record['model'],
            'seconds':round(record['seconds'],2),'host_state':job['state'],
            'semantic_checks_pass':not issues,'findings':issues,
            'canonical_ku':False,'qualification':False}


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report',type=Path)
    args=parser.parse_args()
    expectations=json.loads(Path(__file__).with_name('development_semantic_expectations.json').read_text(encoding='utf-8'))
    by_id={case['id']:case for case in expectations}
    for line in args.report.read_text(encoding='utf-8').splitlines():
        record=json.loads(line)
        print(json.dumps(assess(record,by_id[record['case']['id']]),ensure_ascii=False))


if __name__ == '__main__':
    main()
