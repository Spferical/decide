import { render, screen, fireEvent } from '@testing-library/preact';
import { expect, test } from 'vitest';
import { Vote, Choices } from './vote';

test('renders vote form when no room provided', () => {
    render(<Vote room="" />);
    expect(screen.getByText('Start a Vote')).toBeDefined();
});

test('Choices component renders choices', () => {
    render(<Choices choices={['A', 'B']} initial_ranks={[{ candidate: 0, rank: 0 }, { candidate: 1, rank: 0 }]} />);
    expect(screen.getByText('A')).toBeDefined();
    expect(screen.getByText('B')).toBeDefined();
});
