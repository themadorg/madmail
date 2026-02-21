## [0.17.1](https://github.com/themadorg/madmail/compare/v0.17.0...v0.17.1) (2026-02-21)


### Bug Fixes

* **docker:** switch to GHCR, add deployment examples and docs ([fce0579](https://github.com/themadorg/madmail/commit/fce05794813fb0b79ad3128149b9758dae949918))

# [0.17.0](https://github.com/themadorg/madmail/compare/v0.16.1...v0.17.0) (2026-02-20)


### Features

* align shadowsocks URL with frontend and add QR to landing page ([5e81cb3](https://github.com/themadorg/madmail/commit/5e81cb3869e76b92c93799296dcbdac3211d634d))

## [0.16.1](https://github.com/themadorg/madmail/compare/v0.16.0...v0.16.1) (2026-02-19)


### Bug Fixes

* add admin api documentation and web admin references ([35bf8a0](https://github.com/themadorg/madmail/commit/35bf8a0855a6b7416a377ecaf3bbf51de2f597ed))

# [0.16.0](https://github.com/themadorg/madmail/compare/v0.15.3...v0.16.0) (2026-02-19)


### Bug Fixes

* **lint:** remove deprecated rand.Seed call for Go 1.20+ compatibility ([1f914c6](https://github.com/themadorg/madmail/commit/1f914c6f02f9ed09db17bdbba812006717834424))
* **tests:** update remote tests to use new SMTPServerSTARTTLS signature and MailOptions ([bfa4040](https://github.com/themadorg/madmail/commit/bfa4040fefad012c158d7559952fd7c7662af3d2))


### Features

* **cli:** enhance admin-token output with API URL and GORM integration ([b83ac47](https://github.com/themadorg/madmail/commit/b83ac4733f46230802993a1c3d9f59c1552bc631))

# [0.16.0](https://github.com/themadorg/madmail/compare/v0.15.3...v0.16.0) (2026-02-19)


### Bug Fixes

* **tests:** update remote tests to use new SMTPServerSTARTTLS signature and MailOptions ([bfa4040](https://github.com/themadorg/madmail/commit/bfa4040fefad012c158d7559952fd7c7662af3d2))


### Features

* **cli:** enhance admin-token output with API URL and GORM integration ([b83ac47](https://github.com/themadorg/madmail/commit/b83ac4733f46230802993a1c3d9f59c1552bc631))

# [0.16.0](https://github.com/themadorg/madmail/compare/v0.15.3...v0.16.0) (2026-02-19)


### Features

* **cli:** enhance admin-token output with API URL and GORM integration ([b83ac47](https://github.com/themadorg/madmail/commit/b83ac4733f46230802993a1c3d9f59c1552bc631))

# [0.16.0](https://github.com/themadorg/madmail/compare/v0.15.3...v0.16.0) (2026-02-19)


### Features

* **cli:** enhance admin-token output with API URL and GORM integration ([b83ac47](https://github.com/themadorg/madmail/commit/b83ac4733f46230802993a1c3d9f59c1552bc631))

## [0.15.3](https://github.com/themadorg/madmail/compare/v0.15.2...v0.15.3) (2026-02-18)


### Bug Fixes

* add base path prefix for GitHub Pages deployment ([f9da1d8](https://github.com/themadorg/madmail/commit/f9da1d8408248d71b3e5cdeed9974620a5ea9180))

## [0.15.2](https://github.com/themadorg/madmail/compare/v0.15.1...v0.15.2) (2026-02-18)


### Bug Fixes

* deploy admin panel on every push to main ([a1a344e](https://github.com/themadorg/madmail/commit/a1a344e11f94d68b056795e762c1cb3192709cac))

## [0.15.1](https://github.com/themadorg/madmail/compare/v0.15.0...v0.15.1) (2026-02-18)


### Bug Fixes

* allow GitHub Pages deploy from main branch ([f49d610](https://github.com/themadorg/madmail/commit/f49d610d95c3afdf415000cbbce6d60e07a02a0c))

# [0.15.0](https://github.com/themadorg/madmail/compare/v0.14.2...v0.15.0) (2026-02-18)


### Bug Fixes

* auto-restart on port access toggle + upgrade reliability ([dddbf22](https://github.com/themadorg/madmail/commit/dddbf22687ffbcd95c6794ecaa5c97ca42bc5d17))


### Features

* admin API path config, last login tracking, and account dates ([937bf09](https://github.com/themadorg/madmail/commit/937bf092c831766b0a720fa3b357a9f2726cc59a))
* admin panel improvements and GitHub Pages deployment ([993386d](https://github.com/themadorg/madmail/commit/993386d6f6fea637012aa42e1fbd6e4ae67b6168))
* count received messages from external servers ([0527856](https://github.com/themadorg/madmail/commit/0527856ab389f23f1a0e8f38f7657215708b3975))
* enforce port access control (local-only) for all endpoints ([30bd7ae](https://github.com/themadorg/madmail/commit/30bd7aeac85094afa3cdcff2c696b9596fe482f1))
* message counters, outbound tracking, and quota management UI ([c9d6f80](https://github.com/themadorg/madmail/commit/c9d6f806df2e89317226e59c4a97e6893d69e1f7))
* multi-server credentials stored in IndexedDB ([7f4ce3a](https://github.com/themadorg/madmail/commit/7f4ce3acf68d0d8bc88fec07bca0be7248076220))

## [0.14.2](https://github.com/themadorg/madmail/compare/v0.14.1...v0.14.2) (2026-02-17)


### Bug Fixes

* remove security notes section from API docs and add admin API implementation ([baec364](https://github.com/themadorg/madmail/commit/baec3649d1a7313aff7e6996f1d3b5bd6da1b26f))

## [0.14.1](https://github.com/themadorg/madmail/compare/v0.14.0...v0.14.1) (2026-02-17)


### Bug Fixes

* add Admin API documentation page with endpoint reference ([bceeabe](https://github.com/themadorg/madmail/commit/bceeabefb4857a7e5b946c38b0409a1bbed15b06))
* refactor dns cache to use shared gorm db and fix ipv4 resolution ([94fa3c2](https://github.com/themadorg/madmail/commit/94fa3c212f5e3d5786ce46c431bfddfab19dbc49))
* remove security notes section from API docs and add admin API implementation ([0d7a7ba](https://github.com/themadorg/madmail/commit/0d7a7ba0d4d35631de9749e637c107a19f043bfe))

# [0.14.0](https://github.com/themadorg/madmail/compare/v0.13.2...v0.14.0) (2026-02-16)


### Bug Fixes

* add maddy online command documentation to admin docs ([08d4fbe](https://github.com/themadorg/madmail/commit/08d4fbe1aff6185db3203ce89b421a498aca52f7))
* remove hardcoded sqlite3 defaults for db driver/dsn ([77176cd](https://github.com/themadorg/madmail/commit/77176cdcad7fc2a2c50797dab9eea324afbb37ae))
* use GORM for user count query to support all database backends ([0bbfd02](https://github.com/themadorg/madmail/commit/0bbfd02497dced47e4517be1aa4d07ea673f49c0))
* use pre tags for multi-line sample outputs in admin docs ([7a24067](https://github.com/themadorg/madmail/commit/7a24067a86851df2145f0e290277b7298ecdc186))


### Features

* rename 'maddy online' to 'maddy status' and add registered user count ([db8ba9b](https://github.com/themadorg/madmail/commit/db8ba9ba89c2164d52f2f9dce53fc13de6e51477))

## [0.13.2](https://github.com/themadorg/madmail/compare/v0.13.1...v0.13.2) (2026-02-16)


### Bug Fixes

* add server tracker with boot time, unique sender tracking, and uptime display ([f9a05b8](https://github.com/themadorg/madmail/commit/f9a05b8b751fdcf0e9cac40ab4ebc29a5896157c))
* correct ss output field indices for TURN relay detection ([d5ef99b](https://github.com/themadorg/madmail/commit/d5ef99b2f85730baee769eb6ab042a032f8b2d89))
* detect TURN relay connections on ephemeral UDP ports ([20ba386](https://github.com/themadorg/madmail/commit/20ba3869da3dab0d1d5c23f2cca62cdafb228dd3))
* tighten server_tracker.json permissions to 0640 ([3316912](https://github.com/themadorg/madmail/commit/33169129d5d5a8b4b5c0eeb538f65dc5a07fb54e))

## [0.13.1](https://github.com/themadorg/madmail/compare/v0.13.0...v0.13.1) (2026-02-12)


### Bug Fixes

* in non-interactive installs, make --ip work and autogenerate TURN secret ([#32](https://github.com/themadorg/madmail/issues/32)) ([93b5f87](https://github.com/themadorg/madmail/commit/93b5f872971828a0787dd1eec287177e2884dd35))

# [0.13.0](https://github.com/themadorg/madmail/compare/v0.12.8...v0.13.0) (2026-02-11)


### Bug Fixes

* add version ([89b46f6](https://github.com/themadorg/madmail/commit/89b46f68e0c117bb1245764f293d77aca84cea83))


### Features

* DKIM HTTPS publishing and deploy improvements ([c94327d](https://github.com/themadorg/madmail/commit/c94327d25468369e3d5ae5a937a569d1de209c9b))

## [0.12.8](https://github.com/themadorg/madmail/compare/v0.12.7...v0.12.8) (2026-02-04)


### Bug Fixes

* **dist/apparmor:** extend rules to fit madmail ([#30](https://github.com/themadorg/madmail/issues/30)) ([1bdcb18](https://github.com/themadorg/madmail/commit/1bdcb18f1938d0264db0bcd24dd1534b0ea3a51e))

## [0.12.7](https://github.com/themadorg/madmail/compare/v0.12.6...v0.12.7) (2026-02-04)


### Bug Fixes

* **chatmail:** allow TURN and dynamic ports in Shadowsocks proxy ([2c9f40a](https://github.com/themadorg/madmail/commit/2c9f40a0dc06fb9b4775513f2e16bb8e119e7ed5))

## [0.12.6](https://github.com/themadorg/madmail/compare/v0.12.5...v0.12.6) (2026-02-03)


### Bug Fixes

* add iroh ([a147ea2](https://github.com/themadorg/madmail/commit/a147ea28a2e40198d6f49b151ef00a98c4c48cc8))

## [0.12.5](https://github.com/themadorg/madmail/compare/v0.12.4...v0.12.5) (2026-02-01)


### Bug Fixes

* **privacy:** ensure all imap logs respect "No Log Policy" by using managed logger ([9059763](https://github.com/themadorg/madmail/commit/9059763653e1c5fdf97c5f54d1aa29a42a13357a))

## [0.12.4](https://github.com/themadorg/madmail/compare/v0.12.3...v0.12.4) (2026-01-31)


### Bug Fixes

* implement imap-acct purge commands and fix storage stats ([875596d](https://github.com/themadorg/madmail/commit/875596d067bd99d3bf276ce9f2043b50b40b2458))

## [0.12.3](https://github.com/themadorg/madmail/compare/v0.12.2...v0.12.3) (2026-01-31)


### Bug Fixes

* **publish:** consolidate binary delivery and upgrade mechanism ([565c077](https://github.com/themadorg/madmail/commit/565c0772f85e26622314a4997df6200fdfad784d))

## [0.12.2](https://github.com/themadorg/madmail/compare/v0.12.1...v0.12.2) (2026-01-31)


### Bug Fixes

* **config:** refine maddy configuration and storage tables ([d97bdb7](https://github.com/themadorg/madmail/commit/d97bdb763d5714d0aae4f3603fd58d79ce94129d))
* **core:** vendor go-imap-sql and go-imap-mess ([d3d4589](https://github.com/themadorg/madmail/commit/d3d45895ceda5a232e92739e085efed7a7c39268))
* **deps:** update and sync dependencies ([617454f](https://github.com/themadorg/madmail/commit/617454ffe61e4e97ddbb1b060ceb59ac526d9f6c))
* **tests:** implement comprehensive lxc stress testing ([cdd4ae7](https://github.com/themadorg/madmail/commit/cdd4ae745aa78b44e27c8f8c7e03f150898b4bee))

## [0.12.1](https://github.com/themadorg/madmail/compare/v0.12.0...v0.12.1) (2026-01-24)


### Bug Fixes

* **install:** support non-root local installation and absolute paths ([4f0f1f9](https://github.com/themadorg/madmail/commit/4f0f1f9c1bf9abefaf608f4d97cfb631cd480590))

# [0.12.0](https://github.com/themadorg/madmail/compare/v0.11.1...v0.12.0) (2026-01-20)


### Bug Fixes

* address SASL authentication bug and improve JIT pruning ([ef28a8d](https://github.com/themadorg/madmail/commit/ef28a8d2e6700d3806bf8f2daf9bc39940a30785))


### Features

* add unused account cleanup with configurable retention ([aeff7cb](https://github.com/themadorg/madmail/commit/aeff7cb3d3afc1e2de811f3411c22011d8d91c26))
* track user first login time ([5aafffb](https://github.com/themadorg/madmail/commit/5aafffb4a728322717f234eb655843a52fbacb92))

# [0.11.0](https://github.com/themadorg/madmail/compare/v0.10.3...v0.11.0) (2026-01-20)


### Features

* improve binary upgrade mechanism and revert update documentation to manual method ([84fb4b7](https://github.com/themadorg/madmail/commit/84fb4b7dc2db6d41b8d41c3a80a62f3fa60bc334))

# [0.10.0](https://github.com/themadorg/madmail/compare/v0.9.0...v0.10.0) (2026-01-20)


### Bug Fixes

* **chatmail:** suppress redundant delivery abort errors ([7bf3a8b](https://github.com/themadorg/madmail/commit/7bf3a8ba32b4e6e4b299464a67e1289cf7db326c))


### Features

* add JIT registration flag to control automatic account creation ([bdaa8eb](https://github.com/themadorg/madmail/commit/bdaa8ebf6fa86944ea712e774468b661509d8eeb))
* **jit:** enable auto_create for imapsql and update documentation ([17f60ea](https://github.com/themadorg/madmail/commit/17f60ea683bdc4e9bcb6849da9c713a378877a88))
* **secure-join:** relax pgp verification for multi-step handshakes ([8b16a79](https://github.com/themadorg/madmail/commit/8b16a7928955e327be5546a6e24665f3fd34b1d7))

# [0.9.0](https://github.com/themadorg/madmail/compare/v0.8.103+f3cfc40...v0.9.0) (2026-01-19)


### Features

* implement binary signing and secure upgrade mechanism ([f182fad](https://github.com/themadorg/madmail/commit/f182fadb86ed8c1b43f6f8655f616e17cf313270))
* setup semantic-release and fix linting error in upgrade ([1262b5a](https://github.com/themadorg/madmail/commit/1262b5aa75fe0e39bd58ca48639fc9367cec544b))
