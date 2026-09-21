"""Build the closed private selection-v2 schemas; never a canonical KU schema."""
import copy
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / 'docs/specs/vnext/ku-semantic-selection-v2'


def obj(properties, required):
    return dict(type='object', properties=properties, required=required, additionalProperties=False)


def arr(items, maximum=16, minimum=0):
    return dict(type='array', items=items, minItems=minimum, maxItems=maximum)


def string(maximum=2048):
    return dict(type='string', minLength=1, maxLength=maximum)


def enum(*values):
    return dict(type='string', enum=list(values))


def schemas():
    selection = json.loads((DEST.parent / 'ku-semantic-selection-v1/selection.schema.json').read_text())
    claim = selection['properties']['claims']['items']
    selector = obj({'quote': string(), 'within': string(8192)}, ['quote'])
    branch = obj({'surface': selector, 'borrowed': selector}, ['surface'])
    claim['properties'].update({
        'alternatives': arr(obj({
            'quote': string(), 'cue': string(128),
            'branches': arr(branch, minimum=2),
            'exclusivity': enum('unspecified', 'exclusive', 'inclusive'),
        }, ['quote', 'cue', 'branches', 'exclusivity'])),
        'references': arr(obj({
            'via': selector, 'to': selector,
            'target_kind': enum('term', 'group', 'claim'),
        }, ['via', 'to', 'target_kind'])),
        'comparisons': arr(obj({
            'property': string(), 'cue': string(128),
            'target_kind': enum('explicit', 'implicit_candidate', 'unspecified'),
            'target': selector,
        }, ['property', 'cue', 'target_kind'])),
        'ellipses': arr(obj({
            'surface': selector, 'borrowed': selector,
        }, ['surface', 'borrowed'])),
    })
    repair = obj({'patch': obj(copy.deepcopy(claim['properties']), []),
                  'unresolved': copy.deepcopy(selection['properties']['unresolved'])}, ['patch'])
    draft = json.loads((DEST.parent / 'ku-review-draft-v1/draft.schema.json').read_text())
    dc = draft['properties']['statements']['items']
    for key in ['alternatives', 'ellipses', 'references', 'comparisons']:
        dc['properties'][key] = copy.deepcopy(claim['properties'][key])
        dc['required'].append(key)
    offset = dict(type='integer', minimum=0, maximum=8192)
    pair = arr(offset, maximum=2, minimum=2)
    dc['properties']['id'] = string(64)
    dc['properties']['evidence_spans'] = arr(obj({
        'id': string(64), 'role': string(32), 'quote': string(8192), 'start': offset, 'end': offset,
    }, ['id', 'role', 'quote', 'start', 'end']), maximum=130)
    dc['required'] += ['id', 'evidence_spans']
    group = dc['properties']['alternatives']['items']['properties']
    group.update(id=string(64), operator=enum('or'), origin=enum('explicit'), span=pair, cue_span=pair)
    group['branches']['items']['properties'].update(
        id=string(64), origin=enum('explicit', 'reconstructed'), span=pair, borrowed_span=pair)
    dc['properties']['ellipses']['items']['properties'].update(
        origin=enum('reconstructed'), span=pair, borrowed_span=pair)
    dc['properties']['references']['items']['properties'].update(
        target_id=string(64), status=enum('proposed'), via_span=pair, target_span=pair)
    dc['properties']['comparisons']['items']['properties'].update(
        origin=enum('explicit', 'inferred'), target_span=pair, cue_span=pair)
    draft['properties'].update(profile=enum('ku-semantic-draft/2.0'),
                               semantic_nodes=dict(type='integer', minimum=0, maximum=256))
    draft['required'] += ['profile', 'semantic_nodes']
    return {'selection.schema.json': selection, 'repair.schema.json': repair,
            'draft.schema.json': draft}


if __name__ == '__main__':
    DEST.mkdir(exist_ok=True)
    for name, value in schemas().items():
        (DEST / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
