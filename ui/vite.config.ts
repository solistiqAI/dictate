import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Built into ui/dist and embedded into the binary (build.rs); `npm run dev` serves the page against
// the mock app in src/lib/mock.ts.
export default defineConfig({
	plugins: [svelte()],
	base: './',
	build: { outDir: 'dist', emptyOutDir: true, assetsInlineLimit: 0 },
	server: { host: '127.0.0.1', strictPort: true }
});
