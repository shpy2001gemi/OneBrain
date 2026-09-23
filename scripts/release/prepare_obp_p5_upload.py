"""Assemble an unsigned, source-free P5 staging bundle from measured Linux builds.

Does not sign a request, install services, connect to hosts or run qualification.
The existing P5 manifest verifier remains authoritative for production admission.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile

import blake3

ROOT = Path(__file__).resolve().parents[2]
BINARIES = (
    "onebrain-relay", "relay_preflight_probe", "p5_multi_host_agent",
    "p5_multi_host_agent_v2", "p5_agent_ctl_v2", "p5_receipt_signer_v2",
    "p5_identity_signer_v2", "p5_admin_ctl_v2", "p5_recovery_ops_v2",
    "p5_canary_preflight", "p5_operations_preflight", "dr_m5_soak_release",
)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


VERIFY = r'''#!/usr/bin/env python3
import hashlib,json,os,platform,stat,subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parents[1]
assert platform.system()=='Linux' and platform.machine()=='x86_64','Linux x86_64 required'
libc=subprocess.check_output(['getconf','GNU_LIBC_VERSION'],text=True).strip().split()[-1]
assert tuple(map(int,libc.split('.'))) >= (2,39),'glibc >= 2.39 required by P5 manifest'
manifest=root/'metadata/bundle.manifest.json'
m=json.loads(manifest.read_bytes())
assert m['format']=='onebrain/base-v1-native-runner-bundle/1'
assert m['qualification_tier']=='prepared-not-production-qualified'
assert m['private_material_included'] is False
expected={'metadata/bundle.manifest.json'}
assert len({r['path'] for r in m['files']})==len(m['files']),'duplicate manifest path'
for row in m['files']:
    p=Path(row['path'])
    assert not p.is_absolute() and '..' not in p.parts
    p=root/p
    assert not p.is_symlink() and p.is_file(), row['path']
    data=p.read_bytes()
    assert len(data)==row['size'] and hashlib.sha256(data).hexdigest()==row['sha256'],row['path']
    assert stat.S_IMODE(p.stat().st_mode)==int(row['mode'],8),row['path']
    expected.add(row['path'])
actual=set()
for p in root.rglob('*'):
    assert not p.is_symlink(),str(p)
    if p.is_file(): actual.add(p.relative_to(root).as_posix())
assert expected==actual,'extra or missing file'
assert hashlib.sha256((root/'metadata/BUILD-PROVENANCE.json').read_bytes()).hexdigest()==m['build']['digest']
for binary,receipt in [('p5_multi_host_agent_v2','agent-binding.json'),('relay_preflight_probe','relay-binding.json')]:
    actual=subprocess.check_output([str(root/'bin'/binary),'--print-compiled-binding'],timeout=20)
    assert actual==(root/'metadata'/receipt).read_bytes(),'compiled binding mismatch'
    binding=json.loads(actual)
    assert binding['candidate_commit']==m['candidate']['id'] and binding['candidate_tree']==m['candidate']['version']
print('Bundle SHA-256/mode/binding checks PASS; unsigned staging, not qualification.')
'''

INSPECT = r'''#!/bin/bash
set -euo pipefail
case "${1:-}" in host-a|host-b|host-c) ;; *) echo 'usage: bash inspect-host.sh host-a|host-b|host-c' >&2; exit 2;; esac
printf 'host_role=%s\n' "$1"
uname -sm
getconf GNU_LIBC_VERSION
systemctl --version | head -1
for cmd in python3 gpg ssh nft ip tc systemctl sudo; do command -v "$cmd" || true; done
printf '\nExisting candidate selector (no changes):\n'
readlink -f /opt/onebrain/base-v1/current || true
printf '\nExisting service state (no changes):\n'
for unit in onebrain-relay-p5.service onebrain-p5-agent-v2.service onebrain-p5-agent-v2.socket onebrain-p5-receipt-signer-v2.socket onebrain-p5-identity-signer-v2.socket; do
  systemctl show "$unit" --property=LoadState,ActiveState,SubState,FragmentPath,ExecStart --no-pager || true
done
printf '\nExisting required users (no key material):\n'
for user in onebrain-relay onebrain-p5-agent onebrain-p5-receipt-signer onebrain-p5-identity-signer onebrain-p5-probe-ssh onebrain-p5-control-ssh onebrain-p5-admin-ssh; do id "$user" || true; done
printf '\nPublic SSH host-key fingerprint (no private key):\n'
if test -r /etc/ssh/ssh_host_ed25519_key.pub; then ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub; fi
printf '\nLocal TCP 443 listener (no outbound probe):\n'
if command -v ss >/dev/null; then ss -ltn '( sport = :443 )'; fi
printf '\nExisting relay config equality (no config contents printed):\n'
bundle_root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
case "$1" in
  host-b|host-c)
    config="relay-${1#host-}.json"
    if test -r /etc/onebrain/relay-p5.json; then
      if cmp -s "$bundle_root/config/$config" /etc/onebrain/relay-p5.json; then echo exact_bundle_config; else echo differs_requires_review; fi
    else echo config_unavailable; fi ;;
esac
printf '\nNo service, key, network or configuration was changed. Keep this report private.\n'
'''


def assemble(build, source, output, runtime_templates=None):
    build, source, output = build.resolve(), source.resolve(), output.resolve()
    binding = json.loads((build / "build-env.json").read_text())
    env = binding["env"]
    commit, tree = env["ONEBRAIN_BASE_COMMIT"], env["ONEBRAIN_SOURCE_TREE"]
    def git(*args):
        return subprocess.check_output(["git", "-C", str(source), *args]).strip().decode()
    if git("rev-parse", "HEAD") != commit or git("rev-parse", "HEAD^{tree}") != tree or git("status", "--porcelain"):
        raise ValueError("source must be the exact clean measured candidate")
    output.mkdir(parents=True, exist_ok=False)
    root = output / ("onebrain-p5-" + commit[:12])
    root.mkdir()
    def put(path, data):
        p = root / path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data.encode() if isinstance(data, str) else data)
    for name in BINARIES:
        data = (build / "bin" / name).read_bytes()
        if data[:6] != b"\x7fELF\x02\x01" or int.from_bytes(data[18:20], "little") != 62:
            raise ValueError("not a Linux x86_64 ELF: " + name)
        put("bin/" + name, data)
    for receipt in ("agent-binding.json", "relay-binding.json"):
        raw = (build / receipt).read_bytes()
        info = json.loads(raw)
        for key, expected in (("candidate_commit", commit), ("candidate_tree", tree), ("toolchain_digest", env["ONEBRAIN_TOOLCHAIN_DIGEST"])):
            if info[key] != expected:
                raise ValueError("compiled binding mismatch: " + key)
        put("metadata/" + receipt, raw)
    source_digest = hashlib.sha256(subprocess.check_output(["git", "-C", str(source), "archive", "HEAD", "src", "scripts", "docs/specs/vnext"])).hexdigest()
    provenance = {"candidate_commit": commit, "candidate_tree": tree, "source_digest_sha256": source_digest,
                  "builder_image_id": binding["image"], "compiler_blake3": env["ONEBRAIN_TOOLCHAIN_DIGEST"],
                  "build_environment": env, "qualification": False,
                  "commands": (build / "build.sh").read_text(),
                  "cargo_lock_sha256": hashlib.sha256((source / "src/Cargo.lock").read_bytes()).hexdigest()}
    put("metadata/BUILD-PROVENANCE.json", canonical(provenance))
    put("metadata/rustc-vV.txt", (build / "rustc-vV.txt").read_bytes())
    put("metadata/candidate-commit.txt", commit + "\n")
    put("metadata/candidate-tree.txt", tree + "\n")
    put("scripts/verify.py", VERIFY)
    put("scripts/verify.sh", '#!/bin/bash\nset -euo pipefail\nexec python3 "$(dirname "$0")/verify.py"\n')
    put("scripts/inspect-host.sh", INSPECT)
    if runtime_templates is not None:
        # Reuse measured operational templates, never executables or identities.
        old = json.loads((runtime_templates / "metadata/bundle.manifest.json").read_bytes())
        records = {row["path"]: row for row in old["files"]}
        selected = ["scripts/verify.sh"] + ["units/" + name for name in (
            "onebrain-relay-p5.service", "onebrain-p5-agent-v2.service",
            "onebrain-p5-agent-v2.socket", "onebrain-p5-identity-signer-v2.service",
            "onebrain-p5-identity-signer-v2.socket", "onebrain-p5-receipt-signer-v2.service",
            "onebrain-p5-receipt-signer-v2.socket")]
        measured = {}
        for name in selected:
            path = runtime_templates / name
            if path.is_symlink():
                raise ValueError("template symlink forbidden")
            data = path.read_bytes()
            row = records[name]
            if (len(data) != row["size"] or hashlib.sha256(data).hexdigest() != row["sha256"]
                    or blake3.blake3(data).hexdigest() != row["blake3"]):
                raise ValueError("recovered template differs from its manifest: " + name)
            put(name, data)
            measured[name] = row["sha256"]
        put("metadata/runtime-template-provenance.json", canonical({
            "source_manifest_sha256": hashlib.sha256((runtime_templates / "metadata/bundle.manifest.json").read_bytes()).hexdigest(),
            "source_candidate": old["candidate"]["id"], "files_sha256": measured}))
    for role, address in (("b", "163.61.111.23"), ("c", "103.77.214.30")):
        put(f"config/relay-{role}.json", canonical({"format": "onebrain/relay-config/1",
            "data_root": "/var/lib/onebrain/relay-p5", "identity_key_locator": "/var/lib/onebrain/relay-p5/identity.key",
            "udp_bind": None, "tcp443_bind": "0.0.0.0:443", "advertised_endpoints": [f"tls://{address}:443"],
            "max_reservations": 256, "max_reservations_per_target": 3, "rendezvous_max_records": 256,
            "log_destination": "journald"}))
    for name in ("ONEBRAIN_OUTBOUND_FIRST_RELAY_GUIDE.md", "ONEBRAIN_BASE_V1_P5_MULTI_HOST_GUIDE.md"):
        put("docs/" + name, (source / "docs/operations" / name).read_bytes())
    for name in ("P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2.md", "P5_OUTBOUND_FIRST_PREFLIGHT_PROFILE_V2.md"):
        put("docs/" + name, (source / "docs/specs/vnext" / name).read_bytes())
    put("README.vi.md", f'''# Bộ upload P5 — ba host hiện có

Candidate: `{commit}`; tree: `{tree}`.
Linux x86_64, yêu cầu glibc >= 2.39 theo manifest P5 hiện hành.
Python 3 và các thư viện động ELF (kiểm tra bằng ldd bin/p5_multi_host_agent_v2)
cần có trên host; không cần Rust/Cargo hoặc source để chạy binary.
Gói chứa 12 ELF đã build, không chứa source, Registry hay private key.
Trạng thái: **prepared-not-production-qualified; chưa đủ đầu vào để chạy P5**.

## Upload và kiểm tra trên từng host

Upload cùng một tar.gz và file SHA256SUMS.upload tới thư mục staging mới trên
cả ba host. Không giải nén đè /opt, không thay current hoặc service đang chạy.

```bash
sha256sum -c SHA256SUMS.upload
tar -xzf onebrain-p5-{commit[:12]}-linux-x86_64.tar.gz
cd onebrain-p5-{commit[:12]}
bash scripts/verify.sh
```

Sau đó chạy đúng một lệnh theo host, lưu output ở NGOÀI thư mục bundle:

```bash
bash scripts/inspect-host.sh host-a > ../host-a-readiness.txt
```

Trên runner-b thay host-a bằng host-b; trên runner-c thay bằng host-c.
Ba file readiness chỉ đọc trạng thái, không probe mạng hay đọc private key.
Giữ chúng private và chuyển về máy điều phối để đối chiếu cấu hình đã cài.

Runner-a: chỉ node outbound. Runner-b: node + relay 163.61.111.23:443.
Runner-c: node + relay 103.77.214.30:443. Endpoint cũ cần probe lại từ hai host
còn lại trước inventory mới; địa chỉ trong config không chứng minh reachability.

## Trước khi thực hiện P5

Giữ nguyên khóa/identity/durable state trên host cũ. Không chạy generate-key
hay generate-identity để thay khóa hiện có. Không chạy installer legacy/Ollama.
Gói này không tự cài unit, sửa SSH/sudoers, NAT/firewall, current hay bật relay.
Hướng dẫn canonical trong docs mô tả các bước triển khai nhưng bộ native unit
template ngoài Git của lần trước hiện không có trên workstation; không dùng
template tự đoán để thay unit đang hoạt động. Cần đối chiếu unit/forced-command
hiện hữu và gắn chúng với generation bất biến sau khi có readiness.

Còn cần request Base hợp lệ + chữ ký/policy/keyring, request P5 mới + policy đã
duyệt/chữ ký, inventory đúng candidate, public exports ba host, bằng chứng
provider/topology, probe descriptor mới của hai relay, Registry binding đúng
candidate, và các khóa controller/SSH/recipient bên ngoài gói. Không copy khóa
controller hoặc khóa private của host khác lên ba host.

Controller dùng scripts/runner/onebrain-p5-multi-host-v2.py từ đúng checkout
candidate. Kiểm tra authority bằng verify-request trước run; collector local
obp_product_preflight.py không chạy P5 thực. Không tạo receipt hoặc qualification
flag bằng tay. Giữ production-reference và provider-document-pending đúng scope.

Manifest/hash chỉ kiểm tra nội dung staging, không thay chữ ký request hoặc
phê duyệt activation. Build này chưa có kết quả ba host. Không tự nhận qualification.
''')
    if runtime_templates is not None:
        readme = root / "README.vi.md"
        note = "\n## Operational templates recovered\n\nThis generation includes measured historical systemd templates and the original\nverify.sh (--root supported; prints manifest SHA-256). Their exact source hashes\nare recorded in metadata/runtime-template-provenance.json. The earlier missing-\ntemplate note applies only to staging generation 01. Deployment must preserve\nexisting keys and use fresh approved request/inventory bindings.\n"
        readme.write_bytes(readme.read_bytes() + note.encode())
    paths = sorted(p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file())
    put("metadata/SHA256SUMS", "".join(f"{hashlib.sha256((root/p).read_bytes()).hexdigest()} *{p}\n" for p in paths))
    paths.append("metadata/SHA256SUMS")
    rows = []
    for path in sorted(paths):
        data = (root / path).read_bytes()
        rows.append({"path": path, "size": len(data), "mode": "0555" if path.startswith(("bin/", "scripts/")) else "0444",
                     "sha256": hashlib.sha256(data).hexdigest(), "blake3": blake3.blake3(data).hexdigest()})
    manifest = {"format": "onebrain/base-v1-native-runner-bundle/1", "qualification_tier": "prepared-not-production-qualified",
        "private_material_included": False, "candidate": {"id": commit, "version": tree, "source_digest": source_digest},
        "build": {"digest": hashlib.sha256(canonical(provenance)).hexdigest(), "platform": "linux/x64", "source_date_epoch": int(git("show", "-s", "--format=%ct", "HEAD"))},
        "required_runtime": {"architecture": "x64", "minimum_glibc": "2.39", "os": "linux"}, "files": rows}
    put("metadata/bundle.manifest.json", canonical(manifest))
    archive = output / (root.name + "-linux-x86_64.tar.gz")
    with tarfile.open(archive, "w:gz") as tar:
        for path in sorted(root.rglob("*")):
            info = tar.gettarinfo(str(path), arcname=root.name + "/" + path.relative_to(root).as_posix())
            info.uid = info.gid = 0
            info.uname = info.gname = "root"
            info.mtime = manifest["build"]["source_date_epoch"]
            info.mode = 0o755 if path.is_dir() else (0o555 if path.relative_to(root).parts[0] in ("bin", "scripts") else 0o444)
            if path.is_file():
                with path.open("rb") as stream: tar.addfile(info, stream)
            else: tar.addfile(info)
    (output / "SHA256SUMS.upload").write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n", encoding="ascii")
    shutil.copyfile(root / "README.vi.md", output / "HUONG_DAN_UPLOAD.md")
    print(archive)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", required=True, type=Path)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--runtime-templates", type=Path)
    args = parser.parse_args()
    assemble(args.build, args.source, args.output, args.runtime_templates)
