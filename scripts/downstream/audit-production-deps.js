'use strict';

const { spawn } = require('node:child_process');

const MAX_ATTEMPTS = 3;
const ATTEMPT_TIMEOUT_MS = 4 * 60 * 1000;
const RETRY_DELAYS_MS = [15_000, 30_000];

const transientPatterns = [
  /\b408\b.*(?:timeout|request)/i,
  /\b429\b.*(?:too many requests|rate limit)/i,
  /npm warn audit 5\d\d/i,
  /http(?:error)?[^\n]*\b5\d\d\b/i,
  /service unavailable/i,
  /audit endpoint returned an error/i,
  /eai_again/i,
  /econnreset/i,
  /econnrefused/i,
  /enetunreach/i,
  /etimedout/i,
  /network timeout/i,
  /socket hang up/i,
];

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function terminateChild(child) {
  if (process.platform !== 'win32') {
    child.kill('SIGTERM');
    return;
  }

  if (!child.pid) return;
  const killer = spawn('taskkill.exe', ['/pid', String(child.pid), '/t', '/f'], {
    stdio: 'ignore',
    windowsHide: true,
  });
  killer.on('error', () => {
    child.kill();
  });
}

function runAuditAttempt(attempt) {
  return new Promise((resolve, reject) => {
    const args = ['audit', '--omit=dev', '--audit-level=high'];
    const child = spawn('npm', args, {
      cwd: process.cwd(),
      env: process.env,
      windowsHide: true,
      shell: process.platform === 'win32',
    });

    let output = '';
    let timedOut = false;

    const capture = (stream, destination) => {
      stream.on('data', (chunk) => {
        const text = chunk.toString();
        output += text;
        destination.write(text);
      });
    };

    capture(child.stdout, process.stdout);
    capture(child.stderr, process.stderr);

    const timer = setTimeout(() => {
      timedOut = true;
      console.error(`npm audit attempt ${attempt}/${MAX_ATTEMPTS} exceeded ${ATTEMPT_TIMEOUT_MS / 1000}s; terminating it.`);
      terminateChild(child);
    }, ATTEMPT_TIMEOUT_MS);

    child.once('error', (error) => {
      clearTimeout(timer);
      reject(error);
    });

    child.once('close', (code, signal) => {
      clearTimeout(timer);
      resolve({
        code: Number.isInteger(code) ? code : 1,
        signal,
        timedOut,
        output,
      });
    });
  });
}

function isTransientFailure(result) {
  if (result.timedOut) return true;
  return transientPatterns.some((pattern) => pattern.test(result.output));
}

async function main() {
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt += 1) {
    console.log(`Running production dependency audit (attempt ${attempt}/${MAX_ATTEMPTS}, timeout ${ATTEMPT_TIMEOUT_MS / 1000}s)...`);

    let result;
    try {
      result = await runAuditAttempt(attempt);
    } catch (error) {
      console.error(`Failed to start npm audit: ${error.message}`);
      process.exitCode = 1;
      return;
    }

    if (result.code === 0) {
      console.log('Production dependency audit passed.');
      return;
    }

    const transient = isTransientFailure(result);
    if (!transient) {
      console.error(`npm audit failed with exit code ${result.code}; treating this as a real audit failure, not a transient registry error.`);
      process.exitCode = result.code || 1;
      return;
    }

    if (attempt === MAX_ATTEMPTS) {
      console.error('npm audit failed after 3 attempts because the registry/audit service remained unavailable or timed out.');
      process.exitCode = result.code || 1;
      return;
    }

    const delay = RETRY_DELAYS_MS[attempt - 1];
    console.warn(`Transient npm audit failure detected; retrying in ${delay / 1000}s.`);
    await sleep(delay);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
