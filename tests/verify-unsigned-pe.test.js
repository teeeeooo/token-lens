import assert from 'node:assert/strict';
import test from 'node:test';
import { peHasCertificateTable, peSubsystem } from '../scripts/verify-unsigned-pe.mjs';

function peFixture({ signed = false, pe32Plus = true, subsystem = 2 } = {}) {
  const buffer = Buffer.alloc(512);
  buffer.write('MZ', 0, 'ascii');
  buffer.writeUInt32LE(0x80, 0x3c);
  buffer.write('PE\0\0', 0x80, 'ascii');
  const optional = 0x80 + 24;
  buffer.writeUInt16LE(pe32Plus ? 0x20b : 0x10b, optional);
  buffer.writeUInt16LE(subsystem, optional + 68);
  const dataDirectory = optional + (pe32Plus ? 112 : 96);
  const security = dataDirectory + (4 * 8);
  if (signed) {
    buffer.writeUInt32LE(0x180, security);
    buffer.writeUInt32LE(64, security + 4);
  }
  return buffer;
}

test('PE certificate table detection accepts unsigned PE32 and PE32+', () => {
  assert.equal(peHasCertificateTable(peFixture({ pe32Plus: false })), false);
  assert.equal(peHasCertificateTable(peFixture({ pe32Plus: true })), false);
});
test('PE certificate table detection rejects a populated Authenticode directory', () => {
  assert.equal(peHasCertificateTable(peFixture({ signed: true })), true);
});

test('PE subsystem parsing distinguishes GUI from console executables', () => {
  assert.equal(peSubsystem(peFixture({ subsystem: 2 })), 2);
  assert.equal(peSubsystem(peFixture({ subsystem: 3 })), 3);
});

test('PE helpers reject non-PE input', () => {
  assert.throws(() => peHasCertificateTable(Buffer.alloc(512)), /valid PE/);
  assert.throws(() => peSubsystem(Buffer.alloc(512)), /valid PE/);
});
