import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    // The playground draws src/demo/title-scene.tsx in the browser.
    alias: { '@celesta/react': '/src/demo/celesta-browser.ts' },
  },
  build: {
    rolldownOptions: { input: ['index.html', 'docs/index.html'] },
  },
});
