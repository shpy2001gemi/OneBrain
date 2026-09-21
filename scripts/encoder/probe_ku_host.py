"""Run deliberate development cases through the real local KU API, preview only.
Never saves/publishes. Cancels only reservations created by this run.
Reports contain source text; choose a private new output file.
"""
import argparse
import json
import time
import urllib.error
import urllib.request
from pathlib import Path
from scripts.encoder.probe_review_proposal import NoRedirect

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--development-inputs',type=Path,required=True)
    p.add_argument('--report',type=Path,required=True)
    p.add_argument('--token-file',type=Path,required=True)
    p.add_argument('--port',type=int,default=4280)
    p.add_argument('--model',required=True)
    p.add_argument('--consent-development-text',action='store_true',required=True)
    a=p.parse_args()
    if not 1024 <= a.port <= 65535: p.error('invalid loopback port')
    cases=json.loads(a.development_inputs.read_text(encoding='utf-8'))
    if not 1 <= len(cases) <=16 or any(not 0<len(c['text'].encode())<=8192 for c in cases): p.error('bounded cases required')
    token=a.token_file.read_text().strip()
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),NoRedirect())
    def call(route,body=None):
        req=urllib.request.Request(f'http://127.0.0.1:{a.port}'+route,data=None if body is None else json.dumps(body,ensure_ascii=False).encode(),headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
        try:
            with opener.open(req,timeout=620) as r: raw=r.read(1048577)
        except urllib.error.HTTPError as e: raw=e.read(1048577)
        if len(raw)>1048576: raise ValueError('response size limit')
        return json.loads(raw)
    budget={'max_items':256,'max_bytes':1048576,'max_work_units':1000000}
    with a.report.open('x',encoding='utf-8') as output:
        for i,case in enumerate(cases):
            status=call('/api/vnext/ku/status')
            session=status['data']['session']
            op=call('/api/vnext/ku/reservations',{'session':session})['data']['payload']['operation_id']
            def invoke(name,payload): return call('/api/vnext/ku/operations',{'session':session,'budget':budget,'request':{'operation':name,'payload':payload}})
            started=time.monotonic()
            report={'case':case,'model':a.model,'save_requested':False,'publication_requested':False,'host_status':status['data']['payload']}
            try:
                intake=call('/api/vnext/ku/editor',{'session':session,'budget':budget,'request':{'action':'encode_text','payload':{'operation_id':op,'idempotency_key':op,'model':a.model,'text':case['text'],'consent':True}}})
                report['result']=invoke('prepare',intake['data']['payload']) if intake.get('ok') else intake
            except (ValueError,urllib.error.URLError,TimeoutError) as e: report['transport_error']=type(e).__name__
            finally:
                report['seconds']=round(time.monotonic()-started,2)
                try: report['cleanup']=invoke('cancel',{'operation_id':op})
                except Exception as e: report['cleanup_error']=type(e).__name__
                output.write(json.dumps(report,ensure_ascii=False)+'\n');output.flush()
                print(json.dumps({'case':i+1,'seconds':report['seconds'],'ok':report.get('result',{}).get('ok'),'error':report.get('result',{}).get('error'),'cleanup':report.get('cleanup',{}).get('data',{}).get('payload',{}).get('state')},ensure_ascii=False),flush=True)
if __name__=='__main__':main()
