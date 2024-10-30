use base64::{engine::general_purpose::STANDARD, Engine as _};
use dare::{CipherSuite, DAREDecryptor, DAREEncryptor, HEADER_SIZE};
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;

pub fn generate_object_key(
    kek: &[u8],
    random: Option<&mut dyn RngCore>,
) -> anyhow::Result<ObjectKey> {
    let random = match random {
        Some(r) => r,
        None => &mut OsRng,
    };

    // Generate nonce
    let mut nonce = [0u8; 32];
    random.fill_bytes(&mut nonce);

    // Define the context
    const CONTEXT: &str = "object-encryption-key generation";

    // HMAC setup
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(kek)?;
    mac.update(CONTEXT.as_bytes());
    mac.update(&nonce);

    // Finalize and return the key
    let result = mac.finalize();
    let key = result.into_bytes();

    let mut object_key = [0u8; 32];
    object_key.copy_from_slice(&key[..32]);

    Ok(ObjectKey { key: object_key })
}

pub fn generate_iv(random: Option<&mut dyn RngCore>) -> [u8; 32] {
    let random = match random {
        Some(r) => r,
        None => &mut OsRng,
    };

    // Generate nonce
    let mut nonce = [0u8; 32];
    random.fill_bytes(&mut nonce);

    nonce
}

#[derive(Debug)]
pub struct ObjectKey {
    pub key: [u8; 32],
}

#[derive(Debug)]
pub struct SealedObjectKey {
    key: Vec<u8>,
    iv: [u8; 32],
    algorithm: String,
    domain: String,
}

impl SealedObjectKey {
    pub fn new(
        key: String,
        iv_str: String,
        algorithm: String,
        domain: String,
    ) -> anyhow::Result<SealedObjectKey> {
        let key = STANDARD.decode(&key)?;
        let iv = STANDARD.decode(&iv_str)?.as_slice()[0..32].try_into()?;

        Ok(SealedObjectKey {
            key,
            iv,
            algorithm,
            domain,
        })
    }
    pub fn key(&self) -> Vec<u8> {
        self.key.clone()
    }

    pub fn algorithm(&self) -> String {
        self.algorithm.clone()
    }

    pub fn iv_as_string(&self) -> anyhow::Result<String> {
        Ok(STANDARD.encode(self.iv))
    }

    pub fn key_as_string(&self) -> anyhow::Result<String> {
        Ok(STANDARD.encode(&self.key))
    }

    pub fn domain(&self) -> String {
        self.domain.clone()
    }

    pub fn unseal(&self, kek: String, object_path: &str) -> anyhow::Result<ObjectKey> {
        let key = STANDARD.decode(&kek)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("HMAC can take key of any size");

        // Write data to the MAC
        mac.update(self.iv.as_slice()); // iv
        mac.update(self.domain().as_bytes());
        mac.update("DAREv1-HMAC-SHA256".as_bytes());
        mac.update(object_path.as_bytes());

        // Compute the final HMAC and store it in sealing_key
        let bytes = mac.finalize().into_bytes();
        let sealing_key = bytes.as_slice()[0..32].try_into()?;

        let mut decryptor = DAREDecryptor::new(sealing_key);
        let ciphertext = self.key.as_slice();

        let key = decryptor.decrypt(&ciphertext[..HEADER_SIZE], &ciphertext[HEADER_SIZE..])?;

        Ok(ObjectKey {
            key: key.as_slice()[0..32].try_into()?,
        })
    }
}

impl ObjectKey {
    pub fn seal(
        &self,
        kek: &[u8],
        iv: &[u8; 32],
        domain: &str,
        object_path: &str,
    ) -> anyhow::Result<SealedObjectKey> {
        let mut mac = Hmac::<Sha256>::new_from_slice(kek).expect("HMAC can take key of any size");

        // Write data to the MAC
        mac.update(iv); // iv
        mac.update(domain.as_bytes());
        mac.update("DAREv1-HMAC-SHA256".as_bytes());
        mac.update(object_path.as_bytes());

        // Compute the final HMAC and store it in sealing_key
        let bytes = mac.finalize().into_bytes();
        let sealing_key = bytes.as_slice()[0..32].try_into()?;

        let mut encryptor = DAREEncryptor::new(sealing_key, CipherSuite::AES256GCM)?;
        let cipher_text = encryptor.encrypt(&self.key)?;

        Ok(SealedObjectKey {
            iv: *iv,
            key: cipher_text,
            algorithm: "DAREv1-HMAC-SHA256".to_string(),
            domain: domain.to_string(),
        })
    }
}
