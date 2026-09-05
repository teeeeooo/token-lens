import { gzipSync } from 'node:zlib';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const PORTABLE_FOOTER_MAGIC = Buffer.from('TLTS0001', 'ascii');
export const PORTABLE_FOOTER_LENGTH = 24;

export function parsePortableFooter(buffer) {
  if (buffer.length < PORTABLE_FOOTER_LENGTH) return null;
  const footerOffset = buffer.length - PORTABLE_FOOTER_LENGTH;
  if (!buffer.subarray(footerOffset, footerOffset + 8).equals(PORTABLE_FOOTER_MAGIC)) return null;

  const compressedLength = Number(buffer.readBigUInt64LE(footerOffset + 8));
  const rawLength = Number(buffer.readBigUInt64LE(footerOffset + 16));
  const payloadOffset = footerOffset - compressedLength;
  if (!Number.isSafeInteger(compressedLength) || !Number.isSafeInteger(rawLength)
      || compressedLength <= 0 || rawLength <= 0 || payloadOffset < 0) {
    throw new Error('invalid Token Lens portable footer');
  }
  return { footerOffset, payloadOffset, compressedLength, rawLength };
}
export function createPortableImage(appBuffer, sidecarBuffer) {
  if (!appBuffer?.length) throw new Error('app executable is empty');
  if (!sidecarBuffer?.length) throw new Error('tokScale sidecar is empty');

  const compressed = gzipSync(sidecarBuffer, { level: 9 });
  const footer = Buffer.alloc(PORTABLE_FOOTER_LENGTH);
  PORTABLE_FOOTER_MAGIC.copy(footer, 0);
  footer.writeBigUInt64LE(BigInt(compressed.length), 8);
  footer.writeBigUInt64LE(BigInt(sidecarBuffer.length), 16);
  return Buffer.concat([appBuffer, compressed, footer]);
}

export async function buildPortableExe(appPath, sidecarPath, outputPath) {
  const [appBuffer, sidecarBuffer] = await Promise.all([readFile(appPath), readFile(sidecarPath)]);
  const portable = createPortableImage(appBuffer, sidecarBuffer);
  await writeFile(outputPath, portable);
  const footer = parsePortableFooter(portable);
  return {
    outputPath,
    appLength: appBuffer.length,
    sidecarLength: sidecarBuffer.length,
    compressedLength: footer.compressedLength,
    totalLength: portable.length,
  };
}
const isCli = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isCli) {
  const [appPath, sidecarPath, outputPath] = process.argv.slice(2);
  if (!appPath || !sidecarPath || !outputPath) {
    throw new Error('usage: build-portable-exe <app.exe> <tokscale.exe> <output.exe>');
  }
  const result = await buildPortableExe(appPath, sidecarPath, outputPath);
  console.log(
    `Portable EXE: ${result.outputPath} `
    + `(app=${result.appLength}, tokscale=${result.sidecarLength}, compressed=${result.compressedLength})`,
  );
}
