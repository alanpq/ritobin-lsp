# Changelog

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
