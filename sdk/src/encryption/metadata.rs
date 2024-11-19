/// META_ALGORITHM is the algorithm used to derive internal keys and encrypt the objects.
pub const META_ALGORITHM: &str = "sse-algorithm";
/// META_IV is the random initialization vector (IV) used for key derivation.
pub const META_IV: &str = "sse-iv";
/// META_SEALED_KEY_SSEC is the sealed object encryption key in case of SSE-C.
pub const META_SEALED_KEY_SSE_C: &str = "sse-sealed-key-ssec";
/// META_SEALED_KEY_SSE_C is the sealed object encryption key in case of SSE-KMS.
pub const META_SEALED_KEY_SSE_KMS: &str = "sse-sealed-key-kms";
