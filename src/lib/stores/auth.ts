// src/lib/stores/auth.ts
//
// Store global de autenticación.
// El token JWT se guarda en localStorage y se restaura al cargar la página.
// El usuario se carga desde /auth/me al iniciar si hay token.

import { writable, derived, get } from 'svelte/store';
import type { AuthUser } from '$lib/api';

interface AuthState {
  token:   string | null;
  user:    AuthUser | null;
  loading: boolean;
}

const TOKEN_KEY = 'agrodash_token';

function createAuthStore() {
  const { subscribe, set, update } = writable<AuthState>({
    token:   null,
    user:    null,
    loading: false,
  });

  return {
    subscribe,

    // Inicializar desde localStorage + cargar usuario
    async init() {
      if (typeof localStorage === 'undefined') return;
      const token = localStorage.getItem(TOKEN_KEY);
      if (!token) return;

      update(s => ({ ...s, token, loading: true }));
      try {
        const res = await fetch('/api/v1/auth/me', {
          headers: { Authorization: `Bearer ${token}` },
        });
        if (res.ok) {
          const user: AuthUser = await res.json();
          set({ token, user, loading: false });
        } else {
          // Token expirado o inválido
          localStorage.removeItem(TOKEN_KEY);
          set({ token: null, user: null, loading: false });
        }
      } catch {
        set({ token: null, user: null, loading: false });
      }
    },

    // Llamar después del callback de OAuth con el token del hash
    setToken(token: string, user: AuthUser) {
      localStorage.setItem(TOKEN_KEY, token);
      set({ token, user, loading: false });
    },

    logout() {
      localStorage.removeItem(TOKEN_KEY);
      set({ token: null, user: null, loading: false });
    },

    getToken(): string | null {
      return get({ subscribe }).token;
    },
  };
}

export const auth = createAuthStore();

// Derived helpers
export const isLoggedIn = derived(auth, $a => !!$a.token && !!$a.user);
export const currentUser = derived(auth, $a => $a.user);
export const authLoading = derived(auth, $a => $a.loading);
