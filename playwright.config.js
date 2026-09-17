import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  testMatch: '**/*.pw.js',
  fullyParallel: true,
  use: { baseURL: 'http://127.0.0.1:1420', viewport: { width: 380, height: 720 }, locale: 'en-US' },
  webServer: { command: 'npm run frontend:dev', url: 'http://127.0.0.1:1420', reuseExistingServer: false },
});
