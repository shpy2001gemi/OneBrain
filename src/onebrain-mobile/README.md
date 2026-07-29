# onebrain-mobile

> OneBrain Mobile — autonomous iOS and Android node.
>
> Status: **implementation scaffold; target architecture and product
> decomposition documented, production capability not yet claimed**

## Target

OneBrain Mobile is not a desktop companion or a thin client to
`onebrain-api`. The installation owns its NodeID/signing domains, private vault,
local knowledge, Concept Registry release, journals, media, and intermittent
network lifecycle.

Target stack:

```text
Flutter UI
  -> generated Pigeon API
  -> Swift/Kotlin NativeHost
  -> stable C ABI/JNI
  -> autonomous Rust mobile core
```

The product remains useful offline with every LLM and network lane disabled.
Optional device, app-managed, or remote LLM providers return candidate data;
Rust policy and deterministic handlers own every tool and durable side effect.

## Specifications

- [Implementation plan](../../docs/research/WIP_MOBILE_APP_IMPLEMENTATION_PLAN_V1.md)
- [Technical architecture](../../docs/research/WIP_MOBILE_APP_TECHNICAL_ARCHITECTURE_V1.md)
- [Feature tree](../../docs/features/mobile/MOBILE_APP_FEATURE_TREE_V1.md)
- [Feature details](../../docs/features/mobile/MOBILE_APP_FEATURE_DETAILS_V1.md)
- [App sitemap](../../docs/features/mobile/MOBILE_APP_SITEMAP_V1.md)
- [Mobile Design System](../../docs/design/mobile/MOBILE_DESIGN_SYSTEM_V1.md)
- [Component catalog](../../docs/design/mobile/MOBILE_COMPONENT_CATALOG_V1.md)
- [Screen patterns](../../docs/design/mobile/MOBILE_SCREEN_PATTERNS_V1.md)
- [Design tokens](../../docs/design/mobile/tokens/mobile_design_tokens_v1.json)
- [Mandatory build compliance harness](../../docs/design/mobile/MOBILE_BUILD_HARNESS_V1.md)

When these documents conflict with the distributed-runtime plan, the
distributed-runtime plan remains authoritative.

Before adding a Flutter scaffold or implementation code, follow
[`AGENTS.md`](./AGENTS.md) and run:

```text
python scripts/ci/validate_mobile_build_contracts.py
```
