package auth

import "encoding/hex"

// PublicKeyHex is the Ed25519 public key used for verifying binary signatures.
// This is hardcoded into the binary and used by the upgrade/update commands.
const PublicKeyHex = "7cb0bcc1d8e91e51f631c9ad6025e8e6e0222a27c3eeaf8608cf1c8430a6c6b0"

// GetPublicKey returns the decoded Ed25519 public key.
func GetPublicKey() []byte {
	pub, _ := hex.DecodeString(PublicKeyHex)
	return pub
}
