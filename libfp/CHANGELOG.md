# Changelog

## [0.2.2](https://github.com/ATOVproject/faderpunk/compare/libfp-v0.2.1...libfp-v0.2.2) (2025-08-14)


### Bug Fixes

* **default:** fix curve, slew and bipolar recall ([968d4df](https://github.com/ATOVproject/faderpunk/commit/968d4dfca3812f1f3f4084d8a9448b81b70a7603))

## [0.2.1](https://github.com/ATOVproject/faderpunk/compare/libfp-v0.2.0...libfp-v0.2.1) (2025-08-08)


### Bug Fixes

* **api:** rename Sawinv to SawInv ([9b18e3c](https://github.com/ATOVproject/faderpunk/commit/9b18e3c5f6fd4134e83119d209608b06f5a863e0))

## [0.2.0](https://github.com/ATOVproject/faderpunk/compare/libfp-v0.1.0...libfp-v0.2.0) (2025-08-08)


### Features

* **constants:** introduced some standard LED colors and intensities ([2e2baa3](https://github.com/ATOVproject/faderpunk/commit/2e2baa3f92c27a83cb1f276791162070a4610914))
* **deps:** downgrade heapless in libfp ([8ee90ca](https://github.com/ATOVproject/faderpunk/commit/8ee90ca18c7aa34a187fcea6edf41f057809765a))
* **lfo:** add inverted saw waveform ([b32c7bc](https://github.com/ATOVproject/faderpunk/commit/b32c7bc923010eb65ae5a7ba5b0072cf674aebc5))
* **quantizer:** add quantizer utility ([201a47b](https://github.com/ATOVproject/faderpunk/commit/201a47b3dc9beeaefd57f0f84931c4565e129385))
* **sequencer:** add legato ([e60b3ea](https://github.com/ATOVproject/faderpunk/commit/e60b3ea0cc56dc7d0d5663d92db181f37b6a761f))
* **utils:** add clickless function as public ([819042b](https://github.com/ATOVproject/faderpunk/commit/819042b4f788d795168c841473c8dd4ca56fc96b))


### Bug Fixes

* **constants:** adjust rgb to design guide value ([7e47192](https://github.com/ATOVproject/faderpunk/commit/7e47192926c1e4a0db9fcb3bb31059befad5d838))

## 0.1.0 (2025-07-19)


### Features

* add gen-bindings, restructure project ([0628406](https://github.com/ATOVproject/faderpunk/commit/06284069ff090d442f921713c12f794181328aab))
* **calibration:** add output calibration over i2c ([d8b25a1](https://github.com/ATOVproject/faderpunk/commit/d8b25a1d09294f39396d8960110223bdc71d24a6))
* **calibration:** i2c proto ping pong ([2c1d190](https://github.com/ATOVproject/faderpunk/commit/2c1d190ccb7a76c5bc61cc96cae9749a6277a833))
* **configurator:** implement layout setting ([17cb7b3](https://github.com/ATOVproject/faderpunk/commit/17cb7b338c8764302ada0ed4b54e7c74fbd5e2db))
* **libfp:** add value transformation for Enum/usize ([354cc98](https://github.com/ATOVproject/faderpunk/commit/354cc9854b99208b14a8df37b6a34a3a1d556972))
* merge config crate into libfp ([d69da45](https://github.com/ATOVproject/faderpunk/commit/d69da45ed8b4a60fd020ce567328b348cf475319))
* move BrightnessExt to libfp ([0972e2d](https://github.com/ATOVproject/faderpunk/commit/0972e2d192cc615ebb831a273bf71dedaa7c2af0))
* simplify cross core message routing ([7030d14](https://github.com/ATOVproject/faderpunk/commit/7030d14cc1027c85a48fc73501f91bbe267496bb))
* **utils:** add attenuverter and slew_limiter ([c1c30f0](https://github.com/ATOVproject/faderpunk/commit/c1c30f071c615727c122f4d5196ce5448689ff31))
* **utils:** introduce some useful functions ([26432b4](https://github.com/ATOVproject/faderpunk/commit/26432b4f7b922dd988da904411f4d00642fcb1a3))


### Bug Fixes

* clock fixes and clock debug app ([2f39258](https://github.com/ATOVproject/faderpunk/commit/2f392588048dae6c361383c2fe4aac4ee508c464))
* **midi:** proper midi 1 implementation using midly ([ea38aca](https://github.com/ATOVproject/faderpunk/commit/ea38aca53bb42330f03e86fbb0a78933aeedeb91))
* **utils:** add clamps to spliters ([77301e1](https://github.com/ATOVproject/faderpunk/commit/77301e12ecb98787822de16729c31c17a60318b1))
