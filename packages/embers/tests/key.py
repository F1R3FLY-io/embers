from functools import cached_property
from hashlib import blake2b
from typing import Self

import ecdsa
from ecdsa.util import sigencode_der_canonize


class SECP256k1:
    def __init__(self, key: ecdsa.SigningKey):
        self.key = key

    @classmethod
    def from_hex(cls, data: str) -> Self:
        return cls(ecdsa.SigningKey.from_string(bytes.fromhex(data), curve=ecdsa.SECP256k1))

    @classmethod
    def generate(cls) -> Self:
        return cls(ecdsa.SigningKey.generate(curve=ecdsa.SECP256k1))

    @cached_property
    def public_key(self) -> ecdsa.VerifyingKey:
        return self.key.get_verifying_key()  # pyright: ignore[reportReturnType]

    @cached_property
    def public_key_bytes(self) -> bytes:
        return self.public_key.to_string("uncompressed")

    def sign(self, data: bytes) -> bytes:
        prehashed = blake2b(data, digest_size=32).digest()
        return self.key.sign_digest(prehashed, sigencode=sigencode_der_canonize)
