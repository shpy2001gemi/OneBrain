"""Registration cannot drift independently of the approved product declaration."""
import copy
import json
import unittest

from scripts.ci.validate_ku_registration import ROOT, validate_registration


class KuRegistrationTests(unittest.TestCase):
    def setUp(self):
        self.ku = json.loads((ROOT / 'src/test-vectors/vnext/ku-product-workflow-v1.json').read_text(encoding='utf-8'))
        self.base = json.loads((ROOT / 'src/test-vectors/vnext/base-v1-runtime-interface-v1.json').read_text(encoding='utf-8'))

    def test_registered_pair(self):
        validate_registration(self.ku, self.base)

    def test_payload_discriminator_dto_error_and_version_drift(self):
        changes = [lambda b: b['ku_payloads']['operations'][0].update(wire_id=1),
                   lambda b: b['ku_payloads']['dto_ids'].update(KuPrepareV1=1),
                   lambda b: b['ku_payloads']['errors'][0].update(retryable=True),
                   lambda b: b['profile_version'].update(minor=1),
                   lambda b: b['ku_payloads']['dtos']['KuPrepareV1']['optional'].update(authorized='Boolean')]
        for change in changes:
            with self.subTest(change=change):
                base=copy.deepcopy(self.base); change(base)
                with self.assertRaises(ValueError): validate_registration(self.ku, base)

    def test_golden_binding_drift(self):
        self.ku['registration']['golden_sha256']='00'*32
        with self.assertRaises(ValueError): validate_registration(self.ku, self.base)


if __name__ == '__main__':
    unittest.main()
