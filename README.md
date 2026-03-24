# Madmail - Maddy Chatmail Server
A mad fork of [maddy](https://github.com/foxcpp/maddy), bringing the madness to mail delivery — optimized for instant, secure messaging with #deltachat.

## Quick Setup

```bash
curl -fsSL https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/') -o madmail && chmod +x madmail && sudo ./madmail install --simple --ip YOUR_IP --lang en && sudo systemctl enable madmail && sudo systemctl start madmail
```

> Replace `YOUR_IP` with your server's public IP address.

## Documentation
For installation, configuration, and detailed guides, please refer to the [**Documentation Index**](./docs/index.md).

> [!IMPORTANT]
> Parts of this project are developed with AI assistance. Read our [**AI Disclosure & Security Model**](./docs/ai-disclosure.md) for more details.

## Resources
- [GitHub Releases](https://github.com/themadorg/madmail/releases)
- [Telegram Channel](https://t.me/the_madmail)
- [**Delta Chat Official Website**](https://delta.chat)
- [**Download Delta Chat Apps**](https://delta.chat/en/download)

## License
Licensed under [GPL-3.0](https://www.gnu.org/licenses/gpl-3.0.en.html).
