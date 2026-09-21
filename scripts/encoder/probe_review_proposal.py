"""Development-only proposal experiment. No KU API, Registry binding or save.

Reports intentionally retain user-approved development source and raw proposals.
Use a private output path; do not run with secrets or qualification holdouts.
Requires jsonschema. Uses an already running, explicitly selected loopback Ollama.
"""
import argparse
import hashlib
import json
import re
import time
import urllib.error
import urllib.request
from pathlib import Path
import jsonschema

ROOT = Path(__file__).resolve().parent

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None

def parse_proposal(raw):
    depth, quoted, escaped = 0, False, False
    for c in raw:
        if quoted:
            if escaped: escaped=False
            elif c=='\\': escaped=True
            elif c=='"': quoted=False
        elif c=='"': quoted=True
        elif c in '[{':
            depth+=1
            if depth>32: raise ValueError('proposal depth')
        elif c in ']}': depth-=1
    def pairs(items):
        result={}
        for key,value in items:
            if key in result: raise ValueError('duplicate field')
            result[key]=value
        return result
    def invalid(_): raise ValueError('non-finite value')
    return json.loads(raw,object_pairs_hook=pairs,parse_constant=invalid)

def anchors(source, quote):
    if not quote:
        return []
    raw, needle = source.encode('utf-8'), quote.encode('utf-8')
    result, offset = [], 0
    while (found := raw.find(needle, offset)) >= 0:
        result.append({'start': found, 'end': found + len(needle)})
        offset = found + 1
    return result

def check_proposal(source, proposal, schema):
    errors = [f'schema: {e.json_path}: {e.validator}' for e in jsonschema.Draft202012Validator(schema).iter_errors(proposal)][:8]
    if errors:
        return errors, []
    grounded = []
    def quote(path, value, scope=None):
        matches = anchors(source, value)
        if not matches:
            errors.append(f'grounding: {path}: quote absent from source')
        if scope is not None and value not in scope:
            errors.append(f'grounding: {path}: quote outside proposition evidence')
        grounded.append({'path': path, 'matches': matches, 'ambiguous': len(matches) > 1})
    for i, s in enumerate(proposal['statements']):
        base = f'$.statements[{i}]'
        quote(base+'.evidence', s['evidence'])
        if s['subject']: quote(base+'.subject', s['subject'], s['evidence'])
        elif not proposal['unresolved']: errors.append(base+'.subject: absent subject needs unresolved context')
        quote(base+'.predicate', s['predicate'], s['evidence'])
        for j, value in enumerate(s['arguments']): quote(f'{base}.arguments[{j}]', value, s['evidence'])
        for j, q in enumerate(s['qualifiers']):
            quote(f'{base}.qualifiers[{j}].quote', q['quote'], s['evidence'])
            if q['kind'] in ('frequency','negation') and q['quote'] in s['predicate']:
                errors.append(f'{base}.predicate: frequency/negation qualifier duplicated in predicate; keep the core relation here and qualifier separately')
        for j, n in enumerate(s['numbers']):
            for key, value in n.items():
                if value: quote(f'{base}.numbers[{j}].{key}', value, s['evidence'])
            if not re.fullmatch(r'-?(?:0|[1-9][0-9]*)(?:\.[0-9]+|/[1-9][0-9]*)?', n['value_quote']):
                errors.append(f'{base}.numbers[{j}].value_quote: unsupported numeric notation; retain for review')
        for j, rel in enumerate(s['relations']):
            quote(f'{base}.relations[{j}].quote', rel['quote'])
            if rel['statement'] >= len(proposal['statements']) or rel['statement'] == i:
                errors.append(f'{base}.relations[{j}].statement: invalid target')
    for i, u in enumerate(proposal['unresolved']): quote(f'$.unresolved[{i}].quote', u['quote'])
    # Evidence coverage is mechanical, not proof that every qualifier was modeled.
    intervals = sorted((m['start'], m['end']) for g in grounded if g['path'].endswith('.evidence') or g['path'].startswith('$.unresolved') for m in g['matches'])
    covered = bytearray(len(source.encode('utf-8')))
    for a, b in intervals: covered[a:b] = b'\x01' * (b-a)
    cursor = 0
    for char in source:
        size = len(char.encode('utf-8'))
        if char.isalnum() and not all(covered[cursor:cursor+size]):
            errors.append('coverage: meaningful source text lacks evidence or unresolved entry')
            break
        cursor += size
    return errors[:8], grounded

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--development-inputs', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--port', type=int, required=True)
    parser.add_argument('--model', required=True)
    parser.add_argument('--template', choices=['qwen3-raw', 'ollama-chat'], required=True)
    args = parser.parse_args()
    if not 1024 <= args.port <= 65535: parser.error('invalid loopback port')
    cases = json.loads(args.development_inputs.read_text(encoding='utf-8'))
    if not 1 <= len(cases) <= 16 or any(not 0 < len(c['text'].encode()) <= 8192 for c in cases): parser.error('bounded development cases required')
    schema = json.loads((ROOT/'review_proposal.schema.json').read_text(encoding='utf-8'))
    prompt = (ROOT/'review_proposal_prompt.txt').read_text(encoding='utf-8')
    identity = hashlib.sha256((prompt+json.dumps(schema)).encode()).hexdigest()
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    def metadata(route):
        with opener.open(f'http://127.0.0.1:{args.port}{route}',timeout=5) as r: return json.loads(r.read(1048576))
    version=metadata('/api/version')
    installed=metadata('/api/tags').get('models',[])
    model_record=next((m for m in installed if m.get('name')==args.model),None)
    if model_record is None: parser.error('model is not installed on the selected local server')
    with args.report.open('x', encoding='utf-8') as report:
        for index, case in enumerate(cases):
            calls, errors, grounded = [], [], []
            started = time.monotonic()
            for attempt in range(2):
                content = 'SCHEMA\n'+json.dumps(schema,ensure_ascii=False,separators=(',',':'))+'\nSOURCE\n'+json.dumps(case['text'],ensure_ascii=False)+'\nERRORS\n'+json.dumps(errors)
                body = {'model':args.model,'stream':False,'format':schema,'keep_alive':0,'options':{'num_ctx':8192,'num_predict':2048,'temperature':0,'seed':1,'num_gpu':0}}
                if args.template == 'qwen3-raw':
                    path = '/api/generate'
                    body.update(raw=True,prompt=f'<|im_start|>system\n{prompt}<|im_end|>\n<|im_start|>user\n{content}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n')
                else:
                    path = '/api/chat'
                    body['messages']=[{'role':'system','content':prompt},{'role':'user','content':content}]
                remaining = 600-(time.monotonic()-started)
                if remaining <= 0: errors=['deadline']; break
                req = urllib.request.Request(f'http://127.0.0.1:{args.port}'+path,data=json.dumps(body,ensure_ascii=False).encode(),headers={'Content-Type':'application/json'})
                try:
                    with opener.open(req,timeout=remaining) as r:
                        raw_bytes=r.read(1048577)
                        if len(raw_bytes)>1048576: raise ValueError('response size limit')
                        response=json.loads(raw_bytes)
                    raw=response.get('response',response.get('message',{}).get('content',''))
                    calls.append({'request':body,'raw_response':raw,'prompt_tokens':response.get('prompt_eval_count'),'output_tokens':response.get('eval_count'),'duration_ns':response.get('total_duration'),'done_reason':response.get('done_reason')})
                    if response.get('done') is not True or response.get('done_reason') != 'stop': errors=['generation incomplete']; break
                    try: proposal=parse_proposal(raw)
                    except ValueError:
                        errors=['invalid JSON: do not duplicate fields; return one bounded schema object']
                        continue
                    errors, grounded=check_proposal(case['text'],proposal,schema)
                    if not errors: break
                except (ValueError, urllib.error.URLError, TimeoutError) as e:
                    errors=[type(e).__name__]; break
            row={'case':case,'model':args.model,'model_record':model_record,'server':version,'template':args.template,'prompt_schema_sha256':identity,'seconds':round(time.monotonic()-started,2),'errors':errors,'anchors':grounded,'calls':calls,'status':'needs_independent_semantic_review','canonical_ku':False}
            report.write(json.dumps(row,ensure_ascii=False)+'\n'); report.flush()
            print(json.dumps({'case':index+1,'seconds':row['seconds'],'calls':len(calls),'errors':errors,'canonical_ku':False}),flush=True)
if __name__ == '__main__': main()
