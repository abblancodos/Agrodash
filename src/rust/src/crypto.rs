// src/crypto.rs
//
// Par de llaves RSA generado al arrancar el proceso.
// La llave privada vive solo en memoria — nunca en disco ni en la DB.
// Se usa exclusivamente para descifrar contraseñas en tránsito.
// Se regenera cada vez que reinicia el backend, lo cual es correcto
// porque solo protege el transporte, no datos persistentes.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rsa::{
    pkcs8::EncodePublicKey,
    oaep::Oaep,
    RsaPrivateKey, RsaPublicKey,
};
use sha2::Sha256;
use std::sync::OnceLock;

static KEY_PAIR: OnceLock<(RsaPrivateKey, RsaPublicKey)> = OnceLock::new();

/// Genera el par de llaves al primer llamado, luego lo reutiliza.
pub fn get_or_init_keys() -> &'static (RsaPrivateKey, RsaPublicKey) {
    KEY_PAIR.get_or_init(|| {
        let mut rng = rand::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 2048)
            .expect("Error generando llave RSA");
        let public = RsaPublicKey::from(&private);
        (private, public)
    })
}

/// Devuelve la llave pública en formato PEM (para enviar al cliente).
pub fn public_key_pem() -> String {
    let (_, public) = get_or_init_keys();
    public
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .expect("Error serializando llave pública")
}

/// Descifra un password cifrado con la llave pública (RSA-OAEP + SHA-256).
/// El input es base64 del ciphertext.
pub fn decrypt_password(encrypted_b64: &str) -> Result<String, String> {
    let (private, _) = get_or_init_keys();
    let ciphertext = B64.decode(encrypted_b64)
        .map_err(|e| format!("Base64 inválido: {e}"))?;
    let padding = Oaep::new::<Sha256>();
    let plaintext = private
        .decrypt(padding, &ciphertext)
        .map_err(|e| format!("Error de descifrado: {e}"))?;
    String::from_utf8(plaintext)
        .map_err(|e| format!("UTF-8 inválido: {e}"))
}
