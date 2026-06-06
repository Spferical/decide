import { render, screen } from '@testing-library/preact';
import { Rps } from './rps';
import { expect, test, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

// Mock the websocket module
vi.mock('./websocket', () => ({
    make_websocket: () => ({
        send: vi.fn(),
        close: vi.fn(),
    })
}));

test('renders Rps component', () => {
    render(<Rps room="test-room" />);
    expect(screen.getByRole('status')).toHaveTextContent('Connecting...');
});
