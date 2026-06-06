import { test, expect } from '@playwright/test';

test.describe('Voting & Synchronization', () => {
    test('Two users voting should synchronize in real-time', async ({ browser }) => {
        // 1. Create session as host
        const pageA = await browser.newPage();
        await pageA.goto('/');
        await pageA.click('button:has-text("Start a Vote")');

        // Setup initial vote
        await pageA.fill('textarea#choices', 'Option 1\nOption 2');
        await pageA.click('input[type="submit"]');

        const sessionUrl = pageA.url();

        // 2. Join as participant B
        const pageB = await browser.newPage();
        await pageB.goto(sessionUrl);

        // 3. User A votes
        await pageA.click('button:has-text("Submit Your Vote")');

        // 4. Verify User B sees the update (num_votes updated)
        await expect(pageB.locator('text=/.*1\\/2 voters have submitted ballots.*/')).toBeVisible();
    });

    test('Voter can resubmit vote', async ({ browser }) => {
        const pageA = await browser.newPage();
        await pageA.goto('/');
        await pageA.click('button:has-text("Start a Vote")');
        await pageA.fill('textarea#choices', 'Option 1\nOption 2');
        await pageA.click('input[type="submit"]');

        await pageA.click('button:has-text("Submit Your Vote")');
        await expect(pageA.locator('text=You submitted:')).toBeVisible();

        await pageA.click('button:has-text("Resubmit Your Vote")');
        // Simple state change check
        await expect(pageA.locator('text=You submitted:')).toBeVisible();
    });

    test('Invalid room redirection', async ({ page }) => {
        // Navigate directly
        await page.goto('/vote/invalid-room-id');
        // Check if we are redirected to /vote
        await expect(page).toHaveURL(/.*\/vote$/);
    });

    test('Session persistence via cookie', async ({ browser }) => {
        const pageA = await browser.newPage();
        await pageA.goto('/');
        await pageA.click('button:has-text("Start a Vote")');
        await pageA.fill('textarea#choices', 'Option 1\nOption 2');
        await pageA.click('input[type="submit"]');

        const sessionUrl = pageA.url();
        await pageA.reload();
        // After reload, we should still be in the session
        await expect(pageA.url()).toBe(sessionUrl);
    });
});
