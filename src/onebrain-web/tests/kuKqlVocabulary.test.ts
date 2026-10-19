// @vitest-environment node
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  KQL_CONTAINS_FIELDS,
  KQL_EXAMPLE,
  KQL_FIELDS,
  KQL_KEYWORDS,
} from '../src/components/kqlVocabulary';

// ARCH-04: the Web KQL editor must only suggest what the frozen, read-only
// parser accepts. The Rust gate is the source of truth.
const readOnlyRs = readFileSync(
  fileURLToPath(new URL('../../ku-kql/src/read_only.rs', import.meta.url)),
  'utf8',
);

function rustStringArray(name: string): string[] {
  const match = readOnlyRs.match(new RegExp(`pub const ${name}: &\\[&str\\] = &\\[([\\s\\S]*?)\\];`));
  if (!match) throw new Error(`${name} not found in read_only.rs`);
  return [...match[1].matchAll(/"((?:[^"\\]|\\.)*)"/g)].map(m => m[1].replace(/\\"/g, '"'));
}

describe('legacy KQL editor vocabulary', () => {
  it('suggests no SQL-only keywords or write statements', () => {
    for (const forbidden of ['SELECT', 'FROM', 'JOIN', 'GROUP', 'HAVING', 'LIKE', 'CREATE', 'UPDATE', 'DEPRECATE', 'WATCH']) {
      expect(KQL_KEYWORDS as readonly string[]).not.toContain(forbidden);
    }
  });

  it('matches the Rust read-only field lists', () => {
    expect([...KQL_FIELDS]).toEqual(rustStringArray('READ_ONLY_FIELDS'));
    expect([...KQL_CONTAINS_FIELDS]).toEqual(rustStringArray('CONTAINS_FIELDS'));
  });

  it('uses an example the Rust tests parse and run', () => {
    expect(rustStringArray('DOCUMENTED_EXAMPLES')).toContain(KQL_EXAMPLE);
  });
});
