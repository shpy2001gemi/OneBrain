# KU-WEB-001 experimental Ollama addition

Owner approval: D-023. The local Web now accepts text, offers host-admitted
installed Qwen3 models (preferring `qwen3:8b`), asks for source consent, and runs
the shared encoder before displaying an exact private preview. Save is explicit.
`model_qualified` remains `false`; this is development integration evidence,
not KU-ENC-003 quality qualification. No private VI/EN holdout was opened.

## Run on this Windows host

Use the same stable dataset, Vault key, API token and activated signed Registry
as the [manual host instructions](KU_WEB_001_IMPLEMENTATION.md). Raw text intake
now creates its own governed private SourceArtifact; pre-existing source files
are only needed for the optional manual editor. An unsigned `concepts.obr` still
does not satisfy signed Registry admission.

Add this section to the trusted host JSON outside Git:

```json
{
  "ollama": {
    "executable": "C:/Users/shpy2/AppData/Local/Programs/Ollama/ollama.exe",
    "models_dir": "C:/Users/shpy2/.ollama/models",
    "models": ["qwen3:8b"],
    "memory_limit_bytes": 12884901888
  },
  "sources": []
}
```

These are additional fields; retain `data_dir`, `registry_root`,
`registry_public_key`, `vault_key_file`, `api_token_file`, `web_dir` and `port`.
Do not replace the whole host configuration with this fragment. Add another
installed Qwen3 tag to `models` to offer it in the selector, at most eight.
Models are admitted at startup and never downloaded by the host.

From repository root, PowerShell:

```powershell
Push-Location src/onebrain-web
npm ci
npm run build
Pop-Location
Push-Location src
cargo run --locked --config 'profile.dev.package.sha2.opt-level=3' -p onebrain-api --example ku_local_web -- C:/OneBrainLocal/host.json
Pop-Location
```

The SHA-256 optimization only accelerates full installed-artifact verification
in a debug build; it does not skip integrity checks. A release build also works.
Open `http://127.0.0.1:4280/ku`, authenticate with the configured token, select
`qwen3:8b`, enter a short complete statement, acknowledge consent, then click
**Encode and preview**. Only **ready** output can be saved using
**Save exact preview privately**. Registry coverage still determines whether
concepts can resolve. The product does not install the test Registry used below.

During inference, the page polls authenticated operation status and leaves
**Cancel pending draft** usable. Keep the operation ID for **Reconcile operation**
or recovery after page reload. Refresh/recovery never resamples the model.
Sources and consent are retained privately even if a draft is canceled.

If model verification fails, its selector entry is absent and existing saved
KU remains readable. To disable inference while retaining reads, remove the
`ollama` configuration and restart with the same dataset/key/Registry. To admit
a changed/new model, restart so its complete artifacts are verified again.
No user Ollama server needs to be stopped; the host owns a separate worker.

## Implementation and limits

The additive editor actions are specified in
[the experimental profile](../../../specs/vnext/KU_EXPERIMENTAL_OLLAMA_PROFILE_V1.md).
`encode_text` checks the current authenticated reservation and explicit consent,
then durably stages actual policy, receipt, scope, assessment, source and prepare
template in an encrypted table of the existing KU journal. Opaque local record
references resolve against this bundle; no fake ObservationEvent, public policy
grant or competing Vault writer is introduced.

Preparation uses `OllamaKuInputs` → `SharedKuExtractionInputs` →
`ExtractionWorkflow` → `ManagedOllamaProvider` → strict Candidate/span/coverage
validation → signed Registry resolution → native SEM compiler → KU preview.
No legacy encode endpoint, model tool agent or model-to-Vault write is used.

Windows launches a suspended Ollama worker, assigns it to a Job Object with
kill-on-close and memory limits, then resumes it. Completion, cancellation and
timeout destroy only this worker tree. CPU-only operation reserves four GiB for
host tokenizers/parsers and limits worker memory to the remaining configured
reservation (eight GiB in the example). One inference call is admitted across
the host's models. The job deadline is 120 seconds including startup; startup
itself is bounded to ten seconds. Slow or invalid output fails without a KU.

Admission verifies complete GGUF/config/template/parameter layer hashes,
Ollama executable and native CPU runner/DLL hashes. Windows read-sharing locks
keep these existing files immutable while admitted. The exact Qwen3 raw ChatML
prompt, empty thinking block, schema glossary, byte-span hints, reviewed example,
tokenizer metadata and options are bound into implementation identity.
The tokenizer uses the verified GGUF BPE vocabulary/merges and Qwen2 pre-tokenizer.
Returned prompt token counts must match the host count; truncation/over-budget
output is rejected. JSON responses are bounded to one MiB.

Text intake accepts at most 8192 UTF-8 bytes per submission, 256 retained inputs
and eight MiB of encrypted intake records per dataset, including canceled work.
The host currently processes each text as one whole context. Exact token limits
can reject text below the byte limit. There is no automatic chunk splitting,
garbage collection, live revocation UI, model pull UI or GPU worker profile.
The browser holds pending IDs/text only in memory. Non-Windows worker admission
is unavailable. Production signed Registry and secret provisioning remain
operator responsibilities; this addition does not create a deployment.

## Verification

Focused verification on Windows:

| Check | Result |
|---|---|
| API tests | 27 library + 8 integration passed; real-model test separately invoked below. |
| Node `--lib ku_` | 22 passed, including UTF-8 custody/consent and existing crash/recovery/cancel cases. |
| Encoder `extraction --lib` | 18 passed; the real worker probe is separately invoked. |
| Owned Windows worker probe | Passed: Job Object reports zero remaining processes and its server port closes on drop. |
| Real `qwen3:8b` API roundtrip | Passed: HTTP 200, `ready` preview, explicit committed private save, exact accepted KU readable after restart. Inference plus validation: 109.037 seconds; artifact verification: 6.007 seconds. |
| Web `test:ku` | 10 passed, including consent, model selection, explicit AI save and cancellation with late output. |
| Web build | TypeScript + Vite passed. |
| Web lint | Passed with four existing warnings outside KU files. |
| Local host example | `cargo check --locked -p onebrain-api --example ku_local_web` passed. |
| Feature-disabled API | `cargo check --locked -p onebrain-api --no-default-features` passed with existing non-KU warnings. |
| Contract checks | vNext validator and generated Base projection check passed. |
| Formatting | Cargo format and whitespace checks passed. |

Native test commands use `--config 'profile.test.package.sha2.opt-level=3'` to
accelerate full SHA-256 hashing in test builds; no integrity or validation step
is skipped. Deterministic API tests inject a clearly test-only proposal provider
to test the actual host composition: consent/model/reservation/generation fences,
duplicate output rejection, no save before preview, exact save, cancellation and
encrypted reopen with no model and no resampling. They do not measure model quality.

The live probe uses a temporary test-only signed
one-concept Registry (`is`) and the development sentence `Copper is conductive.`.
It exercises the same authenticated API/intake/shared workflow used by the Web.
This synthetic Registry is confined to tests and is never loaded by the host example.

Final successful development run, 2026-09-06, Windows / Intel Core Ultra 7 155H
(32 GiB RAM), Ollama 0.33.3, CPU-only `qwen3:8b` Q4_K_M. The observed worker
model/context allocation was approximately 6.66 GB with zero VRAM. The host's
exact tokenizer count matched Ollama's returned prompt count; the provider would
have rejected the result otherwise. No worker remained after completion; the
pre-existing user Ollama server was retained.

| Verified pin | SHA-256 |
|---|---|
| GGUF weights | `a3de86cd1c132c822487ededd47a324c50491393e6565cd14bafa40d0b8e686f` |
| Derived exact tokenizer specification | `85c3b9a1c67db47e29f4affb1400c8882e0e1eec4f2b9cad2407d85a908aa2c8` |
| Backend binding (executable, CPU DLLs, installed manifest and provider/rendering/worker source) | `9a42802331aaa27458cdad4e6e34b7178c3fe981d8720b3b94ffa7dc7de0c8a5` |
| Reviewed schema bundle | `8603b99017024d55231e935e4d01d943becf3c4136a5b0afc17b5240fa3ff834` |

Runtime options: raw Qwen3 ChatML, empty thinking block, full Candidate JSON
format, `stream:false`, `keep_alive:0`, `num_ctx:8192`, `num_predict:2048`,
`temperature:0`, `seed:1`, `num_gpu:0`; 12 GiB total declared reservation with
8 GiB worker limit and 4 GiB host allowance. Longer text or CPU contention may
exceed the 120-second deadline. This one successful sentence is not an accuracy
or latency qualification result.

The manually invoked real-model test is excluded from routine CI:

```powershell
Push-Location src
$env:KU_OLLAMA_EXE = 'C:\Users\shpy2\AppData\Local\Programs\Ollama\ollama.exe'
$env:KU_OLLAMA_MODELS = 'C:\Users\shpy2\.ollama\models'
$env:KU_OLLAMA_MODEL = 'qwen3:8b'
cargo --config 'profile.test.package.sha2.opt-level=3' test --locked -p onebrain-api experimental_ollama_real_text_roundtrip -- --ignored --nocapture
Pop-Location
```

Development probes exposed deadline exhaustion and an incorrect source quote
span; both were rejected before creating a saveable KU. Prompt compaction and
host byte-position hints address these observed integration issues. These runs
are not a holdout, accuracy/convergence benchmark, or a promise that arbitrary
text will pass on this machine.
