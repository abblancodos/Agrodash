// src/lib/stores/auth.ts
//
// Store de auth basado en cookies HttpOnly.
// El token JWT vive en una cookie que el browser maneja automáticamente.
// Este store solo guarda el perfil del usuario en memoria para la UI.

import { writable, derived } from 'svelte/store';

const API = typeof window !== 'undefined'
  ? (import.meta.env?.VITE_API_BASE ?? '')
  : '';

export interface AuthUser {
  id:           string;
  email:        string;
  display_name: string;
  role:         string;
}

interface AuthState {
  user:    AuthUser | null;
  loading: boolean;
  checked: boolean;  // true después del primer check
}

function createAuthStore() {
  const { subscribe, set, update } = writable<AuthState>({
    user:    null,
    loading: false,
    checked: false,
  });

  return {
    subscribe,

    // Verificar sesión contra el servidor — el browser manda la cookie solo
    async init() {
      update(s => ({ ...s, loading: true }));
      try {
        const res = await fetch(`${API}/api/v1/auth/me`, {
          credentials: 'include',  // incluir cookies
        });
        if (res.ok) {
          const user: AuthUser = await res.json();
          set({ user, loading: false, checked: true });
        } else {
          set({ user: null, loading: false, checked: true });
        }
      } catch {
        set({ user: null, loading: false, checked: true });
      }
    },

    // Llamar después del login por contraseña (no OAuth — OAuth usa redirect)
    setUser(user: AuthUser) {
      set({ user, loading: false, checked: true });
    },

    // Compatibilidad con flujo OAuth que pasa token en el hash.
    // El token no se almacena en el cliente — las cookies HttpOnly lo manejan.
    // Esta función solo actualiza el perfil en memoria.
    setToken(_token: string, user: AuthUser) {
      set({ user, loading: false, checked: true });
    },

    async logout() {
      try {
        await fetch(`${API}/api/v1/auth/logout`, {
          method: 'POST',
          credentials: 'include',
        });
      } catch { /* silencioso */ }
      set({ user: null, loading: false, checked: true });
    },

    // Helper para saber si hay token sin llamar init()
    getToken(): null {
      return null; // Ya no existe — el browser maneja la cookie
    },
  };
}

export const auth = createAuthStore();

export const isLoggedIn  = derived(auth, $a => !!$a.user);
export const currentUser = derived(auth, $a => $a.user);
export const authLoading = derived(auth, $a => $a.loading);