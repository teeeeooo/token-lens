import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, mkdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {
  TOKSCALE_VERSION,
  resolveTokscaleSidecarSpec,
  stageTokscaleSidecar,
} from '../scripts/tokscale-sidecar.mjs';

test('sidecar target mapping follows Tauri target-triple naming', () => {
  assert.equal(resolveTokscaleSidecarSpec('darwin', 'arm64')?.triple, 'aarch64-apple-darwin');
  assert.equal(resolveTokscaleSidecarSpec('win32', 'x64')?.triple, 'x86_64-pc-windows-msvc');
  assert.equal(resolveTokscaleSidecarSpec('win32', 'x64')?.extension, '.exe');
  assert.equal(resolveTokscaleSidecarSpec('freebsd', 'x64'), null);
});

test('staging verifies package identity and copies a regular platform binary', async () => {
  const rootDir = await mkdtemp(path.join(os.tmpdir(), 'token-lens-sidecar-'));
  const spec = resolveTokscaleSidecarSpec('darwin', 'arm64');
  const pkgDir = path.join(rootDir, 'node_modules', '@tokscale', 'cli-darwin-arm64');
  await mkdir(path.join(pkgDir, 'bin'), { recursive: true });
  await writeFile(path.join(pkgDir, 'package.json'), JSON.stringify({
    name: spec.packageName, version: TOKSCALE_VERSION, os: ['darwin'], cpu: ['arm64'],
  }));
  await writeFile(path.join(pkgDir, 'bin', 'tokscale'), 'fake-binary');
  try {
    const staged = await stageTokscaleSidecar({ rootDir, platform: 'darwin', arch: 'arm64', verifyBinary: false });
    assert.equal(path.basename(staged.output), 'tokscale-aarch64-apple-darwin');
    assert.equal(await readFile(staged.output, 'utf8'), 'fake-binary');
    const mode = (await stat(staged.output)).mode & 0o777;
    if (process.platform !== 'win32') assert.equal(mode, 0o755);
  } finally {
    await rm(rootDir, { recursive: true, force: true });
  }
});

test('staging rejects a mismatched tokScale version', async () => {
  const rootDir = await mkdtemp(path.join(os.tmpdir(), 'token-lens-sidecar-'));
  const pkgDir = path.join(rootDir, 'node_modules', '@tokscale', 'cli-darwin-arm64');
  await mkdir(path.join(pkgDir, 'bin'), { recursive: true });
  await writeFile(path.join(pkgDir, 'package.json'), JSON.stringify({
    name: '@tokscale/cli-darwin-arm64', version: '9.9.9', os: ['darwin'], cpu: ['arm64'],
  }));
  await writeFile(path.join(pkgDir, 'bin', 'tokscale'), 'fake-binary');
  await assert.rejects(
    () => stageTokscaleSidecar({ rootDir, platform: 'darwin', arch: 'arm64' }),
    /package mismatch/,
  );
  await rm(rootDir, { recursive: true, force: true });
});
