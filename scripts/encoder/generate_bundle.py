"""Generate bounded schema projections, examples and version-binding manifest."""
import argparse
import hashlib
import json
from pathlib import Path

from .contract import BUNDLE, ROOT, provider_view
from .schema import check_schema, strict_loads, require


def pretty(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode('utf-8')


def generated():
    source = strict_loads((BUNDLE / 'schema.json').read_bytes())
    profile = strict_loads((BUNDLE / 'profile.json').read_bytes())
    definitions = source['$defs']
    for definition in definitions.values():
        check_schema(definition, definitions)
    outputs = {}
    for filename, root in profile['roots'].items():
        reached = set()

        def visit(value):
            if isinstance(value, dict):
                if '$ref' in value:
                    name = value['$ref'][8:]
                    if name not in reached:
                        reached.add(name)
                        visit(definitions[name])
                for child in value.values():
                    visit(child)
            elif isinstance(value, list):
                for child in value:
                    visit(child)
        visit({'$ref': '#/$defs/' + root})
        outputs[filename + '.schema.json'] = pretty({
            '$schema': source['$schema'], 'title': profile['profile'] + ' ' + root,
            '$ref': '#/$defs/' + root, '$defs': {key: definitions[key] for key in sorted(reached)},
        })
    corpus = strict_loads((BUNDLE / 'corpus.json').read_bytes())
    examples = []
    for row in corpus['cases']:
        if row['id'] in ('en-assertion', 'vi-assertion', 'vi-unsupported-quantifier'):
            examples.append({'id': row['id'], 'context': provider_view(row['context']),
                             'candidate': row['candidate']})
    outputs['examples.json'] = pretty({'profile': profile['profile'], 'examples': examples})
    sources = ['schema.json', 'profile.json', 'corpus.json', 'prompt.en.txt', 'prompt.vi.txt']
    artifacts = {name: (BUNDLE / name).read_bytes() for name in sources}
    artifacts.update(outputs)
    # The envelope is excluded from its own digest. Repository-relative tool/spec
    # hashes bind the review oracle as well, without registering protocol domains.
    extra = ['docs/specs/vnext/KU_EXTRACTION_FRAMEWORK_PROFILE_V1.md',
             'scripts/encoder/schema.py', 'scripts/encoder/contract.py',
             'scripts/encoder/generate_bundle.py', 'scripts/encoder/test_contract.py',
             'scripts/ci/validate_ku_encoder_contract.py']
    hashes = {str((BUNDLE / name).relative_to(ROOT)).replace('\\', '/'):
              hashlib.sha256(raw).hexdigest() for name, raw in artifacts.items()}
    hashes.update({name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in extra})
    for name in hashes:
        # Git stores these as LF on every platform. Reject a Windows CRLF edit
        # before hashing, rather than shipping a manifest that fails after checkout.
        raw = artifacts.get(Path(name).name) if name.startswith('docs/specs/vnext/ku-encoder-v1/') else (ROOT / name).read_bytes()
        require(b'\r' not in raw, 'bundle_requires_lf: ' + name)
    outputs['bundle.manifest.json'] = pretty({'profile': profile['profile'],
                                             'hash_algorithm': 'sha256', 'artifacts': dict(sorted(hashes.items()))})
    return outputs


def run(check=False):
    outputs = generated()
    for name, raw in outputs.items():
        path = BUNDLE / name
        if check:
            require(path.exists() and path.read_bytes() == raw, 'generated_bundle_drift: ' + name)
        else:
            path.write_bytes(raw)
    return len(outputs)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    print(f'KU encoder bundle: {run(args.check)} generated artifacts verified' if args.check
          else f'KU encoder bundle: {run()} generated artifacts written')
