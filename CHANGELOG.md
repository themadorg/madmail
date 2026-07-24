# [2.18.0](https://github.com/themadorg/madmail/compare/v2.17.3...v2.18.0) (2026-07-24)


### Features

* **windows:** installer stack (paths, service, firewall, tray, Inno, CI) ([#112](https://github.com/themadorg/madmail/issues/112)) ([79ed100](https://github.com/themadorg/madmail/commit/79ed100e90a07b601519e99fc9f50901b915c0b5)), closes [#99](https://github.com/themadorg/madmail/issues/99)

## [2.17.3](https://github.com/themadorg/madmail/compare/v2.17.2...v2.17.3) (2026-07-21)


### Bug Fixes

* **db:** decode blocked_at safely on Postgres ([#98](https://github.com/themadorg/madmail/issues/98)) ([03274c2](https://github.com/themadorg/madmail/commit/03274c2e7740414afad0bc7234231f9b8b047887)), closes [#97](https://github.com/themadorg/madmail/issues/97)

## [2.17.2](https://github.com/themadorg/madmail/compare/v2.17.1...v2.17.2) (2026-07-21)


### Bug Fixes

* **www:** post contact share form as urlencoded ([2ffc950](https://github.com/themadorg/madmail/commit/2ffc950832695523c94c8e5d9eda933a859a90e9)), closes [#94](https://github.com/themadorg/madmail/issues/94)

## [2.17.1](https://github.com/themadorg/madmail/compare/v2.17.0...v2.17.1) (2026-07-20)


### Bug Fixes

* **delivery:** rollback partial federated group queue writes ([4fe7c58](https://github.com/themadorg/madmail/commit/4fe7c582a0710c8c6f8a8f87c937ad2a15647085))


### Performance Improvements

* **delivery:** parallelize federated group fan-out ([bed3207](https://github.com/themadorg/madmail/commit/bed3207ccd068f6f4ae149dad2fd0906bf255cda))
* **delivery:** write federated group body once and hard-link per recipient ([1aad99b](https://github.com/themadorg/madmail/commit/1aad99b5065b676d90d9df547bddc3318ee182cb))

# [2.17.0](https://github.com/themadorg/madmail/compare/v2.16.2...v2.17.0) (2026-07-19)


### Bug Fixes

* **upgrade:** extract only madmail from tar.gz into traditional path ([20ed7e1](https://github.com/themadorg/madmail/commit/20ed7e1e37f0a66f73b3a31d39b84982ed3fa93f))
* **upgrade:** harden temp files and tar member path checks ([a822208](https://github.com/themadorg/madmail/commit/a8222086694a8623897fcf253f6686f95fe4d0b2))


### Features

* **upgrade:** support .tar.gz release archives in update/upgrade ([4af64c0](https://github.com/themadorg/madmail/commit/4af64c0640aec0918ca265a3950a5ec863c41e1e)), closes [#46](https://github.com/themadorg/madmail/issues/46)
* **upgrade:** verify TLS by default; --accept-unsafe or interactive y/N ([e54720e](https://github.com/themadorg/madmail/commit/e54720ecde2b5d6c9bbae14af627bac1efe099a8))

## [2.16.2](https://github.com/themadorg/madmail/compare/v2.16.1...v2.16.2) (2026-07-18)


### Bug Fixes

* **www:** correct UTF-8 corruption in Go-template-to-Minijinja conversion ([1bf1759](https://github.com/themadorg/madmail/commit/1bf17598f052cf2d8218260528f3c8150ae38a05))

## [2.16.1](https://github.com/themadorg/madmail/compare/v2.16.0...v2.16.1) (2026-07-16)


### Bug Fixes

* **webimap:** ship multi-mailbox search/create via green CI ([5a31600](https://github.com/themadorg/madmail/commit/5a316002f6d8e81937fba6dea1993f512229fbe6))

# [2.16.0](https://github.com/themadorg/madmail/compare/v2.15.0...v2.16.0) (2026-07-15)


### Bug Fixes

* **www:** convert Go if-not template actions for custom www_dir ([93d350d](https://github.com/themadorg/madmail/commit/93d350d074e6437bc3af584486ef935043736e3d))
* **www:** silence clippy on html-migrate walk/tests ([57a6301](https://github.com/themadorg/madmail/commit/57a63014e828ad4667385234cb487e159a41212d))


### Features

* **www:** migrate custom Go templates on update ([c857e23](https://github.com/themadorg/madmail/commit/c857e2380ccbdac046bf3459e8f4688e5a9e3142))

# [2.15.0](https://github.com/themadorg/madmail/compare/v2.14.0...v2.15.0) (2026-07-15)


### Features

* **logging:** flexible bool flags and maddy-compatible log destinations ([4c45778](https://github.com/themadorg/madmail/commit/4c45778ea6982013ea469e1370347b6dafd184f7))

# [2.14.0](https://github.com/themadorg/madmail/compare/v2.13.5...v2.14.0) (2026-07-13)


### Features

* **webimap:** implement multi-mailbox WS actions for madcore parity ([bc58b0c](https://github.com/themadorg/madmail/commit/bc58b0c63f2c4611c3077edff2fedad6adbc4dd2))

## [2.13.5](https://github.com/themadorg/madmail/compare/v2.13.4...v2.13.5) (2026-07-12)


### Bug Fixes

* resolve the webimap federation issue ([be1f1ff](https://github.com/themadorg/madmail/commit/be1f1fff82ff505a3ccd88615ecc29b84a0adebb))

## [2.13.4](https://github.com/themadorg/madmail/compare/v2.13.3...v2.13.4) (2026-07-12)


### Bug Fixes

* cors issue fix for the /new also ([897a7fb](https://github.com/themadorg/madmail/commit/897a7fb47385bdb3e695af9bce604e3800f5c768))

## [2.13.3](https://github.com/themadorg/madmail/compare/v2.13.2...v2.13.3) (2026-07-12)


### Bug Fixes

* **db:** add unique indexes on legacy schema ensure path ([f00fdaf](https://github.com/themadorg/madmail/commit/f00fdaf740fa0d5003c9768323d3e03bad4e23b8)), closes [#83](https://github.com/themadorg/madmail/issues/83) [#81](https://github.com/themadorg/madmail/issues/81) [#80](https://github.com/themadorg/madmail/issues/80) [#67](https://github.com/themadorg/madmail/issues/67)

## [2.13.2](https://github.com/themadorg/madmail/compare/v2.13.1...v2.13.2) (2026-07-11)


### Bug Fixes

* **db:** ensure tables on Go Madmail upgrade without bare sqlx migrate ([75e9051](https://github.com/themadorg/madmail/commit/75e9051a62daf473bc8696942cf4aa6d160eaf8d)), closes [#67](https://github.com/themadorg/madmail/issues/67) [#80](https://github.com/themadorg/madmail/issues/80) [#67](https://github.com/themadorg/madmail/issues/67)

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
