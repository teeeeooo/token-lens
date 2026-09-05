import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { sha256File, writeSha256Sums } from '../scripts/write-sha256sums.mjs';

test('sha256 writer produces stable lowercase artifact lines', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'token-lens-sha-'));
  const artifact = path.join(root, 'artifact.bin');
  const output = path.join(root, 'SHA256SUMS.txt');
  await writeFile(artifact, 'abc');
  try {
    assert.equal(await sha256File(artifact), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
    const lines = await writeSha256Sums(output, [artifact]);
    assert.deepEqual(lines, [`ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  artifact.bin`]);
    assert.equal(await readFile(output, 'ascii'), `${lines[0]}\n`);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
