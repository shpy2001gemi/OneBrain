"""Explicit secret provisioning for a new local KU dataset; standard library only."""

import argparse
import os
from pathlib import Path
import secrets
import sys


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]


class SetupError(Exception):
    """Operator guidance that does not expose paths or secret bytes."""


def _write_secret(path: Path, content: bytes) -> None:
    # Exclusive creation; POSIX files are owner-only. Windows inherits the
    # operator-controlled parent's ACL, which must already restrict access.
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as handle:
        handle.write(content)


def prepare(data_dir: Path, output_dir: Path) -> None:
    """Create a fresh pair without reading/replacing any existing custody."""
    try:
        if os.path.lexists(data_dir):
            raise SetupError(
                "ku_local_secrets_dataset_exists: data_dir must not exist, even if empty. "
                "For an existing dataset, retain its original Vault key; do not replace it."
            )
        if os.path.lexists(output_dir):
            raise SetupError(
                "ku_local_secrets_output_exists: retain existing custody. "
                "Choose a new output_dir for a new dataset; no overwrite or resume."
            )
        data_dir = data_dir.resolve()
        output_dir = output_dir.resolve()
        if any(path.is_relative_to(REPOSITORY_ROOT) for path in (data_dir, output_dir)):
            raise SetupError(
                "ku_local_secrets_repository_path: keep data_dir and output_dir outside "
                "the repository in an operator-controlled private location."
            )
        if data_dir.is_relative_to(output_dir) or output_dir.is_relative_to(data_dir):
            raise SetupError(
                "ku_local_secrets_path_overlap: use separate, non-overlapping "
                "data_dir and output_dir paths."
            )
        if not output_dir.parent.is_dir():
            raise SetupError(
                "ku_local_secrets_parent_unavailable: prepare an existing private "
                "output_dir parent with restricted local access before running this command."
            )
        key = secrets.token_bytes(32)
        token = secrets.token_hex(32).encode("ascii")
        output_dir.mkdir(mode=0o700 if os.name != "nt" else 0o777)
        _write_secret(output_dir / "vault.key", key)
        _write_secret(output_dir / "api-token.txt", token)
    except OSError:
        raise SetupError(
            "ku_local_secrets_write_failed: check local permissions and available space. "
            "Retain any partial output; do not configure the host from a failed attempt. "
            "Retry with a new output_dir. No cleanup or resume is performed."
        ) from None


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        description="Create Vault key/token files for an explicitly new local KU dataset. "
        "Never use this to recover an existing dataset."
    )
    parser.add_argument("--data-dir", type=Path, required=True,
                        help="Intended new host data_dir, outside Git; must not exist.")
    parser.add_argument("--output-dir", type=Path, required=True,
                        help="New custody directory under an existing private parent, outside Git.")
    args = parser.parse_args(argv)
    try:
        prepare(args.data_dir, args.output_dir)
    except SetupError as error:
        print(error, file=sys.stderr)
        return 1
    print(
        "ku_local_secrets_ready: created vault.key and api-token.txt in output_dir. "
        "Set vault_key_file and api_token_file in your host config. "
        "Keep this pair private and retain the same Vault key and data_dir on restart."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
