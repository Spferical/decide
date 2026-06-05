import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  use: {
    baseURL: 'http://127.0.0.1:8001',
    trace: 'on-first-retry',
  },
  projects: [
    { 
      name: 'chromium', 
      use: { 
        ...devices['Desktop Chrome'],
        launchOptions: {
          executablePath: '/nix/store/583gxir41b9qdsbggwcv33zmrgkk0cvi-playwright-chromium/chrome-linux64/chrome'
        }
      } 
    },
  ],
});
