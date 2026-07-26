from __future__ import annotations

import copy
import json
import unittest

from scripts.ci.validate_vnext_contracts import (
    ContractError,
    PRIVATE_WS_PROFILE,
    validate_private_websocket_profile,
)


def frozen_profile() -> dict[str, object]:
    return json.loads(PRIVATE_WS_PROFILE.read_text(encoding="utf-8"))


class PrivateWebSocketProfileTests(unittest.TestCase):
    def test_frozen_profile_is_accepted(self) -> None:
        events, topics = validate_private_websocket_profile(frozen_profile())
        self.assertEqual(events, 10)
        self.assertEqual(topics, 4)

    def test_ticket_mint_cannot_lose_bearer_authentication(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["routes"][0]["authentication"] = "none"
        with self.assertRaisesRegex(ContractError, "route inventory"):
            validate_private_websocket_profile(profile)

    def test_subscription_cannot_become_mutable_after_upgrade(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["ticket"]["subscription_immutable"] = False
        with self.assertRaisesRegex(ContractError, "ticket contract"):
            validate_private_websocket_profile(profile)

    def test_cross_client_delivery_cannot_be_enabled(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["semantic_firewalls"]["cross_client_delivery"] = True
        with self.assertRaisesRegex(ContractError, "semantic firewall"):
            validate_private_websocket_profile(profile)

    def test_private_need_identifier_cannot_be_exported(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["event_non_exportable_fields"].remove("standing_need_id")
        with self.assertRaisesRegex(ContractError, "non-exportable"):
            validate_private_websocket_profile(profile)

    def test_delivered_event_requires_a_real_acknowledgement(self) -> None:
        profile = copy.deepcopy(frozen_profile())
        profile["publication_states"][
            "delivered_requires_durable_authenticated_acknowledgement"
        ] = False
        with self.assertRaisesRegex(ContractError, "publication state"):
            validate_private_websocket_profile(profile)


if __name__ == "__main__":
    unittest.main()
