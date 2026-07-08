# OBS Storage Research — Pillar 8

Comprehensive research foundation for the OneBrain Storage (OBS) layer.

## Documents

| # | Document | Topic | Key Decision |
|---|----------|-------|--------------|
| 00 | [Storage Analysis](00_obs_storage_analysis.md) | Gap analysis & inventory | 40+ gaps identified across 10 categories |
| 01 | [Distributed Storage](01_distributed_storage_research.md) | Replication strategy | R=7, CRDT consistency, stigmergy repair |
| 02 | [Storage Benchmark](02_storage_architecture_benchmark.md) | Backend evaluation | Keep redb — optimal for workload |
| 03 | [Hot/Cold Tiering](03_hot_cold_tiering_research.md) | Cache architecture | M-ARC, 10K capacity, gossip invalidation |
| 04 | [Schema Migration](04_schema_migration_research.md) | Versioning strategy | Never migrate wire bytes (CID stability) |
| 05 | [Media/Blob Storage](05_media_blob_storage_research.md) | Blob design | 256KB chunks, OB-CID, RS erasure coding |
| 06 | [IPFS Integration](06_ipfs_integration_research.md) | External storage | Self-contained core + optional bridge |

## Related Spec
- [OBS Technical Specification](../../specs/OBS_SPEC.md)

## Research Timeline
- **2026-07-06**: Storage analysis + Topics 1-6 completed
- **2026-07-07**: Implementation completed (4 phases, 1,184 tests)
