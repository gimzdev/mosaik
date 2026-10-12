import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// Tauri serves the built files from disk in release builds and this dev server in `tauri dev`.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: '127.0.0.1', fs: { allow: ['..', '../../crates/mosaik-ipc/fixtures'] } },
  build: {
    target: 'es2023',
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
    // Fonts as files, never inlined as data: URLs (the content security policy allows 'self').
    assetsInlineLimit: 0,
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}'],
    restoreMocks: true,
  },
});
