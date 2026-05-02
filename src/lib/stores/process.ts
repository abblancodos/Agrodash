// Re-export desde process.svelte.ts para compatibilidad de imports sin extensión.
// Vite resuelve process.ts pero no process.svelte.ts en imports bare.
export * from './process.svelte.ts';
