"""Shared paths, error type and reader for the vNext contract validators."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
VNEXT = ROOT / "docs/specs/vnext"
DR_M5_TRANSACTION_INVENTORY = (
    VNEXT / "DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md"
)
VNEXT_FOUNDATION_WORKFLOW = ROOT / ".github/workflows/vnext-foundation.yml"


class ContractError(RuntimeError):
    pass


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise ContractError(f"cannot read {path.relative_to(ROOT)}: {error}") from error
