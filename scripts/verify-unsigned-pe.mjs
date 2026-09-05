import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function peOptionalOffset(buffer) {
  if (buffer.length < 0x100 || buffer.toString('ascii', 0, 2) !== 'MZ') {
    throw new Error('not a valid PE image');
  }
  const peOffset = buffer.readUInt32LE(0x3c);
  if (peOffset + 24 > buffer.length || buffer.toString('ascii', peOffset, peOffset + 4) !== 'PE\0\0') {
    throw new Error('missing PE signature');
  }
  return peOffset + 24;
}

export function peSubsystem(buffer) {
  const optionalOffset = peOptionalOffset(buffer);
  if (optionalOffset + 70 > buffer.length) throw new Error('truncated PE optional header');
  return buffer.readUInt16LE(optionalOffset + 68);
}

export function peHasCertificateTable(buffer) {
  const optionalOffset = peOptionalOffset(buffer);
  const magic = buffer.readUInt16LE(optionalOffset);
  const dataDirectoryOffset = magic === 0x10b ? optionalOffset + 96
    : magic === 0x20b ? optionalOffset + 112
      : null;
  if (dataDirectoryOffset == null || dataDirectoryOffset + 40 > buffer.length) {
    throw new Error('unsupported or truncated PE optional header');
  }
  const securityEntry = dataDirectoryOffset + (4 * 8);
  const certificateOffset = buffer.readUInt32LE(securityEntry);
  const certificateSize = buffer.readUInt32LE(securityEntry + 4);
  return certificateOffset !== 0 && certificateSize !== 0;
}

export async function assertUnsignedGuiPe(filePath) {
  const buffer = await readFile(filePath);
  if (peHasCertificateTable(buffer)) {
    throw new Error(`unexpected Authenticode certificate table: ${filePath}`);
  }
  if (peSubsystem(buffer) !== 2) {
    throw new Error(`expected Windows GUI subsystem: ${filePath}`);
  }
}

const isCli = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isCli) {
  const files = process.argv.slice(2);
  if (!files.length) throw new Error('at least one PE path is required');
  for (const file of files) {
    await assertUnsignedGuiPe(file);
    console.log(`Unsigned GUI PE verified: ${file}`);
  }
}
