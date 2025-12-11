import { defineConfig } from '@rsbuild/core';
import { pluginReact } from '@rsbuild/plugin-react';

export default defineConfig({
  plugins: [pluginReact()],
  server: {
    proxy: {
      '/api': {
        target: 'http://192.168.123.117:3373',
        changeOrigin: true,
      },
    },
  },
  resolve: {
    alias: {
      '@': './src',
    },
  },
  html: {
    title: 'DRBD HA',
  },
});
