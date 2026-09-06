"""Independent finite CBOR/BLAKE3 goldens for the approved KU semantic profile."""
from pathlib import Path
import copy
import json
import blake3

ROOT = Path(__file__).resolve().parents[2]


def cbor(v):
    def head(m, n):
        if n < 24:
            return bytes([m * 32 + n])
        for width, marker in [(1, 24), (2, 25), (4, 26), (8, 27)]:
            if n < 1 << (width * 8):
                return bytes([m * 32 + marker]) + n.to_bytes(width, 'big')
        raise ValueError('integer overflow')
    if isinstance(v, bool):
        return b'\xf5' if v else b'\xf4'
    if isinstance(v, int):
        return head(0, v) if v >= 0 else head(1, -v - 1)
    if isinstance(v, bytes):
        return head(2, len(v)) + v
    if isinstance(v, str):
        b = v.encode('utf-8'); return head(3, len(b)) + b
    if isinstance(v, list):
        return head(4, len(v)) + b''.join(map(cbor, v))
    if isinstance(v, dict):
        rows = sorted([(cbor(k), cbor(val)) for k, val in v.items()], key=lambda row: (len(row[0]), row[0]))
        return head(5, len(rows)) + b''.join(k + val for k, val in rows)
    raise ValueError('unsupported value')


def vectors():
    ccid = bytes([7])*16
    base = {0:1, 1:0, 2:[
        {0:0, 1:ccid, 2:[{0:1,1:0,2:ccid},{0:0,1:ccid},{0:2,1:{0:1,1:'water'}}], 3:[], 4:{0:False,1:0}},
        {0:1, 1:ccid, 2:[{0:3,1:0}], 3:[], 4:{0:False,1:0}},
    ]}
    cases = {'base':base}
    negated=copy.deepcopy(base); negated[2][0][4][0]=True; cases['negated']=negated
    reordered=copy.deepcopy(base); reordered[2][0][2].reverse(); cases['argument_order']=reordered
    ordered=copy.deepcopy(base); ordered[2].reverse(); ordered[2][0][0]=0; ordered[2][0][2][0][1]=1; ordered[2][1][0]=1; cases['statement_order']=ordered
    for unit in (8,9):
        quantity=copy.deepcopy(base)
        ratio = lambda n, d: {0:n.to_bytes(8, 'big', signed=True), 1:d.to_bytes(8, 'big')}
        quantity[2][0][2].append({0:2,1:{0:2,1:{0:ratio(1,2),1:{0:bytes([unit])*16,1:[0]*7,2:ratio(1,1),3:ratio(0,1)}}}})
        cases[f'unit_{unit}']=quantity
    rows=[]
    for name, root in cases.items():
        raw=cbor(root)
        digest=lambda domain: blake3.blake3(domain+raw).hexdigest()
        rows.append({'name':name,'canonical_hex':raw.hex(),'semantic_cid':digest(b'onebrain:vnext:semantic-content:1\0'),
                     'other_profile_cid':digest(b'onebrain:vnext:semantic-content:2\0'),
                     'object_domain_same_root':digest(b'onebrain:vnext:object:1\0')})
    return {'format':'onebrain/ku-semantic-content-golden/1','profile':'ku-semantic-content/1.0','producer':'independent finite Python CBOR encoder + BLAKE3','vectors':rows}


if __name__ == '__main__':
    path=ROOT/'src/test-vectors/vnext/ku-semantic-content-v1.json'
    path.write_text(json.dumps(vectors(),indent=2)+'\n',encoding='utf-8')
    print(f'Wrote {path.relative_to(ROOT)}')
