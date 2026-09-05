import assert from 'node:assert/strict';
import test from 'node:test';
import { gunzipSync } from 'node:zlib';
import {
  PORTABLE_FOOTER_LENGTH,
  createPortableImage,
  parsePortableFooter,
} from '../scripts/build-portable-exe.mjs';

test('single-EXE portable image embeds only the compressed tokScale payload', () => {
  const app = Buffer.from('MZ-token-lens-app');
  const sidecar = Buffer.from('tokscale-sidecar-payload'.repeat(64));
  const portable = createPortableImage(app, sidecar);
  const footer = parsePortableFooter(portable);

  assert.ok(footer);
  assert.deepEqual(portable.subarray(0, app.length), app);
  assert.equal(footer.payloadOffset, app.length);
  assert.equal(footer.rawLength, sidecar.length);
  assert.equal(footer.footerOffset + PORTABLE_FOOTER_LENGTH, portable.length);
  const compressed = portable.subarray(footer.payloadOffset, footer.footerOffset);
  assert.deepEqual(gunzipSync(compressed), sidecar);
});
