import { test, expect } from '@playwright/test';

test.describe('Session Management', () => {
    test('Host initiates a decision and gets a shareable link', async ({ page }) => {
        await page.goto('/');
        // Use the actual button text found in src/index.tsx
        await page.click('button:has-text("Start a Vote")');
        await expect(page).toHaveURL(/.*\/vote\/.*/);
    });

    test('Participant joins a session via link', async ({ page, browser }) => {
        // 1. Create session as host
        const hostPage = await page;
        await hostPage.goto('/');
        await hostPage.click('button:has-text("Start a Vote")');
        const sessionUrl = hostPage.url();

        // 2. Join as participant
        const participantPage = await browser.newPage();
        await participantPage.goto(sessionUrl);
        await expect(participantPage.url()).toBe(sessionUrl);
    });
});
