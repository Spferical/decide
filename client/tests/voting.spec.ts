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
});
