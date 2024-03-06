import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		proxy: {
			'/ws': 'ws://localhost:3030/ws'
		},
	},
	test: {
		include: ['src/**/*.{test,spec}.{js,ts}'],
		exclude: ['node_modules', '**/wwwroot/**'],
		coverage: {
			exclude: ['wwwroot', '.svelte-kit', '**/**.svelte'],
		}
	},
});
