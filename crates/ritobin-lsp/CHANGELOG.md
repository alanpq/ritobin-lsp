# Changelog

## [0.1.14](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.13...ritobin-lsp-v0.1.14) (2026-08-19)


### Features

* code actions + new lint ([#64](https://github.com/alanpq/ritobin-lsp/issues/64)) ([4422a19](https://github.com/alanpq/ritobin-lsp/commit/4422a1991c0790b4cec1af08346ad56832f45a50))


### Bug Fixes

* load wad hashes ([#62](https://github.com/alanpq/ritobin-lsp/issues/62)) ([aea6a45](https://github.com/alanpq/ritobin-lsp/commit/aea6a45763a22aa56a0bc3bcaa1949d80f5f1c5b))

## [0.1.13](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.12...ritobin-lsp-v0.1.13) (2026-08-18)


### Features

* meta wiki documentation in hover & completion requests ([#58](https://github.com/alanpq/ritobin-lsp/issues/58)) ([0ab6781](https://github.com/alanpq/ritobin-lsp/commit/0ab6781566b4d9be130ad9cf8894c1c134dc6715))


### Bug Fixes

* **lsp:** correct class scope tracking ([#57](https://github.com/alanpq/ritobin-lsp/issues/57)) ([f79bde0](https://github.com/alanpq/ritobin-lsp/commit/f79bde0b848a964c53382c17a67fb5139183beb1))

## [0.1.12](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.11...ritobin-lsp-v0.1.12) (2026-08-13)


### Features

* better semantic tokens ([884e0ba](https://github.com/alanpq/ritobin-lsp/commit/884e0ba9aa5607248d8a513ac2a020dcb22ba1fd))
* **lsp:** batch document changes ([fa90df2](https://github.com/alanpq/ritobin-lsp/commit/fa90df2a62ec3e53e0ebe6d7e1d26e58a2b29c4e))
* **lsp:** implement wide character support ([106cfc1](https://github.com/alanpq/ritobin-lsp/commit/106cfc198dba557b96d00bf5b481c409863fcde5))
* **lsp:** semantic token deltas ([cbfe4e1](https://github.com/alanpq/ritobin-lsp/commit/cbfe4e107de9b4e7c9d45e77a39e8e59f21d96c7))
* **lsp:** static type indices ([4518f3f](https://github.com/alanpq/ritobin-lsp/commit/4518f3fb83acac632aaec20d7fd6cc36e6195611))
* property value auto-complete ([3f3d7a1](https://github.com/alanpq/ritobin-lsp/commit/3f3d7a1a17e711ddeb33f7cf0c0d15ecf8595318))


### Bug Fixes

* account for EntryTerminator in completion context resolution ([f232dda](https://github.com/alanpq/ritobin-lsp/commit/f232dda494e72388c0ecc5a7c448f4070ea4bee7))
* lol_meta schema change ([7622f6b](https://github.com/alanpq/ritobin-lsp/commit/7622f6b31630b07110d49d7f243c266a3d45b691))
* **lsp:** handle parser and typechecker panics ([4dc17c0](https://github.com/alanpq/ritobin-lsp/commit/4dc17c0802862b9847e8c20b12d57d3c3763eaad))
* **lsp:** worker cleanup on document close ([8b5c6ba](https://github.com/alanpq/ritobin-lsp/commit/8b5c6ba8cf59412a4a88c47f5eaefe9b30f59269))
* update lol-meta schema ([#45](https://github.com/alanpq/ritobin-lsp/issues/45)) ([2684b09](https://github.com/alanpq/ritobin-lsp/commit/2684b0946e97421840c9081260be064cff5eb67e))


### Performance Improvements

* **lsp:** debounce and split diagnostics from parsing ([bf1536c](https://github.com/alanpq/ritobin-lsp/commit/bf1536cc949503b1f0954d35540d125097499a29))
* **lsp:** line index splicing ([bbf4770](https://github.com/alanpq/ritobin-lsp/commit/bbf4770d29684f734f42343fc3967c24ee7b8b83))
* **lsp:** start diag debounce before parsing ([cd779fc](https://github.com/alanpq/ritobin-lsp/commit/cd779fc26dd5fb558add96c6697c091d20671bdd))
* prune out-of-range subtrees in semanticTokens/range ([840d9fc](https://github.com/alanpq/ritobin-lsp/commit/840d9fcbbda18031b0073cbe06075f7bdf550a9e))

## [0.1.11](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.10...ritobin-lsp-v0.1.11) (2026-07-23)


### Features

* configurable diagnostic limit + tweak err ordering ([e27a29f](https://github.com/alanpq/ritobin-lsp/commit/e27a29f41639de9d59f6c375fe74377aa69e520d))

## [0.1.10](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.9...ritobin-lsp-v0.1.10) (2026-07-17)


### Bug Fixes

* raise max diags -&gt; 1000 + push lint warns after cst/bin errors ([#37](https://github.com/alanpq/ritobin-lsp/issues/37)) ([59a1730](https://github.com/alanpq/ritobin-lsp/commit/59a173015c60a12486df09d571c2a223848022e2))

## [0.1.9](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.8...ritobin-lsp-v0.1.9) (2026-07-17)


### Features

* use hash provider when direct opening bins ([092e66a](https://github.com/alanpq/ritobin-lsp/commit/092e66a6ce10130f221741d025fa3dc7ff6d6f84))

## [0.1.8](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.7...ritobin-lsp-v0.1.8) (2026-07-17)


### Features

* add explorer integration and vfs scheme for .bin files ([9217b8e](https://github.com/alanpq/ritobin-lsp/commit/9217b8e06619d0aa8fabd0140dff0f69d5b9e05e))
* fmt CustomSpan ([3a8dc49](https://github.com/alanpq/ritobin-lsp/commit/3a8dc4936213fca7525ea0b0b9f455f007d5b95f))
* human readable NotEnoughItems ([c3f4f00](https://github.com/alanpq/ritobin-lsp/commit/c3f4f00e091056d63ed600687dfaba25e3773354))
* initial linter w/ unknown field lint ([350e24f](https://github.com/alanpq/ritobin-lsp/commit/350e24f12d11df8897e90a6e8b0f6369263b6833))
* switch to mimir, hash auto update ([8e453ad](https://github.com/alanpq/ritobin-lsp/commit/8e453ad8449170823fb376bb86967eb8c3bdecb1))
* update ltk ([cb3c584](https://github.com/alanpq/ritobin-lsp/commit/cb3c584bde57416dd09b3179632fe7f284952e25))
* update mimir ([30bfc64](https://github.com/alanpq/ritobin-lsp/commit/30bfc644ea3403dd56a16591b103b51e178b5172))


### Bug Fixes

* remove debug logs ([432b16c](https://github.com/alanpq/ritobin-lsp/commit/432b16ca8a10924b276f1b1ab6ac7d8a101cd3ed))
* replace poro_hash with latest ltk_hash ([8156c06](https://github.com/alanpq/ritobin-lsp/commit/8156c06dc0701c35a460f8b5e6615df6235b6e7a))

## [0.1.7](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.6...ritobin-lsp-v0.1.7) (2026-07-10)


### Features

* better format diffing ([9de2846](https://github.com/alanpq/ritobin-lsp/commit/9de2846c1aa92c5292e031ce968562b12a9d6c0d))
* human readable ParseNumericError ([ee08d17](https://github.com/alanpq/ritobin-lsp/commit/ee08d17eb6d63c80efa44da70c517ac6c5749f14))
* update to latest ltk ([ecf2ecd](https://github.com/alanpq/ritobin-lsp/commit/ecf2ecd5d5d51500f20182b30a06b19a68256f36))

## [0.1.6](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.5...ritobin-lsp-v0.1.6) (2026-06-14)


### Bug Fixes

* update to latest ltk crates + vsc package lock ([9dcaabb](https://github.com/alanpq/ritobin-lsp/commit/9dcaabb864ce6438bf8dec69079594cdfd831bac))

## [0.1.5](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.4...ritobin-lsp-v0.1.5) (2026-04-16)


### Features

* better meta dump management + auto fetch latest ([#27](https://github.com/alanpq/ritobin-lsp/issues/27)) ([9b84c3a](https://github.com/alanpq/ritobin-lsp/commit/9b84c3a982df913bcfb1e971e6620c35776d0ed0))

## [0.1.4](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.3...ritobin-lsp-v0.1.4) (2026-03-31)


### Features

* bump ltk ([adeb368](https://github.com/alanpq/ritobin-lsp/commit/adeb368d7e9eec7e83f007ace48dc64e2e270b5f))
* class entry hover ([2c5fbac](https://github.com/alanpq/ritobin-lsp/commit/2c5fbac1a0941ae03340be7569ba8161fa60fe2f))
* class token hover ([a4141d2](https://github.com/alanpq/ritobin-lsp/commit/a4141d22210dbc799a6b9a64b06b3470dd3ddba7))
* diff formatted output ([cd4fb67](https://github.com/alanpq/ritobin-lsp/commit/cd4fb679aed184ba536f56540b3c4390ab212a4b))
* document worker refactor ([99e4f07](https://github.com/alanpq/ritobin-lsp/commit/99e4f073415201b2fbd4f99b699a2309c8c32e1c))
* enable incremental document sync ([601e769](https://github.com/alanpq/ritobin-lsp/commit/601e769c2daef40fbaa7923882fcf33141fb8636))
* lol_meta service ([3494b5c](https://github.com/alanpq/ritobin-lsp/commit/3494b5ce138d6fa5c60bab9f99171558c8c06f08))
* rough and dirty hash lookups ([590cd78](https://github.com/alanpq/ritobin-lsp/commit/590cd780720601f224a9a0d7e7105f65f0b5bdc0))
* support UnexpectedContainerItem diagnostic ([6587448](https://github.com/alanpq/ritobin-lsp/commit/6587448ba0907c1ad173d478f456cbfbb933dd19))
* take paths as lsp config ([67b4ba4](https://github.com/alanpq/ritobin-lsp/commit/67b4ba42265816ac5f650514674b55835944071c))
* unhash command ([d19f28a](https://github.com/alanpq/ritobin-lsp/commit/d19f28a6768d1e7af32f41e2c4550be15b296b96))


### Bug Fixes

* class hierarchy indentation ([29bef45](https://github.com/alanpq/ritobin-lsp/commit/29bef45a9396fb3b46e3b847f5dea2b2995d583a))
* comment ClassFinder debug logs ([08f1c28](https://github.com/alanpq/ritobin-lsp/commit/08f1c2835fb816d4ddbeb3725c62fe913e209b6f))
* fallback hover to cst walk ([0089680](https://github.com/alanpq/ritobin-lsp/commit/0089680efa233817e197886ca92b744cf73df45f))
* more versatile ClassFinder ([94fee59](https://github.com/alanpq/ritobin-lsp/commit/94fee5914fccde6bbe5da90bb54bb43793b40ec5))
* show class property hash in hover ([743c10f](https://github.com/alanpq/ritobin-lsp/commit/743c10fa53eaa2ff9f451b7f93a4c0cabbd2689e))
* stop advertising definition provider support ([211cde9](https://github.com/alanpq/ritobin-lsp/commit/211cde97c6710d1e47edd406a13ab490544a8f0b))
* store built Bin ([8202742](https://github.com/alanpq/ritobin-lsp/commit/820274222e5dec72d08f6065cbc092ffb39ccc88))
* tweak format diffing ([5d3017c](https://github.com/alanpq/ritobin-lsp/commit/5d3017c027d112f54fc4e187c6c2c7feb2089f50))

## [0.1.3](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.2...ritobin-lsp-v0.1.3) (2026-03-19)


### Features

* bump ltk ([5157b6a](https://github.com/alanpq/ritobin-lsp/commit/5157b6a16b64b95397dc9dc4098dca4b2f06e592))

## [0.1.2](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.1...ritobin-lsp-v0.1.2) (2026-03-19)


### Bug Fixes

* 10MiB format limit ([d3e7c25](https://github.com/alanpq/ritobin-lsp/commit/d3e7c257e00526a012a3d2efac30c8a4933fc9d5))
* test ([bcc7266](https://github.com/alanpq/ritobin-lsp/commit/bcc7266899fd7005865bf047524157d04e094fb7))
* update ltk ([ab91e8c](https://github.com/alanpq/ritobin-lsp/commit/ab91e8c0337bc4fe6acdc25cf8bf68c155ab7655))

## [0.1.1](https://github.com/alanpq/ritobin-lsp/compare/ritobin-lsp-v0.1.0...ritobin-lsp-v0.1.1) (2026-03-19)


### Features

* basic format support ([ba65879](https://github.com/alanpq/ritobin-lsp/commit/ba658795050fd0ccf80e505820423527941b1b78))
* bump ltk ([a4cf561](https://github.com/alanpq/ritobin-lsp/commit/a4cf56178d3c42e94ff8cdf1e27f9ff1416db01a))
* don't format files &gt; ~5MiB ([9bc8bce](https://github.com/alanpq/ritobin-lsp/commit/9bc8bcec97ce430390ad0dee51e1da22120824e2))


### Bug Fixes

* bump ltk_ritobin ([be2a364](https://github.com/alanpq/ritobin-lsp/commit/be2a36423eed90cb3cd836f0aede0de0f68ce6c4))
* bump max format limit ([13fdc9c](https://github.com/alanpq/ritobin-lsp/commit/13fdc9ce9af6bc4ec9249c5868d99b89f2a261c5))
* disable useless log ([e0fd7f1](https://github.com/alanpq/ritobin-lsp/commit/e0fd7f1e3da07b503c1870e9d6edc6d7464b4a24))

## 0.1.0 (2026-01-16)


### Features

* export parse errors + basic type checking ([c941e13](https://github.com/alanpq/ritobin-lsp/commit/c941e13237e2e93fbe26f10a3b58a7337f165052))
* fmt for RootNonEntry diag ([219be50](https://github.com/alanpq/ritobin-lsp/commit/219be50b713f19b21d38e3f7e7697760d4ca7807))
* handle reqs/notifs on new threads ([957803a](https://github.com/alanpq/ritobin-lsp/commit/957803ad1d8f6022bc15b8df8bcc141dc95494fb))
* hello world ([2d888d8](https://github.com/alanpq/ritobin-lsp/commit/2d888d8ad8bf95afdc62f6ada278e733f9e17f25))
* hex literal highlighting ([71ae33d](https://github.com/alanpq/ritobin-lsp/commit/71ae33de98d5fa87dae13baf8b694ed12d42931b))
* LineNumbers helper methods ([0849989](https://github.com/alanpq/ritobin-lsp/commit/0849989884058941af1bf4bec6ed1cc131c7cf3e))
* more diagnostics ([816fa30](https://github.com/alanpq/ritobin-lsp/commit/816fa30b9583d73254b8e3b393523587086b1bfb))
* more type "checking" fun ([ca718d6](https://github.com/alanpq/ritobin-lsp/commit/ca718d64145535647ba4065ded9ef818761554a3))
* real semantic tokens ([747703d](https://github.com/alanpq/ritobin-lsp/commit/747703de85c5b30f3831e50a9ad0c3bfd45b655a))
* semantic range requests ([03ddc1f](https://github.com/alanpq/ritobin-lsp/commit/03ddc1f1c33e0399550c6d6a5491fb9848d8e577))
* steal minimal example from lsp-server ([954857d](https://github.com/alanpq/ritobin-lsp/commit/954857d7eecd4850737851bc29c7a4c6cf5f2c48))
* use new type checker ([01fe956](https://github.com/alanpq/ritobin-lsp/commit/01fe95658d45b3d58ca781a9721dc3df1fd2c95b))


### Bug Fixes

* line endings ([f90323d](https://github.com/alanpq/ritobin-lsp/commit/f90323df8ab39e23fcc200806ca1bed59dd06442))
* match new span struct ([ea85a2e](https://github.com/alanpq/ritobin-lsp/commit/ea85a2ecbbcb94534fc0c328de891d5dd837a16f))
* new visitor pattern ([81e1900](https://github.com/alanpq/ritobin-lsp/commit/81e1900ce694f1498bf70a464f7801bd3c3875cb))
* truncate diagnostics to 20 for safety ([454b5ff](https://github.com/alanpq/ritobin-lsp/commit/454b5ff27d71980f979e77af193b2feb9fb698b9))
* tweak semantic tokens ([c315a10](https://github.com/alanpq/ritobin-lsp/commit/c315a106a286b9891a72e443aca97eddb8d575af))
* updated TypeChecker api ([e0f0257](https://github.com/alanpq/ritobin-lsp/commit/e0f0257188f7f40f8e0c7e1f0a38287532f08935))
* use git branch for ltk_ritobin ([9378781](https://github.com/alanpq/ritobin-lsp/commit/9378781983a7683c834aa84418dbec28963d8061))
* working lsp events ([dcd6339](https://github.com/alanpq/ritobin-lsp/commit/dcd63395494e6b8bc65be3bc608ad375b86bd36b))
