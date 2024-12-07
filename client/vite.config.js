import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';
import { createProxyMiddleware } from 'http-proxy-middleware';
import eslint from 'vite-plugin-eslint';

export default defineConfig({
  plugins: [preact(), eslint()],
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8000',
        changeOrigin: true,
        ws: true
      }
    }
  }
});
