import assert from 'node:assert/strict';
import test from 'node:test';

import { deriveVNextConfirmationReceipt } from '../src/api/vnextReceipt.ts';

test('interaction receipt matches the frozen Rust/CLI BLAKE3 framing', () => {
  assert.equal(
    deriveVNextConfirmationReceipt('11'.repeat(32)),
    'obc1.Xty15PTowg0NrRClYO_Oqp3UAyS1HOQx_-N8DwH1Rh4',
  );
  assert.equal(
    deriveVNextConfirmationReceipt('41'.repeat(32)),
    'obc1.Oo1te0JXXyKLZ2BHG858E3eeOJ6YcT2Y63pHVN_-850',
  );
});

test('interaction receipt rejects non-canonical identifiers', () => {
  assert.throws(
    () => deriveVNextConfirmationReceipt('AA'.repeat(32)),
    /64 lowercase hexadecimal/u,
  );
  assert.throws(
    () => deriveVNextConfirmationReceipt('11'.repeat(31)),
    /64 lowercase hexadecimal/u,
  );
});
