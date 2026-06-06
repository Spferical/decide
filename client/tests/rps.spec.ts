import { test, expect } from '@playwright/test';

test.describe('Rock Paper Scissors', () => {
  test('Two players can play a game', async ({ browser }) => {
    const pageA = await browser.newPage();
    const roomId = 'test-room';
    await pageA.goto(`/rps/${roomId}`);

    const pageB = await browser.newPage();
    await pageB.goto(`/rps/${roomId}`);

    // Both select rock
    await pageA.click('button:has-text("rock")');
    await pageB.click('button:has-text("rock")');

    // Verify history shows draw
    await expect(pageA.locator('text=draw: rock vs rock')).toBeVisible();
    await expect(pageB.locator('text=draw: rock vs rock')).toBeVisible();
  });

  test('Spectator view', async ({ browser }) => {
    const roomId = 'spectator-test';
    const playerA = await browser.newPage();
    await playerA.goto(`/rps/${roomId}`);
    const playerB = await browser.newPage();
    await playerB.goto(`/rps/${roomId}`);
    
    // Player A picks
    await playerA.click('button:has-text("rock")');

    // Spectator joins
    const spectator = await browser.newPage();
    await spectator.goto(`/rps/${roomId}`);
    
    await expect(spectator.locator('text=You are a spectator!')).toBeVisible();
  });
});
