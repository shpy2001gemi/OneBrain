"""Narrow post-run development checks. Never imported by provider/model code.

Faithful abstention is permitted for implicit targets and ambiguous references.
These checks are not independent factual verification or blind qualification.
"""
import argparse
import json
import sys
from pathlib import Path


def assess(record):
    case = record['case']['id']
    claims, unresolved, mechanical = [], [], []
    for window in record['job']['windows']:
        if not window['revisions']:
            return dict(case=case, semantic_checks_pass=False, findings=['no retained draft'])
        revision = window['revisions'][-1]
        claims.extend(revision['draft']['statements'])
        unresolved.extend(revision['draft']['unresolved'])
        mechanical.extend(revision['validation']['issues'])
        mechanical.extend('uncovered: ' + q for q in revision['validation']['uncovered'])
    failures = []

    def check(ok, message):
        if not ok:
            failures.append(message)

    def claim(subject, predicate):
        matches = [c for c in claims if c['subject'] == subject and c['predicate'] == predicate]
        check(len(matches) == 1, f'claim roles: {subject} / {predicate}')
        return matches[0] if len(matches) == 1 else {}

    def alternative(c, quote, surfaces, borrowed=None):
        matches = [g for g in c.get('alternatives', []) if g['quote'] == quote]
        check(len(matches) == 1, 'one structured alternative group')
        if matches:
            g = matches[0]
            check([b['surface']['quote'] for b in g['branches']] == surfaces, 'alternative branches')
            check(g['exclusivity'] == 'unspecified', 'do not infer XOR')
            if borrowed:
                check(g['branches'][-1].get('borrowed', {}).get('quote') == borrowed, 'ellipsis retains borrowed source')
                check(g['branches'][-1].get('origin') == 'reconstructed', 'ellipsis provenance')
                check(all(not b.get('borrowed') for b in g['branches'][:-1]), 'no extra borrowed content')
            else:
                check(all(not b.get('borrowed') for b in g['branches']), 'complete branches must not borrow a predicate')

    if case == 'vi-rocket-or-reference':
        check(len(claims) == 2, 'two claims')
        a = claim('Tên lửa', 'sử dụng')
        b = claim('nhiên liệu rắn', 'an toàn')
        check(a.get('modality') == ['có thể'], 'possibility scope')
        check(b.get('time') == ['sẽ'], 'future scope')
        alternative(a, 'nhiên liệu lỏng hoặc rắn', ['nhiên liệu lỏng', 'rắn'], 'nhiên liệu')
        refs = b.get('references', [])
        check(any(r['via']['quote'] == 'Trong đó' and r['target_kind'] == 'group' and r.get('target_id') for r in refs), 'reference to alternative group')
        comparisons = b.get('comparisons', [])
        check(len(comparisons) == 1, 'comparison retained')
        if comparisons:
            c = comparisons[0]
            check(c['cue'] == 'hơn' and c['property'] == 'an toàn', 'comparison property/cue')
            check(c['target_kind'] in ('unspecified', 'implicit_candidate'), 'comparison target not explicit')
            if c['target_kind'] == 'implicit_candidate':
                check(c.get('target', {}).get('quote') == 'nhiên liệu lỏng' and c.get('origin') == 'inferred', 'marked candidate target')
        check(all(not c.get('numbers') and not c.get('relations') for c in claims), 'no quantities or causal assertion')
        check(not a.get('frequency') and not b.get('frequency'), 'no spurious frequency')
    elif case in ('vi-alternative', 'vi-negated-alternative'):
        check(len(claims) == 1, 'one claim')
        c = claim('Máy', 'dùng')
        alternative(c, 'pin hoặc điện', ['pin', 'điện'])
        check(c.get('negation') == (['không'] if case == 'vi-negated-alternative' else []), 'negation scope')
        check(not c.get('time') and not c.get('frequency'), 'no spurious qualifiers')
    elif case == 'vi-comparison-explicit':
        check(len(claims) == 1, 'one claim')
        c = claim('Xe đỏ', 'nhanh')
        comparisons = c.get('comparisons', [])
        check(len(comparisons) == 1, 'comparison retained')
        if comparisons:
            r = comparisons[0]
            check(r['cue'] == 'hơn' and r['target_kind'] == 'explicit' and r.get('target', {}).get('quote') == 'xe xanh', 'explicit comparison target')
        check(not c.get('time') and not c.get('numbers'), 'no invented time/quantity')
    elif case == 'vi-number-word':
        c = claim('Xe', 'có')
        check(len(claims) == 1 and c.get('arguments') == ['bốn bánh'], 'number words retained')
        check(any('bốn' in u['quote'] for u in unresolved) or
              'numeric: possible_number_word_unassessed' in mechanical,
              'unsupported number words explicitly require review')
        check(not c.get('numbers'), 'no fabricated numeric quote')
    elif case == 'vi-ambiguous-reference':
        claim('Lan', 'gặp')
        check(any('Cô ấy' in u['quote'] for u in unresolved), 'ambiguous pronoun unresolved')
        check(not any(c['subject'] in ('Lan', 'Mai') and c['predicate'] == 'cười' for c in claims), 'do not resolve ambiguous antecedent')
        check(any(c['predicate'] == 'cười' for c in claims) or any('cười' in u['quote'] for u in unresolved), 'second claim retained or explicitly unresolved')
    elif case == 'en-condition-alternative':
        check(len(claims) == 1, 'condition not asserted independently')
        c = claim('the team', 'use')
        check(c.get('condition') == ['If it rains'] and c.get('modality') == ['may'], 'conditional possibility scope')
        alternative(c, 'a bus or a train', ['a bus', 'a train'])
    elif case == 'vi-and':
        c = claim('Hộp', 'chứa')
        check(len(claims) == 1 and c.get('arguments') == ['bi xanh và bi đỏ'], 'conjunction retained')
        check(not c.get('alternatives') and not c.get('time'), 'AND not OR or time')
    else:
        raise ValueError('unknown development case')
    # Grounding/resource errors cannot be excused as faithful abstention.
    allowed = ('comparisons: target_unspecified', 'comparisons: implicit_target_unassessed',
               'semantic: unresolved', 'subject: unresolved', 'numeric: possible_number_word_unassessed')
    check(all(any(a in error for a in allowed) for error in mechanical if not error.startswith('uncovered: ')), 'grounding/structure errors')
    for error in mechanical:
        if error.startswith('uncovered: '):
            quote = error.removeprefix('uncovered: ')
            check(any(quote in u['quote'] for u in unresolved), 'unexplained uncovered source')
    return dict(case=case, host_state=record['job']['state'], seconds=round(record['seconds'], 2),
                semantic_checks_pass=not failures, findings=failures,
                mechanical_findings=mechanical, qualification=False, factual_verification='unassessed')


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    args = parser.parse_args()
    for line in args.report.read_text(encoding='utf-8').splitlines():
        print(json.dumps(assess(json.loads(line)), ensure_ascii=False))
