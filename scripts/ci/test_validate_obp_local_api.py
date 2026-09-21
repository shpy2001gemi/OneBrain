"""Adversarial transport proposal checks, not runtime implementation tests."""
import copy
import unittest

from scripts.ci.validate_obp_local_api import (
    ObpApiContractError, load_profile, strict_json, validate_contract, validate_example,
)


class ObpLocalApiContractTests(unittest.TestCase):
    def setUp(self):
        self.profile = load_profile()

    def reject(self, path, value):
        p = copy.deepcopy(self.profile)
        target = p
        for key in path[:-1]:
            target = target[key]
        target[path[-1]] = value
        with self.assertRaises(ObpApiContractError):
            validate_contract(p)

    def test_full_operation_mapping_and_examples(self):
        self.assertEqual(validate_contract(), (6, 13, 35))

    def test_no_implicit_implementation_acceptance(self):
        self.reject(("handlers_registered",), False)
        self.reject(("owner_acceptance",), "D-028")

    def test_no_bearer_to_management_escalation(self):
        self.reject(("authentication", "management_mint_route"), True)
        self.reject(("authentication", "default_control_grant"), True)
        self.reject(("operations", 1, "access"), "product_read")

    def test_no_raw_intake_transport(self):
        self.reject(("authentication", "raw_upload_route"), True)
        self.reject(("authentication", "host_intake"), "filesystem_path")

    def test_ws_endpoint_and_queue_isolation(self):
        self.reject(("ws", "ticket_endpoint_bound"), False)
        self.reject(("ws", "broadcast"), True)
        self.reject(("ws", "shared_hub_limits", "queue_frames"), 33)
        self.reject(("ws", "topics"), ["network", "runtime"])

    def test_ws_private_fields_and_false_claims(self):
        fields = self.profile["ws"]["network_fields"] | {"expected_peer": "NodeID"}
        self.reject(("ws", "network_fields"), fields)
        self.reject(("ws", "network_fields", "authorizes_reward"), "Boolean")

    def test_unknown_outcome_never_becomes_safe_retry(self):
        index = next(i for i, e in enumerate(self.profile["errors"])
                     if e["reason"] == "outcome_unknown")
        self.reject(("errors", index, "reconcile_before_retry"), False)
        self.reject(("errors", index, "outcome"), "not_admitted")

    def test_budgets_and_frozen_inputs(self):
        self.reject(("limits", "concurrent_commands"), 5)
        name = next(iter(self.profile["frozen_inputs"]))
        self.reject(("frozen_inputs", name), "0" * 64)

    def test_duplicate_keys_at_root_and_nested(self):
        for raw in [b'{"operation":1,"operation":2}', b'{"payload":{"x":1,"x":2}}']:
            with self.subTest(raw=raw), self.assertRaises(ObpApiContractError):
                strict_json(raw)

    def test_unbounded_or_non_json_values(self):
        for raw in [b'{"x":NaN}', b'{"x":Infinity}', b'{"x":-Infinity}', b'{"x":1e999}',
                    b'[]', b'\xff', b'{"x":' + b'[' * 18 + b'0' + b']' * 18 + b'}',
                    b' ' * 1_048_577]:
            with self.subTest(prefix=raw[:40]), self.assertRaises(ObpApiContractError):
                strict_json(raw)

    def test_boolean_generation_and_uppercase_id_rejected(self):
        original = next(f["value"] for f in self.profile["fixtures"] if f["name"] == "configure")
        for field, value in [("expected_generation", True), ("idempotency_key", "AA" * 32)]:
            command = copy.deepcopy(original)
            command["payload"][field] = value
            with self.subTest(field=field), self.assertRaises(ObpApiContractError):
                validate_example(self.profile, "commands", command)

    def test_redacted_event_cannot_invent_active_state(self):
        event = copy.deepcopy(next(f["value"] for f in self.profile["fixtures"]
                                   if f["name"] == "redacted_snapshot"))
        event["data"]["active"] = True
        with self.assertRaises(ObpApiContractError):
            validate_example(self.profile, "event", event)

    def test_unknown_outcome_cannot_include_result_or_disable_reconcile(self):
        outcome = dict(idempotency_key="33" * 32, operation="refresh",
                       state="reconcile_required", reconcile_before_retry=True)
        validate_example(self.profile, "outcome", outcome)
        for update in [{"result": {}}, {"reconcile_before_retry": False}]:
            with self.subTest(update=update), self.assertRaises(ObpApiContractError):
                validate_example(self.profile, "outcome", outcome | update)


if __name__ == "__main__":
    unittest.main()
