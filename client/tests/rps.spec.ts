import { test, expect } from '@playwright/test';

test.describe('Rock Paper Scissors', () => {
    test('Draw handling', async ({ browser }) => {
        const roomId = 'draw-test';
        const playerA = await browser.newPage();
        await playerA.goto(`/rps/${roomId}`);
        const playerB = await browser.newPage();
        await playerB.goto(`/rps/${roomId}`);

        // Both pick rock
        await playerA.click('button:has-text("rock")');
        await playerB.click('button:has-text("rock")');

        await expect(playerA.locator('text=draw: rock vs rock')).toBeVisible();
    });

    test('Player disconnection', async ({ browser }) => {
        const roomId = 'disconnect-test';
        const playerA = await browser.newPage();
        await playerA.goto(`/rps/${roomId}`);
        const playerB = await browser.newPage();
        await playerB.goto(`/rps/${roomId}`);

        // Both are connected, verify both are players
        await expect(playerB.locator('text=You are a player!')).toBeVisible();

        // Player A leaves
        await playerA.close();

        // Player B should see the notice to send URL to opponent since players count dropped < 2
        await expect(playerB.locator('text=Send this URL to your opponent to connect.')).toBeVisible();
    });
});
