<!-- src/routes/auth/callback/+page.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { auth } from '$lib/stores/auth';

  onMount(async () => {
    // La cookie ya fue seteada por el backend antes del redirect.
    // Solo hay que verificar la sesión y cargar el usuario.
    await auth.init();
    goto('/experiments');
  });
</script>

<div class="shell">
  <div class="spinner"></div>
  <p>Iniciando sesión...</p>
</div>

<style>
  .shell {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    min-height: 60vh; gap: 16px;
    font-size: 14px; color: var(--text-secondary);
  }
  .spinner {
    width: 24px; height: 24px;
    border: 2px solid var(--border-subtle);
    border-top-color: var(--text-muted);
    border-radius: 50%;
    animation: spin .8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>