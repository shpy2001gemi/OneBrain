# KU-ENC-003 autonomous input recovery — 2026-09-27

The owner directed the agent to find or create preparation inputs rather than
ask the owner to locate artifacts previously prepared in this project. This
supersedes the earlier next-action request for workbook paths. It does not waive
qualification gates or manufacture independent reviewer evidence.

Follow-up: [offline preparation](KU_ENC_003_OFFLINE_PREPARATION.md) reverified
the 13 pins, added a repeatable preparation audit and completed native Registry
verification. Required reading is complete. The existing per-source text hashes
are NFC comparison hashes; exact UTF-8 exports still need their own bindings.
The remaining-work list below records the recovery checkpoint, not completion
of the later execution/grading harness or reviewer/run-lock gates.

## Recovered local artifacts

Both workbooks were found in `C:/Users/shpy2/Documents/OneBrain KU data`:

- `KU_100_nguon_tieng_Viet_holdout.xlsx`
- `KU_100_English_independent_holdout.xlsx`

Each has `Nguon` and `Nhan_truoc_encode` sheets, headers on row 5 and 100
source/annotation rows. Local code inspected values for counts and commitments;
source text, expected meanings and label contents were not emitted into chat.
The workbooks were not modified. This is automated intake, not semantic review.

| Observed check | Vietnamese | English |
|---|---:|---:|
| Source rows / annotation rows | 100 / 100 | 100 / 100 |
| Declared holdout rows | 100 | 100 |
| Declared independent groups | 100 | 100 |
| Duplicate source IDs / annotation IDs | 0 / 0 | 0 / 0 |
| Source and annotation ID sets match | Yes | Yes |
| Exact source duplicates after NFC | 0 | 0 |
| Exact NFC overlap with three recovered development workbooks | 0 | 0 |
| Empty reviewer/provenance fields | 100 | 100 |

Source metadata fields, including origin, URL, permission and grouping, are
populated. Their presence does not verify provenance, reuse rights or group
independence. Exact-text comparison does not detect translations, paraphrases
or shared templates. It covers only the three known development workbooks,
not every source ever exposed in project conversations or probes.

The excluded development files were found under
`C:/Users/shpy2/.codex/outputs/01a075c0-0712-7bd1-9946-2470f31c85f5`:
`KU_100_nguon_tieng_Viet_da_sua.xlsx`, `KU_20_nguon_tieng_Viet.xlsx` and
`KU_20_nguon_tieng_Anh.xlsx`. They remain development material.

The existing host configuration was located at
`C:/Users/shpy2/Documents/OneBrainLocal/host.json`. It references the local
Registry root and has a configured public key. Release
`owner-local-20260907-v1` contains `release.stamp.json`, `concepts.obr`, both
indexes, a manifest and SBOM. This turn records paths and metadata hashes only;
it does not claim fresh signature/data verification or current qualification
authority. No private key, API token or Vault key was read. No host was changed.

## Created private preparation files

The offline intake script is stored outside Git at
`C:/Users/shpy2/.codex/ku-enc-003-evidence/intake_inventory_20260927.py`.
It has no inference or network path. Its successful invocation used the bundled
Python runtime and created a fresh directory without overwriting earlier evidence:

`C:/Users/shpy2/.codex/ku-enc-003-evidence/intake-20260927T004431004208Z`

- `intake.json`: exact workbook hashes, per-source and annotation commitments,
  observed counts, development exclusion inventory and exposure limits.
- `run-package.draft.json`: current Git HEAD, contract hashes, intake binding,
  host/Registry metadata candidates and explicit unfilled execution bindings.
- `REVIEWER_PACKET.md`: concrete review procedure, evidence requirements and
  grading separation, without invented reviewer identity or acknowledgement.

The timestamp is the local tool's observed UTC clock value; it is not a trusted
timestamp or reviewer signature. The package has `locked=false` and
`model_qualified=false`. Hashing the corpus now is not proof of historical
blindness or a complete preregistration. No model call occurred.

An initial intake attempt stopped because older development workbooks use
`20_nguon` rather than `Nguon`. The script was corrected to recognize those
known layouts and rerun successfully; the failed attempt's empty directory is
preserved. A bounded historical-log path search was stopped after a slow regular
expression scan; no claim of exhaustive conversation/provenance search is made.

## Remaining work owned by the agent

1. Finish the full task-23 required read set before implementing the execution
   harness. The current script is a private preparation utility, not that harness.
2. Verify the recovered Registry through its existing native verifier and build
   isolated source custody/grants under the owner's authorized local purpose.
   Do not import synthetic development authority or modify the live host.
3. Expand contamination/provenance checks without exposing holdout text to the
   development conversation. Keep unresolved independence findings explicit.
4. Complete exact family/tokenizer/worker/device bindings, admission, measurement,
   durable failure accounting and the preregistered analysis plan. The existing
   second-family and worker-measurement gaps remain engineering work.
5. Obtain actual reviewer/provenance evidence and acknowledgement for the exact
   final package. Every recovered reviewer field is blank. An agent-created
   reviewer packet can organize this work, but cannot establish two evidenced
   independent administrative-principal/pipeline groups by self-declaration.

The owner no longer needs to supply paths already recovered here. A final
reviewer acknowledgement and model run lock cannot truthfully be created while
the harness and evaluator bindings remain unfinished. Continue the technical
preparation before asking for any concrete remaining external action.

Task status remains in PROGRESS: `Blocked`, `model_qualified=false`. Keep all
model/platform/NAT limits, downstream dependencies, OBP default-off, rollback,
legacy data and durable state. No tuning, mobile work, remote host operation,
commit, push, merge or branch deletion was performed.

Verification passed: all 13 private artifact pins rehashed correctly, aggregate
vNext validation, 204 local handoff links and `git diff --check`. Tracked source,
scripts and specifications are unchanged. No runtime or model suite was run.
