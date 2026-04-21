<script lang="ts">
  import { auth } from '$lib/stores/auth';
  import { encryptPassword } from '$lib/crypto';

  let { open = $bindable(false) }: { open: boolean } = $props();

  // Tabs: 'choose' | 'invite'
  let tab = $state<'choose' | 'invite'>('choose');

  // Invite registration form
  let inviteCode    = $state('');
  let email         = $state('');
  let displayName   = $state('');
  let password      = $state('');
  let password2     = $state('');
  let loading       = $state(false);
  let error         = $state('');

  const API = import.meta.env.VITE_API_BASE ?? '';

  function close() {
    open = false;
    tab = 'choose';
    error = '';
    inviteCode = displayName = email = password = password2 = '';
  }

  function loginWithGitea() {
    window.location.href = `${API}/api/v1/auth/gitea/login`;
  }

  async function registerWithInvite() {
    error = '';
    if (!inviteCode.trim()) { error = 'Ingresá el código de invitación'; return; }
    if (!email.trim())      { error = 'Email requerido'; return; }
    if (!displayName.trim()){ error = 'Nombre requerido'; return; }
    if (password.length < 8){ error = 'La contraseña debe tener al menos 8 caracteres'; return; }
    if (password !== password2){ error = 'Las contraseñas no coinciden'; return; }

    loading = true;
    try {
      const password_encrypted = await encryptPassword(password);
      const res = await fetch(`${API}/api/v1/auth/register`, {
        method:  'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          invite_code: inviteCode.trim(),
          email: email.trim(),
          display_name: displayName.trim(),
          password_encrypted,
        }),
      });
      const data = await res.json();
      if (!res.ok) { error = data.error ?? 'Error al registrarse'; return; }
      auth.setToken(data.token, data.user);
      close();
    } catch (e: any) {
      error = e.message ?? 'Error de red';
    } finally {
      loading = false;
    }
  }
</script>

{#if open}
  <div class="overlay" onclick={close} role="presentation">
    <div class="modal" onclick={(e) => e.stopPropagation()}
         onkeydown={(e) => e.stopPropagation()}
         role="dialog" aria-modal="true" tabindex="-1">

      <div class="modal__head">
        <span class="modal__title">acceder a AgroDash</span>
        <button class="modal__close" onclick={close} aria-label="cerrar">✕</button>
      </div>

      {#if tab === 'choose'}
        <div class="modal__body">
          <p class="modal__hint">
            Usá tu cuenta del Gitea del lab, o ingresá un código de invitación si es tu primera vez.
          </p>

          <button class="btn-gitea" onclick={loginWithGitea}>
            <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor">
              <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z"/>
            </svg>
            entrar con Gitea
          </button>

          <div class="divider"><span>o</span></div>

          <button class="btn-invite" onclick={() => tab = 'invite'}>
            tengo un código de invitación
          </button>
        </div>

      {:else}
        <div class="modal__body">
          <button class="btn-back" onclick={() => { tab = 'choose'; error = ''; }}>
            ← volver
          </button>

          {#if error}
            <div class="error-box">{error}</div>
          {/if}

          <div class="field">
            <label class="field__label">código de invitación</label>
            <input class="field__input mono" bind:value={inviteCode}
                   placeholder="ej: Xk9mPqR2aT" maxlength="10" />
          </div>
          <div class="field">
            <label class="field__label">nombre completo</label>
            <input class="field__input" bind:value={displayName}
                   placeholder="Tu nombre" />
          </div>
          <div class="field">
            <label class="field__label">email</label>
            <input class="field__input" type="email" bind:value={email}
                   placeholder="tu@email.com" />
          </div>
          <div class="field">
            <label class="field__label">contraseña</label>
            <input class="field__input" type="password" bind:value={password}
                   placeholder="mínimo 8 caracteres" />
          </div>
          <div class="field">
            <label class="field__label">confirmar contraseña</label>
            <input class="field__input" type="password" bind:value={password2}
                   placeholder="repetí la contraseña" />
          </div>

          <button class="btn-register" onclick={registerWithInvite} disabled={loading}>
            {loading ? 'registrando...' : 'crear cuenta'}
          </button>
        </div>
      {/if}

    </div>
  </div>
{/if}

<!-- Callback handler — lee el token del hash después del OAuth redirect -->
<svelte:window onhashchange={() => {
  const hash = window.location.hash;
  if (hash.startsWith('#/auth/callback')) {
    const params = new URLSearchParams(hash.replace('#/auth/callback?', ''));
    const token = params.get('token');
    if (token) {
      // Obtener el usuario con el token
      fetch(`${API}/api/v1/auth/me`, {
        headers: { Authorization: `Bearer ${token}` }
      }).then(r => r.json()).then(user => {
        auth.setToken(token, user);
        // Limpiar el hash
        history.replaceState(null, '', window.location.pathname);
      }).catch(() => {});
    }
  }
}} />

<style>
  .overlay {
    position: fixed; inset: 0; z-index: 200;
    background: rgba(0,0,0,0.45);
    display: flex; align-items: center; justify-content: center;
    padding: 20px;
  }
  .modal {
    background: var(--bg-surface);
    border: 0.5px solid var(--border-default);
    border-radius: 12px;
    width: 100%; max-width: 380px;
    overflow: hidden;
  }
  .modal__head {
    display: flex; align-items: center; justify-content: space-between;
    padding: calc(14px * var(--font-scale)) calc(18px * var(--font-scale));
    border-bottom: 0.5px solid var(--border-subtle);
  }
  .modal__title {
    font-size: calc(14px * var(--font-scale));
    font-weight: 500; color: var(--text-primary);
  }
  .modal__close {
    background: none; border: none; cursor: pointer;
    font-size: calc(14px * var(--font-scale));
    color: var(--text-muted); padding: 4px;
  }
  .modal__body {
    padding: calc(20px * var(--font-scale)) calc(18px * var(--font-scale));
    display: flex; flex-direction: column; gap: calc(12px * var(--font-scale));
  }
  .modal__hint {
    font-size: calc(13px * var(--font-scale));
    color: var(--text-secondary); line-height: 1.5;
  }
  .btn-gitea {
    display: flex; align-items: center; justify-content: center;
    gap: calc(8px * var(--font-scale));
    padding: calc(10px * var(--font-scale));
    background: var(--text-primary); color: var(--bg-surface);
    border: none; border-radius: 8px; cursor: pointer;
    font-size: calc(13px * var(--font-scale)); font-weight: 500;
    width: 100%; transition: opacity .12s;
  }
  .btn-gitea:hover { opacity: 0.85; }
  .divider {
    display: flex; align-items: center; gap: 10px;
    color: var(--text-muted); font-size: calc(12px * var(--font-scale));
  }
  .divider::before, .divider::after {
    content: ''; flex: 1; height: 0.5px; background: var(--border-subtle);
  }
  .btn-invite {
    background: none; border: 0.5px solid var(--border-default);
    border-radius: 8px; padding: calc(9px * var(--font-scale));
    color: var(--text-secondary); cursor: pointer; width: 100%;
    font-size: calc(13px * var(--font-scale)); transition: all .12s;
  }
  .btn-invite:hover { background: var(--interactive-hover); color: var(--text-primary); }
  .btn-back {
    background: none; border: none; cursor: pointer;
    font-size: calc(12px * var(--font-scale)); color: var(--text-muted);
    text-align: left; padding: 0; margin-bottom: 4px;
  }
  .error-box {
    background: var(--error-bg); color: var(--error-color);
    border: 0.5px solid var(--error-color);
    border-radius: 6px; padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale));
    font-size: calc(12px * var(--font-scale));
  }
  .field { display: flex; flex-direction: column; gap: calc(4px * var(--font-scale)); }
  .field__label { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); }
  .field__input {
    padding: calc(7px * var(--font-scale)) calc(10px * var(--font-scale));
    background: var(--bg-elevated); border: 0.5px solid var(--border-default);
    border-radius: 6px; color: var(--text-primary);
    font-size: calc(13px * var(--font-scale)); outline: none;
    transition: border-color .12s;
  }
  .field__input:focus { border-color: var(--text-primary); }
  .field__input.mono { font-family: 'DM Mono', monospace; letter-spacing: .08em; }
  .btn-register {
    padding: calc(10px * var(--font-scale));
    background: var(--text-primary); color: var(--bg-surface);
    border: none; border-radius: 8px; cursor: pointer;
    font-size: calc(13px * var(--font-scale)); font-weight: 500;
    width: 100%; transition: opacity .12s; margin-top: 4px;
  }
  .btn-register:hover:not(:disabled) { opacity: 0.85; }
  .btn-register:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
