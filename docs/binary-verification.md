# Binary Verification & Release Hashes

To ensure the integrity and security of your Madmail installation, we provide SHA256 hashes for every official binary release.

## 🛡 How to Verify
Before installing or updating, you should verify that the `madmail` binary matches the official hash.

### On Linux
Run the following command in your terminal:
```bash
sha256sum madmail
```

### On macOS
```bash
shasum -a 256 madmail
```

### On Windows (PowerShell)
```powershell
Get-FileHash madmail -Algorithm SHA256
```

Compare the output string with the hashes listed below or in our official channels.

> [!TIP]
> **Preferred Method (v0.8.103+)**: Starting from version **v0.8.103**, we recommend using the built-in **[Digital Signature Verification](./signature.md)**. The `maddy update` command automatically performs this check for you. 
> 
> **For older versions**: Any version prior to v0.8.103 **must** be verified manually by checking its SHA256 hash against the list below.

## 📢 Official Sources
- **Telegram Channel**: The latest binaries and their corresponding hashes are always posted in our [**Telegram Channel**](https://t.me/the_madmail). If you cannot access our file servers directly, you can always download the latest `madmail` binary from there.
- **GitHub Releases**: Hashes for core releases are also available on the [GitHub Releases](https://github.com/themadorg/madmail/releases) page.

---

## 📜 Historical Release Hashes

If you are using an older version, you can verify it using this list:

### 📦 Madmail Release v0.8.103+f3cfc40
🔐 SHA256 (Binary): `5fc324728ba1df164888e362dfc248670befa97778cab73e2d52ac3e30841bc9`

### 📦 Madmail Release v0.8.102+5a20ec2
🔐 SHA256 (Binary): `c9ee913856807ee7f6619edc5328b098de62799d8bb65473e481f81819a1fa66`

### 📦 Madmail Release v0.8.101+21d710b
🔐 SHA256 (Binary): `e306af3953dffc5f19f1c7525a709d0c6d611dee102a22b82cb658b4eff4ee84`

### 📦 Madmail Release v0.8.99+628a187
🔐 SHA256 (Binary): `97b45464934017c43a0276755586e943959ce5070a16caf69a7966ee619c75d9`

### 📦 Madmail Release v0.8.98+8faeb0d
🔐 SHA256 (Binary): `e162676b7586de1c6825a231debeb32dd46ef6ffac427f2616d9f7b88f31b16f`

### 📦 Madmail Release v0.8.97+b7587cf
🔐 SHA256 (Binary): `dfbda8d32baa82afd0c7a18d477d9abf01c330ccea23861f96014acb727bc6a4`

### 📦 Madmail Release v0.8.96+c7bb9f3
🔐 SHA256 (Binary): `5e55f6c708a658a4efc2cbe047a91164cc0efe4ef1c3f0a7b728e52b3884e391`

### 📦 Madmail Release v0.8.95+0d2adac
🔐 SHA256 (Binary): `64abfcecdfee1065fd60a2c3134ad7021de7b8a159e69c92980fe807a5437b71`

### 📦 Madmail Release v0.8.94+0d2adac
🔐 SHA256 (Binary): `19d4d81eb4d317c8dee505e885d2a48ca008cc0d677856ee0a548c47ae874b5c`

### 📦 Madmail Release v0.8.93+954bb34
🔐 SHA256 (Binary): `30d0ac3ba4f083ae77ca093370af1c71ac1114d59c9c54be8ddafc232d22f907`

### 📦 Madmail Release v0.8.92+954bb34
🔐 SHA256 (Binary): `767f2c07f86ad1ffc26bf84d4a685c930f0bae80dfc45bd4761261158301eb4f`

### 📦 Madmail Release v0.8.86+251aa2e
🔐 SHA256: `4073253dfb5ce1bdc83bdf4ad840ee814bd0af2bb34003348322c0e73f2eaf3d`

### 📦 Madmail Release v0.8.85+251aa2e
🔐 SHA256: `983205cf5bf8d9a59321b9770321c7cd204ad9c6910a01ab96792c8b7433a77c`

### 📦 Madmail Release v0.8.66+251aa2e
🔐 SHA256: `3612eea983a580577d5efb51eeb131e058d2df255edeab9b195f0de6000f8578`

### 📦 Madmail Release v0.8.65+251aa2e
🔐 SHA256: `b2b5079ff15cb87333db4761562ef84605ff01ab6d6f928a96a6f15b35ce1c58`

### 📦 Madmail Release v0.8.55+801b46c
🔐 SHA256: `d8fb6a5421f0883405ccde84fb27b24607294d4c4717a9705ede5d98e620b4b9`

### 📦 Madmail Release v0.8.49+801b46c
🔐 SHA256: `a91924d20cd384853be3cf84fc4b95cbdcfb51fe8fa7b15dee247d4d03224fbd`

### 📦 Madmail Release v0.8.37+143367c
🔐 SHA256: `3c0932e3eb7a20edfe57e2755aeccbd991197a5110ae34e7084f931b2fa8c79e`