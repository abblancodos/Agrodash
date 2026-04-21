<script lang="ts">
  import { auth } from '$lib/stores/auth';
  import { experimentStore } from '$lib/stores/experiment';

  let { value = $bindable(''), readonly = false }:
    { value: string; readonly?: boolean } = $props();

  const API = import.meta.env.VITE_API_BASE ?? '';

  let validating = $state(false);
  let validation = $state<{ valid: boolean; error?: string } | null>(null);

  // Available variables hint
  const availableVars = $derived([
    ...$experimentStore.definitions
      .filter(d => d.type === 'constant' || d.type === 'expression')
      .map(d => d.key),
    ...Object.keys($experimentStore.experiment?.constants ?? {}),
  ]);

  async function validate() {
    validating = true; validation = null;
    try {
      const token = auth.getToken();
      const res = await fetch(`${API}/api/v1/scripts/validate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
        body: JSON.stringify({ script: value }),
      });
      validation = await res.json();
    } catch { validation = { valid: false, error: 'Error de red' }; }
    finally { validating = false; }
  }

  // Simple syntax highlighting via regex replacements
  function highlight(code: string) {
    return code
      .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
      .replace(/\b(let|if|else|while|for|fn|return|true|false|in)\b/g,
               '<span class="kw">$1</span>')
      .replace(/\b(log|warn|error|output|plot|table|goto)\b(?=\s*\()/g,
               '<span class="fn">$1</span>')
      .replace(/"([^"]*?)"/g, '<span class="str">"$1"</span>')
      .replace(/\/\/.*/g, '<span class="comment">$&</span>')
      .replace(/\b(\d+\.?\d*)\b/g, '<span class="num">$1</span>');
  }
</script>

<div class="script-editor">
  <div class="editor-head">
    <span class="editor-label">script rhai</span>
    <div class="editor-actions">
      {#if availableVars.length > 0}
        <details class="vars-hint">
          <summary class="vars-toggle">variables disponibles</summary>
          <div class="vars-list">
            {#each availableVars as v}
              <code class="var-chip">{v}</code>
            {/each}
            <code class="var-chip fn-chip">log()</code>
            <code class="var-chip fn-chip">warn()</code>
            <code class="var-chip fn-chip">output()</code>
            <code class="var-chip fn-chip">goto()</code>
          </div>
        </details>
      {/if}
      {#if !readonly}
        <button class="btn-validate" onclick={validate} disabled={validating}>
          {validating ? 'validando...' : 'validar'}
        </button>
      {/if}
    </div>
  </div>

  {#if readonly}
    <div class="code-display">
      {@html highlight(value)}
    </div>
  {:else}
    <textarea
      class="code-input"
      bind:value
      spellcheck="false"
      autocomplete="off"
      rows="8"
      placeholder="// Escribí el script Rhai acá&#10;let theta = steps.pesaje.value;&#10;output(&quot;theta&quot;, theta, &quot;m³/m³&quot;);"
    ></textarea>
  {/if}

  {#if validation}
    <div class="validation" class:ok={validation.valid} class:fail={!validation.valid}>
      {#if validation.valid}
        ✓ sintaxis válida
      {:else}
        ✗ {validation.error}
      {/if}
    </div>
  {/if}
</div>

<style>
  .script-editor { display: flex; flex-direction: column; gap: calc(6px * var(--font-scale)); }
  .editor-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .editor-label { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); text-transform: uppercase; letter-spacing: .06em; }
  .editor-actions { display: flex; align-items: center; gap: 8px; }

  .vars-hint { position: relative; }
  .vars-toggle { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); cursor: pointer; list-style: none; }
  .vars-list { position: absolute; right: 0; top: calc(100% + 4px); background: var(--bg-surface); border: 0.5px solid var(--border-default); border-radius: 6px; padding: 8px; display: flex; flex-wrap: wrap; gap: 4px; z-index: 10; min-width: 200px; max-width: 300px; }
  .var-chip { font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace; background: var(--bg-elevated); padding: 2px 6px; border-radius: 4px; color: var(--text-secondary); }
  .fn-chip { color: #0F6E56; background: #E1F5EE; }

  .btn-validate { font-size: calc(11px * var(--font-scale)); padding: 3px 10px; border: 0.5px solid var(--border-default); border-radius: 4px; background: none; color: var(--text-secondary); cursor: pointer; }

  .code-input, .code-display {
    font-family: 'DM Mono', monospace;
    font-size: calc(11px * var(--font-scale));
    line-height: 1.6;
    background: var(--bg-elevated);
    border: 0.5px solid var(--border-default);
    border-radius: 6px;
    padding: calc(10px * var(--font-scale));
    color: var(--text-primary);
    width: 100%;
    resize: vertical;
    outline: none;
    tab-size: 2;
  }
  .code-display { white-space: pre; overflow-x: auto; }

  .validation { font-size: calc(12px * var(--font-scale)); padding: 6px 10px; border-radius: 4px; }
  .validation.ok   { background: #EAF3DE; color: #3B6D11; }
  .validation.fail { background: #FCEBEB; color: #A32D2D; font-family: 'DM Mono', monospace; }

  :global(.kw)      { color: #534AB7; }
  :global(.fn)      { color: #0F6E56; }
  :global(.str)     { color: #993C1D; }
  :global(.num)     { color: #185FA5; }
  :global(.comment) { color: var(--text-muted); font-style: italic; }
</style>
