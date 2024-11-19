use crate::encryption::encryptor::EncryptReader;
use crate::encryption::key::{generate_iv, generate_object_key};
use crate::encryption::metadata::{META_ALGORITHM, META_IV, META_SEALED_KEY_SSE_C};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::collections::HashMap;
use tokio::io::AsyncRead;

pub fn encrypt_reader<R: AsyncRead>(
    reader: R,
    key: &str,
    object_path: &str,
) -> anyhow::Result<(EncryptReader<R>, HashMap<String, String>)> {
    let kek = STANDARD.decode(key)?;
    let object_encryption_key = generate_object_key(&kek, None)?;

    let encryptor = dare::encryptor::DAREEncryptor::new(
        object_encryption_key.key,
        dare::CipherSuite::AES256GCM,
    )?;

    let reader = EncryptReader::new(reader, encryptor);

    let iv = generate_iv(None);
    let sealed_object_key = object_encryption_key.seal(&kek, &iv, "SSE-C", object_path)?;

    let sealed_key_str = sealed_object_key.key_as_string()?;
    let iv = sealed_object_key.iv_as_string()?;
    let algorithm = sealed_object_key.algorithm();

    let mut metadata = HashMap::new();
    metadata.insert(META_SEALED_KEY_SSE_C.into(), sealed_key_str);
    metadata.insert(META_IV.into(), iv);
    metadata.insert(META_ALGORITHM.into(), algorithm);

    Ok((reader, metadata))
}
