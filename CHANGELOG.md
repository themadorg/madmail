## [2.13.1](https://github.com/themadorg/madmail/compare/v2.13.0...v2.13.1) (2026-07-09)


### Bug Fixes

* deliver /mxdeliv federation mail to all X-Mail-To recipients ([64c50ea](https://github.com/themadorg/madmail/commit/64c50ea586e66744565640b80d4c9afe38bb3845))

# [2.13.0](https://github.com/themadorg/madmail/compare/v2.12.0...v2.13.0) (2026-07-08)


### Features

* reflect request Origin for WebIMAP/WebSMTP browser access ([ae009a3](https://github.com/themadorg/madmail/commit/ae009a3c4feed5d52c0e0aa6dd53bb4ca402ad3e))

# [2.12.0](https://github.com/themadorg/madmail/compare/v2.11.2...v2.12.0) (2026-07-08)


### Bug Fixes

* **contact-sharing:** implement contact sharing functionality ([aa27055](https://github.com/themadorg/madmail/commit/aa27055434d2644d320983b38622eb461438e6d4))


### Features

* add cors handeling ([1b57402](https://github.com/themadorg/madmail/commit/1b574026c26f2d5ec914fe30bced46bca9c37ebd))

## [2.11.2](https://github.com/themadorg/madmail/compare/v2.11.1...v2.11.2) (2026-07-07)


### Bug Fixes

* **storage:** avoid maildir list-cache deadlock under concurrent access ([4e28790](https://github.com/themadorg/madmail/commit/4e2879052b42e29b2bda59396d276fee4b5548c3))

## [2.11.1](https://github.com/themadorg/madmail/compare/v2.11.0...v2.11.1) (2026-06-17)


### Bug Fixes

* Enhance Makefile and code formatting across multiple files ([96abc2d](https://github.com/themadorg/madmail/commit/96abc2db38e6b6a6248c8826791d0890f196d808))

# [2.11.0](https://github.com/themadorg/madmail/compare/v2.10.0...v2.11.0) (2026-06-17)


### Features

* **auth:** implement SHA256 password hashing and upgrade mechanism ([3304b2e](https://github.com/themadorg/madmail/commit/3304b2eccb1828d5317fc5bc73ea67589bad718a))

# [2.10.0](https://github.com/themadorg/madmail/compare/v2.9.0...v2.10.0) (2026-06-17)


### Features

* add Shadowsocks proxy support and enhance TURN server configuration ([6037c72](https://github.com/themadorg/madmail/commit/6037c72975330bf7dc8ae6801fde7dceb4596f5a))

# [2.9.0](https://github.com/themadorg/madmail/compare/v2.8.2...v2.9.0) (2026-06-15)


### Bug Fixes

* add support for Iroh relay configuration ([960d6bb](https://github.com/themadorg/madmail/commit/960d6bb5ebea3f267379f0db685eb9f482fb316b))


### Features

* implement federation size management in the admin interface ([a184cec](https://github.com/themadorg/madmail/commit/a184cec39e911f01411fd17bded43a26fbc2fab5))

## [2.8.2](https://github.com/themadorg/madmail/compare/v2.8.1...v2.8.2) (2026-06-13)


### Bug Fixes

* remove memory leak probe test ([b3fee06](https://github.com/themadorg/madmail/commit/b3fee064d060878877733b9bb1f0b91a75ac1e66))

## [2.8.1](https://github.com/themadorg/madmail/compare/v2.8.0...v2.8.1) (2026-06-12)


### Bug Fixes

* rename project from chatmail-rs to madmail-v2 ([ab647de](https://github.com/themadorg/madmail/commit/ab647dee344a42748c5d8da25aee6a12d338e91b))
* **storage:** add coordinator count for Never-mode delivery batcher ([b38d087](https://github.com/themadorg/madmail/commit/b38d087f15e97e2bee7218bc8e9303f089dbdab4))

# [2.8.0](https://github.com/themadorg/madmail/compare/v2.7.0...v2.8.0) (2026-06-11)


### Features

* **landing:** add scripts for documentation generation and font synchronization ([9fb8d3d](https://github.com/themadorg/madmail/commit/9fb8d3da70aae4487f67ff798e2017dde9e39034))

# [2.7.0](https://github.com/themadorg/madmail/compare/v2.6.0...v2.7.0) (2026-06-11)


### Features

* **landing:** add build and deployment support for static SvelteKit site ([d7217c0](https://github.com/themadorg/madmail/commit/d7217c0e6b6dffd2ae2b936eab61219865c12a44))

# [2.6.0](https://github.com/themadorg/madmail/compare/v2.5.0...v2.6.0) (2026-06-11)


### Features

* **cli:** enhance command-line interface and configuration handling ([2a51a79](https://github.com/themadorg/madmail/commit/2a51a796e6adaf86a79c5f86dbf453180f35f441))

# [2.5.0](https://github.com/themadorg/madmail/compare/v2.4.1...v2.5.0) (2026-06-11)


### Bug Fixes

* **install:** enhance configuration handling and systemd integration ([2588673](https://github.com/themadorg/madmail/commit/25886730908a715f1746ebccd25681df4c82c7e6))


### Features

* **cli:** enhance command-line interface with new features and improvements ([bbd84b8](https://github.com/themadorg/madmail/commit/bbd84b810c96acfa9a2355a7ac02a4e5190900db))

## [2.4.1](https://github.com/themadorg/madmail/compare/v2.4.0...v2.4.1) (2026-06-10)


### Bug Fixes

* **deps:** update dependencies in Cargo.lock and Cargo.toml ([a984ddc](https://github.com/themadorg/madmail/commit/a984ddc10dbfbc27485f6810cff9cc6eae50387f))

# [2.4.0](https://github.com/themadorg/madmail/compare/v2.3.1...v2.4.0) (2026-06-10)


### Features

* **certificates:** implement in-process Let's Encrypt certificate renewal for autocert mode ([fe39ef4](https://github.com/themadorg/madmail/commit/fe39ef403bcbd73012ef49f3e4f3355061fc84ea))
* **ci:** add Docker build and push workflow ([1e531d5](https://github.com/themadorg/madmail/commit/1e531d569508ec9ac045f9d46e278a0362193b18))
* **push:** implement Delta Chat push notifications support ([5f267f5](https://github.com/themadorg/madmail/commit/5f267f568d5e9a5e12a5fe98881377b508b516cf))
* **reload:** implement reload functionality for HTTP routes and server management ([87c0dda](https://github.com/themadorg/madmail/commit/87c0dda2d510d8fe77b0fcf21914b038c57fa036))

## [2.3.1](https://github.com/themadorg/madmail/compare/v2.3.0...v2.3.1) (2026-06-10)


### Bug Fixes

* rename chatmail to madmail across the codebase ([53944b9](https://github.com/themadorg/madmail/commit/53944b9d16cd31ee04af30f3b24899d0ae016c15))

# [2.3.0](https://github.com/themadorg/madmail/compare/v2.2.2...v2.3.0) (2026-06-09)


### Features

* **chatmail:** add CPU profiling support and storage policy configuration ([343aa31](https://github.com/themadorg/madmail/commit/343aa314edf2f4db3baeb37e9e46c4b8bc9753aa))

## [2.2.2](https://github.com/themadorg/madmail/compare/v2.2.1...v2.2.2) (2026-05-30)


### Bug Fixes

* **ci:** Correct Cargo.lock version extraction in release verify step ([7a9a0ce](https://github.com/themadorg/madmail/commit/7a9a0cefb1255acc1ac2d8c97e75fa4802611803))

## [2.2.1](https://github.com/themadorg/madmail/compare/v2.2.0...v2.2.1) (2026-05-30)


### Bug Fixes

* **release:** Bump chatmail version to 9.9.9 in Cargo.lock ([6206e04](https://github.com/themadorg/madmail/commit/6206e04ddfdc28f87e77b0422bbc0ad87eb0a372))

# [2.2.0](https://github.com/themadorg/madmail/compare/v2.1.0...v2.2.0) (2026-05-30)


### Features

* **release:** Integrate semantic-release for automated versioning and GitHub releases ([2925c76](https://github.com/themadorg/madmail/commit/2925c765a0560fec5934aef418fa99f1cf52a156))
