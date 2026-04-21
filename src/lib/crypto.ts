// src/lib/crypto.ts
//
// Cifrado RSA-OAEP de contraseñas en el cliente.
// Usa la Web Crypto API nativa del browser — sin dependencias extra.
//
// Flujo:
//   1. Al cargar la página de login: fetchPublicKey()
//   2. Al enviar el form: encryptPassword(password, publicKey)
//   3. Enviar { email, password_encrypted: base64 } al backend

const API_BASE = import.meta.env.VITE_API_URL ?? '';

let cachedPublicKey: CryptoKey | null = null;

/**
 * Obtiene la llave pública del backend y la importa para uso con Web Crypto.
 * Cachea el resultado para no hacer múltiples requests.
 */
export async function fetchPublicKey(): Promise<CryptoKey> {
    if (cachedPublicKey) return cachedPublicKey;

    const res = await fetch(`${API_BASE}/api/v1/auth/public-key`);
    if (!res.ok) throw new Error('No se pudo obtener la llave pública');

    const { public_key } = await res.json();

    // Convertir PEM a ArrayBuffer
    const pemBody = public_key
        .replace(/-----BEGIN PUBLIC KEY-----/, '')
        .replace(/-----END PUBLIC KEY-----/, '')
        .replace(/\s/g, '');
    const binary = atob(pemBody);
    const buffer = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
        buffer[i] = binary.charCodeAt(i);
    }

    cachedPublicKey = await crypto.subtle.importKey(
        'spki',
        buffer.buffer,
        { name: 'RSA-OAEP', hash: 'SHA-256' },
        false,
        ['encrypt']
    );

    return cachedPublicKey;
}

/**
 * Cifra una contraseña con la llave pública RSA-OAEP.
 * Devuelve el ciphertext en base64 para enviar al backend.
 */
export async function encryptPassword(password: string): Promise<string> {
    const publicKey = await fetchPublicKey();
    const encoded   = new TextEncoder().encode(password);
    const encrypted = await crypto.subtle.encrypt(
        { name: 'RSA-OAEP' },
        publicKey,
        encoded
    );
    // Convertir a base64
    const bytes  = new Uint8Array(encrypted);
    const binary = Array.from(bytes).map(b => String.fromCharCode(b)).join('');
    return btoa(binary);
}

/**
 * Invalida el caché de la llave pública.
 * Llamar si el backend se reinició (llave nueva).
 */
export function invalidatePublicKey(): void {
    cachedPublicKey = null;
}
