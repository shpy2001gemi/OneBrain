# Base v1 release signer policy

Base v1 uses two independent OpenPGP Ed25519 roles. The
`qualification-approver` signs one immutable external `base-release-request`;
the `base-release` role signs the derived evidence manifest and the annotated
`base-v1.0.0` tag. A valid signature from the wrong role or any unlisted key is
rejected.

The owner-approved replacement qualification approver is
`A9BFDC59364354F954ABD26947FCF15DD9C32781`. Its canonical trust-policy digest
is `0710845f71ca7aca7ce89a0377a31ce293c6dd99778c0ff3decf9e04745528be`
under derive-key context
`onebrain:base-v1:qualification-approver-policy:1`.

The owner-approved Base release signer is
`F9DDAFB46FB6603E14B21B4DB0D9DBF23DBE8ED2` (key ID
`B0D9DBF23DBE8ED2`), valid from `2026-08-11T04:50:30Z` through
`2028-08-10T04:50:30Z`. Its exported public-key packet BLAKE3 is
`d28acd703d6bb7addad30de1e9cd0c05d1ca84dfd1e41ad5963458913382d0db`.
The canonical `base-release` policy permits only `base-evidence-manifest` and
`base-release-tag`; derive-key context
`onebrain:base-v1:release-signer-policy:1` yields
`443534ac4f583368cc5e07b1c4dbddf1ac66c63eba32bcf9e565b07f07a80d88`.

Machine receipts use a third, dedicated Ed25519 role named
`base-evidence-approver`. It approves the exact digest of every three-OS target
receipt and every Base gate receipt that does not already carry the frozen
Registry, P5, or soak child signature. The owner-approved public key is
`c40d8892b480f80b78cb1acddaa5a85c571ac5adfac71ff1ccebd6c3f6abce42`.
Derive-key context `onebrain:base-v1:evidence-approver-fingerprint:1` yields
fingerprint
`a5f274124c48fdc9c9c50a504733ac731e67a7dfdbcfe59d83bf5ed0c8944009`.
Canonicalizing the closed public policy and hashing it with derive-key context
`onebrain:base-v1:evidence-approver-policy:1` yields
`01f5989e96ca840b2ddc53781bd57dad18bf52fb332046543bfb1dbd42fb0df8`.
The key is valid from `2026-08-11T07:40:40Z` through
`2028-08-10T07:40:40Z`, and its only usages are
`gate-receipt-approval` and `target-receipt-approval`. A valid signature from
any unlisted key is rejected. No release, qualification, Registry/P5/soak, or
default local key may be repurposed for this role.

Verification parses exactly one `VALIDSIG`, requires algorithm 22, compares the
full primary fingerprint rather than a short key ID, and checks the signature
time against both the request and signer validity intervals. The manifest's
detached signature is an outer envelope and is never included in the manifest
bytes or digest it signs.

Production private keys remain outside the repository. Tests use isolated
ephemeral keyrings and may never claim production qualification.
