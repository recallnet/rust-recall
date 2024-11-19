use crate::encryption::key::SealedObjectKey;
use crate::encryption::metadata::{
    META_ALGORITHM, META_IV, META_SEALED_KEY_SSE_C, META_SEALED_KEY_SSE_KMS,
};
use anyhow::anyhow;
use dare::{HEADER_SIZE, MAX_PAYLOAD_SIZE, TAG_SIZE};
use fendermint_actor_bucket::Object;

pub trait EncryptedObjectExt {
    fn is_encrypted(&self) -> bool;
    fn is_sse_c(&self) -> bool;
    fn is_sse_kms(&self) -> bool;
    fn sealed_object_key(&self) -> anyhow::Result<SealedObjectKey>;

    fn size_decrypted(&self) -> u64;
}

impl EncryptedObjectExt for Object {
    fn is_encrypted(&self) -> bool {
        self.is_sse_c() || self.is_sse_kms()
    }

    fn is_sse_c(&self) -> bool {
        self.metadata
            .contains_key::<String>(&META_SEALED_KEY_SSE_C.into())
    }

    fn is_sse_kms(&self) -> bool {
        self.metadata
            .contains_key::<String>(&META_SEALED_KEY_SSE_KMS.into())
    }

    fn sealed_object_key(&self) -> anyhow::Result<SealedObjectKey> {
        if !self.is_encrypted() {
            return Err(anyhow!(
                "you called sealed_object_key on an object that is not encrypted"
            ));
        }

        let (key, domain) = if self.is_sse_c() {
            (
                self.metadata.get(META_SEALED_KEY_SSE_C).unwrap().to_owned(),
                "SSE-C".to_string(),
            )
        } else {
            return Err(anyhow!("no other method is implemented"));
        };

        let Some(iv) = self.metadata.get(META_IV) else {
            return Err(anyhow!("encrypted objects should have META_IV metadata"));
        };

        let Some(algorithm) = self.metadata.get(META_ALGORITHM) else {
            return Err(anyhow!(
                "encrypted objects should have META_ALGORITHM metadata"
            ));
        };
        SealedObjectKey::new(key, iv.to_owned(), algorithm.to_owned(), domain)
    }

    fn size_decrypted(&self) -> u64 {
        if !self.is_encrypted() {
            return self.size;
        }

        let package_size = HEADER_SIZE + MAX_PAYLOAD_SIZE + TAG_SIZE;
        let content_length = self.size as usize;

        let n_package = (content_length + package_size - 1) / package_size;

        (content_length - (n_package * (HEADER_SIZE + TAG_SIZE))) as u64
    }
}
