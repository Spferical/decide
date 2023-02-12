import { defineConfig } from 'wmr';
import { createProxyMiddleware } from 'http-proxy-middleware';

export default defineConfig((options) => {
    if (options.mode != 'build') {
        const proxy_events = createProxyMiddleware('/api', {
            target: 'http://127.0.0.1:8000', changeOrigin: true, ws: true
        });
        options.middleware.push(proxy_events);
    }
    options.alias.react = 'preact/compat';
    options.alias['react-dom'] = 'preact/compat';
});
