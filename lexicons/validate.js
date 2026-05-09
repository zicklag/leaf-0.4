import { readdirSync, readFileSync } from 'node:fs';
import { join, basename } from 'node:path';
import { isValidLexiconDoc, parseLexiconDoc } from '@atproto/lexicon';
import { fileURLToPath } from 'node:url';

const dir = join(fileURLToPath(import.meta.url), '..', 'town', 'muni');

function *walk(root) {
  for (const e of readdirSync(root, { withFileTypes: true })) {
    const p = join(root, e.name);
    if (e.isDirectory()) yield *walk(p);
    else if (e.isFile() && e.name.endsWith('.json')) yield p;
  }
}

let ok = 0, fail = 0;

for (const path of walk(dir)) {
  const name = basename(path);
  const doc = JSON.parse(readFileSync(path, 'utf8'));

  if (name.replace('.json', '') !== doc.id.split('.').pop()) {
    console.error(`FAIL  ${name}  — id "${doc.id}" mismatch`);
    fail++;
    continue;
  }

  try {
    parseLexiconDoc(doc);
    if (!isValidLexiconDoc(doc)) throw 'isValidLexiconDoc returned false';
    console.log(`OK    ${name}`);
    ok++;
  } catch (e) {
    console.error(`FAIL  ${name}  — ${e}`);
    fail++;
  }
}

console.log(`\n${ok + fail} files — ${ok} passed, ${fail} failed`);
process.exit(fail > 0 ? 1 : 0);
