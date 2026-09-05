import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export async function sha256File(filePath) {
  const hash = createHash('sha256');
  await new Promise((resolve, reject) => {
    const stream = createReadStream(filePath);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.on('error', reject);
    stream.on('end', resolve);
  });
  return hash.digest('hex');
}

export async function writeSha256Sums(outputPath, files) {
  const lines = [];
  for (const file of files) lines.push(`${await sha256File(file)}  ${path.basename(file)}`);
  await writeFile(outputPath, `${lines.join('\n')}\n`, 'ascii');
  return lines;
}

const isCli = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isCli) {
  const [outputPath, ...files] = process.argv.slice(2);
  if (!outputPath || !files.length) throw new Error('usage: write-sha256sums <output> <file>...');
  for (const line of await writeSha256Sums(outputPath, files)) console.log(line);
}
