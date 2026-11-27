"""Private metadata ledger for qualification harness assembly, with no dispatch.

Stores an immutable planned matrix, call reservations and terminal failures.
Contains no source, labels, prompts or output bodies. Native encrypted Attempt
and KU journals remain authoritative for actual execution/preparation. This
ledger cannot authenticate a run lock, authorize a provider or qualify a tuple.
"""
from __future__ import annotations

from contextlib import contextmanager
import json
import os
from pathlib import Path
import sqlite3
import time

from scripts.encoder.qualification_package_audit import ROOT, BUNDLE, is_digest, local_path, require, sha256

PROFILE_BYTES = (ROOT / BUNDLE / "profile.json").read_bytes()
PROFILE = json.loads(PROFILE_BYTES)
TERMINALS = frozenset({"reported_complete", "incomplete", "unsupported", "abstained",
                       "timeout", "canceled", "oom", "process_death", "interrupted",
                       "failed", "admission_denied"})


def encode(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def integer(value, code):
    require(type(value) is int and value >= 0, code)
    return value


def plan_rows(plan):
    require(set(plan) == {"kind", "package_sha256", "profile_sha256", "sources", "tuples", "repeats"},
            "invalid_plan_fields")
    require(plan["kind"] == "ku-enc-003-attempt-plan/1" and is_digest(plan["package_sha256"]),
            "invalid_plan_binding")
    require(plan["profile_sha256"] == sha256(PROFILE_BYTES), "profile_binding_mismatch")
    require(type(plan["repeats"]) is int and plan["repeats"] == PROFILE["qualification"]["repeats_per_source"],
            "repeat_count_mismatch")
    sources, tuples = plan["sources"], plan["tuples"]
    require(isinstance(sources, list) and 0 < len(sources) <= 4096 and
            isinstance(tuples, list) and 0 < len(tuples) <= 32, "plan_bounds")
    seen = set()
    for source in sources:
        require(set(source) == {"source_id_sha256", "utf8_sha256", "language"}, "invalid_source_fields")
        key = source["source_id_sha256"]
        require(is_digest(key) and key not in seen and is_digest(source["utf8_sha256"]) and
                source["language"] in ("vi", "en"), "invalid_plan_source")
        seen.add(key)
    seen = set()
    for item in tuples:
        require(set(item) == {"tuple_sha256", "resource_profile"}, "invalid_tuple_fields")
        key = item["tuple_sha256"]
        require(is_digest(key) and key not in seen and
                item["resource_profile"] in PROFILE["resource_profiles"], "invalid_plan_tuple")
        seen.add(key)
    result = []
    for source in sources:
        for item in tuples:
            for repeat in range(1, plan["repeats"] + 1):
                binding = {**source, **item, "repeat": repeat, "package_sha256": plan["package_sha256"]}
                result.append((sha256(encode(binding).encode()), binding))
    return result


class AttemptLedger:
    """Metadata accounting only; reservation is not permission to execute."""

    @classmethod
    def create(cls, path, plan, root=ROOT):
        path = local_path(path)
        require(not path.is_relative_to(local_path(root)), "private_ledger_required")
        rows = plan_rows(plan)
        # Caller supplies a private preparation directory. No file replacement.
        fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        os.close(fd)
        ledger = cls(path, sha256(encode(plan).encode()))
        try:
            with ledger.transaction():
                ledger.db.execute("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
                ledger.db.executemany("INSERT INTO meta VALUES (?, ?)",
                                      [("plan", encode(plan)), ("plan_sha256", ledger.plan_digest)])
                ledger.db.execute("""CREATE TABLE rows (
                    id TEXT PRIMARY KEY, binding TEXT NOT NULL, state TEXT NOT NULL,
                    generation TEXT, started_ns INTEGER, calls INTEGER NOT NULL DEFAULT 0,
                    input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
                    work INTEGER NOT NULL DEFAULT 0, elapsed_ms INTEGER,
                    evidence_sha256 TEXT, worker_stopped INTEGER NOT NULL DEFAULT 0)""")
                ledger.db.execute("""CREATE TABLE calls (
                    row_id TEXT NOT NULL REFERENCES rows(id), seq INTEGER NOT NULL,
                    context_sha256 TEXT NOT NULL, input_tokens INTEGER NOT NULL,
                    output_tokens INTEGER NOT NULL, kind TEXT NOT NULL,
                    returned INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY(row_id, seq))""")
                ledger.db.executemany("INSERT INTO rows(id,binding,state) VALUES (?,?,'pending')",
                                      [(key, encode(value)) for key, value in rows])
        except BaseException:
            ledger.close()
            raise
        return ledger

    def __init__(self, path, plan_digest, clock=time.monotonic_ns):
        require(is_digest(plan_digest), "invalid_plan_digest")
        self.plan_digest, self.clock = plan_digest, clock
        path = local_path(path)
        require(path.is_file(), "ledger_missing")
        self.db = sqlite3.connect(path.as_uri() + "?mode=rw", uri=True, isolation_level=None, timeout=5)
        self.db.row_factory = sqlite3.Row
        self.db.execute("PRAGMA synchronous=FULL")
        self.db.execute("PRAGMA foreign_keys=ON")

    def close(self):
        self.db.close()

    @contextmanager
    def transaction(self):
        self.db.execute("BEGIN IMMEDIATE")
        try:
            yield
            self.db.execute("COMMIT")
        except BaseException:
            self.db.execute("ROLLBACK")
            raise

    def checked_plan(self):
        values = dict(self.db.execute("SELECT key,value FROM meta"))
        require(values.get("plan_sha256") == self.plan_digest and
                sha256(values["plan"].encode()) == self.plan_digest, "ledger_plan_mismatch")
        plan = json.loads(values["plan"])
        expected = dict(plan_rows(plan))
        actual = dict(self.db.execute("SELECT id,binding FROM rows"))
        require(actual == {key: encode(value) for key, value in expected.items()}, "ledger_matrix_mismatch")
        return plan

    def row(self, row_id):
        require(is_digest(row_id), "invalid_row_id")
        row = self.db.execute("SELECT * FROM rows WHERE id=?", (row_id,)).fetchone()
        require(row is not None, "unknown_row")
        return row

    def start(self, row_id, generation):
        require(is_digest(generation), "invalid_generation")
        with self.transaction():
            self.checked_plan()
            row = self.row(row_id)
            require(row["state"] == "pending", "row_already_started")
            # An unconfirmed stopped worker fences all subsequent rows, including
            # after recovery. Terminal status alone does not prove worker death.
            busy = self.db.execute("SELECT count(*) FROM rows WHERE state!='pending' AND worker_stopped=0").fetchone()[0]
            require(busy == 0, "worker_stop_confirmation_required")
            self.db.execute("UPDATE rows SET state='active',generation=?,started_ns=? WHERE id=?",
                            (generation, self.clock(), row_id))

    def active(self, row_id, generation):
        row = self.row(row_id)
        require(row["state"] == "active" and row["generation"] == generation, "stale_or_terminal_callback")
        elapsed = (self.clock() - row["started_ns"]) // 1_000_000
        limits = PROFILE["resource_profiles"][json.loads(row["binding"])["resource_profile"]]
        require(0 <= elapsed < limits["deadline_ms"], "deadline_exhausted")
        return row, limits, elapsed

    def reserve_call(self, row_id, generation, context, input_tokens, output_tokens, kind="extract"):
        require(is_digest(context) and kind in ("extract", "repair_or_disambiguate", "warmup", "schema_probe", "ablation"),
                "invalid_call_binding")
        integer(input_tokens, "invalid_input_tokens")
        integer(output_tokens, "invalid_output_tokens")
        with self.transaction():
            self.checked_plan()
            row, limits, _ = self.active(row_id, generation)
            require(not self.db.execute("SELECT 1 FROM calls WHERE row_id=? AND returned=0", (row_id,)).fetchone(),
                    "call_already_active")
            require(0 < input_tokens <= limits["call_input_tokens"] and
                    0 < output_tokens <= limits["call_output_tokens"], "call_tokens_exceeded")
            count = self.db.execute("SELECT count(*) FROM calls WHERE row_id=? AND context_sha256=?", (row_id, context)).fetchone()[0]
            require(count < PROFILE["max_context_calls"] and row["calls"] < limits["job_calls"], "call_budget_exceeded")
            require(row["input_tokens"] + input_tokens <= limits["job_input_tokens"] and
                    row["output_tokens"] + output_tokens <= limits["job_output_tokens"], "aggregate_tokens_exceeded")
            seq = row["calls"] + 1
            self.db.execute("INSERT INTO calls(row_id,seq,context_sha256,input_tokens,output_tokens,kind) VALUES(?,?,?,?,?,?)",
                            (row_id, seq, context, input_tokens, output_tokens, kind))
            self.db.execute("UPDATE rows SET calls=?,input_tokens=input_tokens+?,output_tokens=output_tokens+? WHERE id=?",
                            (seq, input_tokens, output_tokens, row_id))
        return seq

    def call_returned(self, row_id, generation, seq):
        integer(seq, "invalid_call_sequence")
        with self.transaction():
            self.checked_plan()
            self.active(row_id, generation)
            require(self.db.execute("UPDATE calls SET returned=1 WHERE row_id=? AND seq=? AND returned=0",
                                    (row_id, seq)).rowcount == 1, "call_return_conflict")

    def charge_work(self, row_id, generation, amount):
        integer(amount, "invalid_work")
        with self.transaction():
            self.checked_plan()
            row, _, _ = self.active(row_id, generation)
            require(row["work"] + amount <= PROFILE["max_job_work_units"], "work_budget_exceeded")
            self.db.execute("UPDATE rows SET work=work+? WHERE id=?", (amount, row_id))

    def finish(self, row_id, generation, outcome, evidence_sha256):
        require(outcome in TERMINALS and is_digest(evidence_sha256), "invalid_terminal_evidence")
        with self.transaction():
            self.checked_plan()
            row = self.row(row_id)
            require(row["state"] == "active" and row["generation"] == generation, "stale_or_terminal_callback")
            elapsed = (self.clock() - row["started_ns"]) // 1_000_000
            deadline = PROFILE["resource_profiles"][json.loads(row["binding"])["resource_profile"]]["deadline_ms"]
            if not 0 <= elapsed < deadline:
                # Late success/abstention is retained as timeout, never accepted.
                outcome = "timeout"
            if outcome == "reported_complete":
                require(not self.db.execute("SELECT 1 FROM calls WHERE row_id=? AND returned=0", (row_id,)).fetchone(),
                        "completion_before_call_return")
            self.db.execute("UPDATE rows SET state=?,elapsed_ms=?,evidence_sha256=? WHERE id=?",
                            (outcome, max(elapsed, 0), evidence_sha256, row_id))
        return outcome

    def confirm_worker_stopped(self, row_id, evidence_sha256):
        """Record collector evidence reference, not verification of that evidence."""
        require(is_digest(evidence_sha256), "invalid_stop_evidence")
        with self.transaction():
            self.checked_plan()
            require(self.row(row_id)["state"] in TERMINALS, "terminal_required")
            # Preserve a separate immutable stop receipt; do not overwrite output.
            self.db.execute("CREATE TABLE IF NOT EXISTS stops (row_id TEXT PRIMARY KEY, evidence TEXT NOT NULL)")
            old = self.db.execute("SELECT evidence FROM stops WHERE row_id=?", (row_id,)).fetchone()
            require(old is None or old[0] == evidence_sha256, "stop_receipt_conflict")
            self.db.execute("INSERT OR IGNORE INTO stops VALUES (?,?)", (row_id, evidence_sha256))
            self.db.execute("UPDATE rows SET worker_stopped=1 WHERE id=?", (row_id,))

    def recover(self, new_generation):
        """Exhaust interrupted inference; never renew elapsed time or reservations."""
        require(is_digest(new_generation), "invalid_generation")
        with self.transaction():
            self.checked_plan()
            require(not self.db.execute("SELECT 1 FROM rows WHERE state='active' AND generation=?", (new_generation,)).fetchone(),
                    "fresh_generation_required")
            count = self.db.execute("UPDATE rows SET state='interrupted',elapsed_ms=NULL WHERE state='active'").rowcount
        return count

    def snapshot(self):
        # One read transaction protects matrix/counts against concurrent updates.
        with self.transaction():
            self.checked_plan()
            rows = [dict(row) for row in self.db.execute("SELECT * FROM rows ORDER BY id")]
        return {"kind": "ku-enc-003-attempt-ledger-snapshot/1", "plan_sha256": self.plan_digest,
                "execution_admitted": False, "model_qualified": False,
                "planned_rows": len(rows), "rows": rows}
