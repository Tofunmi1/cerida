import { reactRouter } from '@react-router/dev/vite'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'
import tsconfigPaths from 'vite-tsconfig-paths'
import { nodePolyfills } from 'vite-plugin-node-polyfills'

export default defineConfig({
  plugins: [tailwindcss(), reactRouter(), tsconfigPaths(), nodePolyfills()],
  server: {
    proxy: {
      '/tee': {
        // Default to a locally-running TEE. Override with CERP_TEE_DEV_URL when you
        // want the local dev app to hit a remote/staging TEE.
        target: process.env.CERP_TEE_DEV_URL || 'http://136.114.124.56:9721',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/tee/, ''),
      },
    },
  },
})
