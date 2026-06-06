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

    test('Only creator can end the vote', async ({ browser }) => {
        // 1. Host creates a vote with creator restriction
        const pageA = await browser.newPage();
        await pageA.goto('/');
        await pageA.click('button:has-text("Start a Vote")');
        await pageA.fill('textarea#choices', 'Option A\nOption B');
        await pageA.selectOption('select#end-rule', 'creator');
        await pageA.click('input[type="submit"]');

        const sessionUrl = pageA.url();

        // 2. Participant B joins
        const pageB = await browser.newPage();
        await pageB.goto(sessionUrl);

        // 3. Verify pageA (creator) sees the end button
        await expect(pageA.locator('button:has-text("End Voting and Show the Results")')).toBeVisible();

        // 4. Verify pageB (non-creator) does not see the end button, and sees the creator notice
        await expect(pageB.locator('button:has-text("End Voting and Show the Results")')).not.toBeVisible();
        await expect(pageB.locator('text=Only the vote creator can end voting.')).toBeVisible();

        // 5. Host clicks end button and results are tallied
        await pageA.click('button:has-text("End Voting and Show the Results")');
        await expect(pageA.locator('summary:has-text("Results")').first()).toBeVisible();
        await expect(pageB.locator('summary:has-text("Results")').first()).toBeVisible();
    });

    test('Password protected vote ending', async ({ browser }) => {
        // 1. Host creates a vote with password restriction
        const pageA = await browser.newPage();
        await pageA.goto('/');
        await pageA.click('button:has-text("Start a Vote")');
        await pageA.fill('textarea#choices', 'Option X\nOption Y');
        await pageA.selectOption('select#end-rule', 'password');
        await pageA.fill('input#end-password', 'secret123');
        await pageA.click('input[type="submit"]');

        const sessionUrl = pageA.url();

        // 2. Participant B joins
        const pageB = await browser.newPage();
        await pageB.goto(sessionUrl);

        // 3. Both see the password protected button
        await expect(pageA.locator('button:has-text("End Voting and Show the Results (Password Protected)")')).toBeVisible();
        await expect(pageB.locator('button:has-text("End Voting and Show the Results (Password Protected)")')).toBeVisible();

        // 4. Set up dynamic dialog handling on pageB
        const dialogs: string[] = [];
        pageB.on('dialog', async dialog => {
            dialogs.push(dialog.type());
            if (dialog.type() === 'prompt') {
                if (dialogs.filter(t => t === 'prompt').length === 1) {
                    await dialog.accept('wrongpassword');
                } else {
                    await dialog.accept('secret123');
                }
            } else if (dialog.type() === 'alert') {
                expect(dialog.message()).toContain('Incorrect password');
                await dialog.dismiss();
            }
        });

        // 5. Click end button with wrong password
        await pageB.click('button:has-text("End Voting and Show the Results (Password Protected)")');

        // Results should NOT be visible yet
        await expect(pageB.locator('summary:has-text("Results")').first()).not.toBeVisible();

        // 6. Click end button again and enter the correct password
        await pageB.click('button:has-text("End Voting and Show the Results (Password Protected)")');

        // Results should now be visible on both pages
        await expect(pageB.locator('summary:has-text("Results")').first()).toBeVisible();
        await expect(pageA.locator('summary:has-text("Results")').first()).toBeVisible();
    });
});
