# Base v1 qualification-approver policy

Base release requests are detached OpenPGP Ed25519 signatures made for the
`base-release-request` usage by the `qualification-approver` role. Verification
uses GPG status output and accepts only the full primary fingerprint from one
`VALIDSIG` record; a cryptographically valid signature from any unlisted key is
rejected.

The owner-approved signer is
`CB3FF16A1A2C8B017B5D83DF59DC9C079E00928B` (key ID
`59DC9C079E00928B`), valid from `2026-08-09T13:27:27Z` through
`2028-08-08T13:27:27Z`. Its exported public-key packet BLAKE3 is
`ecee4527ed22908e0afc3a859492f7e0be7d4f4ccef087dd2781673364f39108`.

The policy object is canonical UTF-8 JSON with sorted keys, separators `,` and
`:`, `ensure_ascii=false`, and no insignificant whitespace. BLAKE3 derive-key
context `onebrain:base-v1:qualification-approver-policy:1` yields
`2e7cc2dacafad658ab5fe4e1536a4b92590f788c9c9e5a450d123930d65cfbd6`.

The production private key is external to this repository. Tests create only
ephemeral keys in isolated temporary GPG homes.
