<script lang="ts">
  import '$lib/theme.css';
  import favicon from '$lib/assets/favicon.svg';
  import Topbar from '$lib/components/Topbar.svelte';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { preferences } from '$lib/stores/preferences';
  import { auth } from '$lib/stores/auth';

  const API = import.meta.env.VITE_API_BASE ?? '';

  // Maneja el token de OAuth en el hash — funciona desde cualquier página
  async function handleAuthHash() {
    const hash = window.location.hash;
    if (!hash.includes('auth_token=')) return;
    const token = new URLSearchParams(hash.slice(hash.indexOf('?') + 1)).get('auth_token')
                  ?? hash.split('auth_token=')[1]?.split('&')[0];
    if (!token) return;
    try {
      const res = await fetch(`${API}/api/v1/auth/me`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (res.ok) {
        const user = await res.json();
        auth.setToken(token, user);
        // Limpiar el hash y quedarse en /experiments
        history.replaceState(null, '', '/experiments');
      }
    } catch { /* silencioso */ }
  }

  let { children } = $props();

  onMount(() => {
    preferences.init();
    handleAuthHash();
  });
</script>

<svelte:head>
  <link rel="icon" href={favicon} />
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link href="https://fonts.googleapis.com/css2?family=DM+Mono:wght@400;500&display=swap" rel="stylesheet">
</svelte:head>

<div class="app-shell" style="--font-scale: {$preferences.fontScaleValue}">
  <Topbar />
  <div class="app-content">{@render children()}</div>
</div>

<style>
  .app-shell { display:flex; flex-direction:column; min-height:100vh; }
  .app-content { flex:1; }
</style>