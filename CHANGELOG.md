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
