"""Bind KU runtime dispatch to its approved Base registrations and golden corpus."""
from pathlib import Path
import hashlib
import json

ROOT = Path(__file__).resolve().parents[2]
OPERATIONS = ['prepare','preview','save','get','list','search','revise','export','status','cancel','reconcile']
DTO_NAMES = ['KuPrepareV1','KuPreparedV1','KuOperationRefV1','KuSaveV1','KuReceiptV1','KuGetV1','KuViewV1','KuListV1','KuSearchV1','KuPageV1','KuReviseV1','KuExportV1','KuExportViewV1','KuStatusV1','KuFailureV1','KuStatusRequestV1','KuPreparedArtifactV1','KuSummaryV1']


def validate_registration(ku: dict, base: dict) -> None:
    def require(value, reason):
        if not value:
            raise ValueError('KU registration: ' + reason)
    ids = {name: 0x4b01+i for i, name in enumerate(OPERATIONS)}
    dto_ids = {name: 0x4c01+i for i, name in enumerate(DTO_NAMES)}
    registration = ku.get('registration', {})
    golden_path = ROOT/'src/test-vectors/vnext/ku-semantic-content-v1.json'
    require(registration == {'decision':'D-016','base_profile_minor':2,'local_payload_ids':ids,'dto_ids':dto_ids,
                            'domain':'semantic-content/1','golden_vectors':'ku-semantic-content-v1.json',
                            'golden_sha256':hashlib.sha256(golden_path.read_bytes()).hexdigest()}, 'dispatch registration or golden corpus binding drift')
    require(base['profile_version'] == {'major':1,'minor':2}, 'Base profile minor must advance before dispatch')
    section = base.get('ku_payloads', {})
    require(section == {'format':'onebrain/base-ku-payloads/1','minimum_base_minor':2,'encoding':'bounded_json_utf8',
                        'types':ku['types'],'dtos':ku['dtos'],'dto_ids':dto_ids,'operations':ku['operations'],'errors':ku['errors']}, 'Base payload declarations differ from approved KU contract')
    defs = base['type_definitions']
    require(defs['KuOperationKindV1'] == {'kind':'enum','repr':'u16','closed':True,'variants':[{'id':ids[n],'name':n.capitalize()} for n in OPERATIONS]}, 'operation discriminator allocation drift')
    require(defs['KuDtoKindV1'] == {'kind':'enum','repr':'u16','closed':True,'variants':[{'id':dto_ids[n],'name':n} for n in DTO_NAMES]}, 'DTO discriminator allocation drift')
    require({o['name']:o['wire_id'] for o in ku['operations']} == ids, 'unregistered wire discriminator')
    golden = json.loads(golden_path.read_text(encoding='utf-8'))
    require(golden['profile'] == ku['identity']['profile'], 'golden profile mismatch')
    rows = golden['vectors']
    require([r['name'] for r in rows] == ['base','negated','argument_order','statement_order','unit_8','unit_9'], 'missing equality/separation cases')
    require(len({r['semantic_cid'] for r in rows}) == len(rows), 'semantic separation collision')
    for r in rows:
        require(len({r['semantic_cid'],r['other_profile_cid'],r['object_domain_same_root']}) == 3, 'cross-domain/profile collision')
    foundation = json.loads((ROOT/'src/test-vectors/vnext/foundation/canonical-v1.json').read_text(encoding='utf-8'))
    require(any(r['domain']=='semantic-content' and r['digest_hex']=='4b6160440776adfebaba4b9ce675c5450f1c6d6d05b33687e5164948554c9e3f' for r in foundation['domain_digests']), 'semantic domain golden absent')
