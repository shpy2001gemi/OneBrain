"""Exercise the authenticated durable draft API with owner-approved dev sources.

Retains private draft jobs for inspection. Never prepares, saves or publishes KU.
Only cancels its own job if the finite polling deadline expires.
"""
import argparse
import json
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from .probe_review_proposal import NoRedirect


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--development-inputs',type=Path,required=True)
    parser.add_argument('--report',type=Path,required=True)
    parser.add_argument('--token-file',type=Path,required=True)
    parser.add_argument('--port',type=int,default=4280)
    parser.add_argument('--model',required=True)
    parser.add_argument('--consent-development-text',action='store_true',required=True)
    args=parser.parse_args()
    if not 1024 <= args.port <= 65535:
        parser.error('invalid local port')
    cases=json.loads(args.development_inputs.read_text(encoding='utf-8'))
    if not 1 <= len(cases) <= 16 or any(not 0 < len(c['text'].encode()) <= 8192 for c in cases):
        parser.error('bounded development sources required')
    token=args.token_file.read_text().strip()
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),NoRedirect())
    budget={'max_items':256,'max_bytes':1048576,'max_work_units':1000000}
    def call(path,body=None):
        req=urllib.request.Request(f'http://127.0.0.1:{args.port}/api/vnext/ku/'+path,
            data=None if body is None else json.dumps(body,ensure_ascii=False).encode(),
            headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
        with opener.open(req,timeout=30) as response:
            assert response.headers.get('Cache-Control') == 'no-store'
            raw=response.read(1048577)
        if len(raw)>1048576:
            raise ValueError('response bound')
        result=json.loads(raw)
        if not result.get('ok'):
            raise ValueError('host rejected request')
        return result['data']
    def session():
        return call('status')['session']
    def editor(action,payload):
        return call('editor',{'session':session(),'budget':budget,'request':{'action':action,'payload':payload}})['payload']
    def saved():
        return call('operations',{'session':session(),'budget':budget,'request':{'operation':'list','payload':{'limit':20}}})['payload']['items']
    with args.report.open('x',encoding='utf-8') as output:
        for case in cases:
            before=saved()
            op=call('reservations',{'session':session()})['payload']['operation_id']
            started=time.monotonic()
            payload={'operation_id':op,'idempotency_key':op,'model':args.model,'text':case['text'],'consent':True}
            initial=editor('review_start',payload)['review_job']
            admission=time.monotonic()-started
            print(json.dumps({'case':case['id'],'queued_seconds':round(admission,3),'state':initial['job']['state']}),flush=True)
            previous=None
            while True:
                view=editor('review_get',{'operation_id':op})['review_job']
                job=view['job']
                progress=(job['state'],job['calls'])
                if progress != previous:
                    print(json.dumps({'case':case['id'],'state':progress[0],'calls':progress[1]}),flush=True)
                    previous=progress
                if job['state'] not in ('queued','extracting','reviewing','repairing'):
                    break
                if time.monotonic()-started>1900:
                    editor('review_cancel',{'operation_id':op})
                    raise TimeoutError('bounded development poll expired; own job canceled')
                time.sleep(2)
            # Repeated start and reads must not resample a completed job.
            repeat=editor('review_start',payload)['review_job']
            assert repeat['job']['calls']==job['calls']
            assert repeat['job']['windows']==job['windows']
            assert any(r['operation_id']==op for r in editor('review_list',{})['review_jobs'])
            assert saved()==before, 'draft operation changed saved KU list'
            if job['profile']=='ku-semantic-selection/1.0':
                assert view['semantic_verification']=='unassessed'
                assert view['factual_verification']=='unassessed'
                assert job['state']!='draft_ready'
            record={'case':case,'model':args.model,'seconds':time.monotonic()-started,
                'admission_seconds':admission,'operation_id':op,'job':job,
                'semantic_verification':view.get('semantic_verification'),
                'factual_verification':view.get('factual_verification'),
                'canonical_ku':False,'publication_requested':False,'save_requested':False,
                'read_and_repeat_do_not_resample':True,'saved_ku_list_unchanged':True}
            output.write(json.dumps(record,ensure_ascii=False)+'\n');output.flush()
            print(json.dumps({'case':case['id'],'done':job['state'],'seconds':round(record['seconds'],2)}),flush=True)


if __name__=='__main__':
    main()
