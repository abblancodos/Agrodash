<script lang="ts">
  import '$lib/theme.css';
  import favicon from '$lib/assets/favicon.svg';
  import Topbar from '$lib/components/Topbar.svelte';
  import { onMount } from 'svelte';
  import { preferences } from '$lib/stores/preferences';

  let { children } = $props();

  onMount(() => preferences.init());
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