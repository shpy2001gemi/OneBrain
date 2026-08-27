# Base v1 qualification-approver policy

Base release requests are detached OpenPGP Ed25519 signatures made for the
`base-release-request` usage by the `qualification-approver` role. Verification
uses GPG status output and accepts only the full primary fingerprint from one
`VALIDSIG` record; a cryptographically valid signature from any unlisted key is
rejected.

The owner-approved replacement signer is
`A9BFDC59364354F954ABD26947FCF15DD9C32781` (key ID
`47FCF15DD9C32781`), valid from `2026-08-27T04:49:51Z` through
`2028-08-26T04:49:51Z`. Its exported public-key packet BLAKE3 is
`228d43ea4f3cc0b7548124682e353544ae9549e5458456016242cfa738a5575e`.
The public key is versioned in
`src/test-vectors/vnext/base-v1-qualification-approver-v2.asc`; its private
key is held only in the external signing authority.

The policy object is canonical UTF-8 JSON with sorted keys, separators `,` and
`:`, `ensure_ascii=false`, and no insignificant whitespace. BLAKE3 derive-key
context `onebrain:base-v1:qualification-approver-policy:1` yields
`0710845f71ca7aca7ce89a0377a31ce293c6dd99778c0ff3decf9e04745528be`.

The production private key is external to this repository. Tests create only
ephemeral keys in isolated temporary GPG homes.
