import { execFile } from 'node:child_process';
import { chmod, copyFile, lstat, mkdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';

const execFileAsync = promisify(execFile);

export const TOKSCALE_VERSION = '4.15.1';

const SPECS = Object.freeze({
  'darwin:arm64': Object.freeze({
    packageName: '@tokscale/cli-darwin-arm64',
    triple: 'aarch64-apple-darwin',
    platform: 'darwin', arch: 'arm64', extension: '',
  }),
  'darwin:x64': Object.freeze({
    packageName: '@tokscale/cli-darwin-x64',
    triple: 'x86_64-apple-darwin',
    platform: 'darwin', arch: 'x64', extension: '',
  }),
  'win32:x64': Object.freeze({
    packageName: '@tokscale/cli-win32-x64-msvc',
    triple: 'x86_64-pc-windows-msvc',
    platform: 'win32', arch: 'x64', extension: '.exe',
  }),
  'win32:arm64': Object.freeze({
    packageName: '@tokscale/cli-win32-arm64-msvc',
    triple: 'aarch64-pc-windows-msvc',
    platform: 'win32', arch: 'arm64', extension: '.exe',
  }),
  'linux:x64': Object.freeze({
    packageName: '@tokscale/cli-linux-x64-gnu',
    triple: 'x86_64-unknown-linux-gnu',
    platform: 'linux', arch: 'x64', extension: '',
  }),
  'linux:arm64': Object.freeze({
    packageName: '@tokscale/cli-linux-arm64-gnu',
    triple: 'aarch64-unknown-linux-gnu',
    platform: 'linux', arch: 'arm64', extension: '',
  }),
});

export function resolveTokscaleSidecarSpec(platform = process.platform, arch = process.arch) {
  return SPECS[`${platform}:${arch}`] || null;
}

function packageDirectory(rootDir, packageName) {
  const [scope, name] = packageName.split('/');
  return path.join(rootDir, 'node_modules', scope, name);
}

async function validatePackage(pkgDir, spec) {
  const pkg = JSON.parse(await readFile(path.join(pkgDir, 'package.json'), 'utf8'));
  if (pkg.name !== spec.packageName || pkg.version !== TOKSCALE_VERSION) {
    throw new Error(`tokScale sidecar package mismatch: expected ${spec.packageName}@${TOKSCALE_VERSION}`);
  }
  if (Array.isArray(pkg.os) && !pkg.os.includes(spec.platform)) throw new Error('tokScale sidecar OS mismatch');
  if (Array.isArray(pkg.cpu) && !pkg.cpu.includes(spec.arch)) throw new Error('tokScale sidecar CPU mismatch');
}

async function verifyBinaryVersion(binaryPath) {
  const { stdout } = await execFileAsync(binaryPath, ['--version'], { timeout: 5000, windowsHide: true });
  const version = String(stdout).trim().split(/\s+/).find((part) => /^\d+\.\d+\.\d+/.test(part));
  if (version !== TOKSCALE_VERSION) {
    throw new Error(`tokScale sidecar binary mismatch: expected ${TOKSCALE_VERSION}, got ${version || 'unknown'}`);
  }
}
export async function stageTokscaleSidecar({
  rootDir,
  platform = process.platform,
  arch = process.arch,
  verifyBinary = true,
} = {}) {
  if (!rootDir) throw new Error('rootDir is required');
  const spec = resolveTokscaleSidecarSpec(platform, arch);
  if (!spec) throw new Error(`unsupported tokScale sidecar target: ${platform}/${arch}`);

  const pkgDir = packageDirectory(rootDir, spec.packageName);
  await validatePackage(pkgDir, spec);
  const binaryName = platform === 'win32' ? 'tokscale.exe' : 'tokscale';
  const source = path.join(pkgDir, 'bin', binaryName);
  const sourceStat = await lstat(source);
  if (!sourceStat.isFile() || sourceStat.isSymbolicLink()) {
    throw new Error('tokScale sidecar source must be a regular file');
  }
  if (verifyBinary) await verifyBinaryVersion(source);

  const outputDir = path.join(rootDir, 'src-tauri', 'binaries');
  const output = path.join(outputDir, `tokscale-${spec.triple}${spec.extension}`);
  await mkdir(outputDir, { recursive: true });
  await copyFile(source, output);
  if (platform !== 'win32') await chmod(output, 0o755);
  return { ...spec, source, output };
}

const isCli = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isCli) {
  const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const staged = await stageTokscaleSidecar({ rootDir });
  console.log(`Staged tokScale ${TOKSCALE_VERSION}: ${path.relative(rootDir, staged.output)}`);
}
