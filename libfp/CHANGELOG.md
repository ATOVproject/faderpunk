# Changelog

## [0.5.0](https://github.com/ATOVproject/faderpunk/compare/libfp-v0.4.0...libfp-v0.5.0) (2025-09-04)


### Features

* **api:** complete rework of app parameter implementation ([8a6f44d](https://github.com/ATOVproject/faderpunk/commit/8a6f44dcefe066d20a1db0e81c96a3fa3caa1832))
* **api:** new color api and improved color consistency ([056761f](https://github.com/ATOVproject/faderpunk/commit/056761ff42a336f8836da01ec7a58c773b6e5598))
* **api:** use new latch in default and lfo ([9abe1cd](https://github.com/ATOVproject/faderpunk/commit/9abe1cdf27c78bd8dfc72cb3b2b946b15d2ea95d))
* **config:** add ability to change global config via faders ([9374b77](https://github.com/ATOVproject/faderpunk/commit/9374b779429b8bb6f242dca0ae5078368ad4ecd5))
* **config:** introduce more global settings, config task loop ([17e48d4](https://github.com/ATOVproject/faderpunk/commit/17e48d4a9f1fcf43130984e9adaa0505c5e2dae6))
* **cv2midi:** add cv2midi app ([ab4864f](https://github.com/ATOVproject/faderpunk/commit/ab4864f3714c9907dc485aaf77893b2ae5cd3d09))
* **euclid:** add euclid app ([2eaff71](https://github.com/ATOVproject/faderpunk/commit/2eaff715aacbcf0c1e643768ce9a9cf8348f67e4))
* **latch:** add third latch layer ([c827400](https://github.com/ATOVproject/faderpunk/commit/c82740005ad0829f4fd7eee9ef80a01389dc23cf))
* **leds:** improve led brightness and color apis ([4ff24e2](https://github.com/ATOVproject/faderpunk/commit/4ff24e20b812fcbcaa332297c126f82b072e2848))
* **quantizer:** rewrite quantizer, make it more predictable ([0c14ef6](https://github.com/ATOVproject/faderpunk/commit/0c14ef6f9d8561f74b9b85f157de4acbeaf19c08))


### Bug Fixes

* **clock:** adjust clock config only when it was changed ([9d53f36](https://github.com/ATOVproject/faderpunk/commit/9d53f36edf53b4cd33089df2a9dac831d012eab1))
* **euclid:** change description ([549611e](https://github.com/ATOVproject/faderpunk/commit/549611e2aa7ba9f721e04164a477ca0f5d0e58fa))
* **latch:** add jitter tolerance to latch ([95f4c67](https://github.com/ATOVproject/faderpunk/commit/95f4c67167bf42f58413b5635404e048dcc39818))
* **latch:** unlatch when target value is changed externally ([b2caa67](https://github.com/ATOVproject/faderpunk/commit/b2caa67eda5c78b3e695b51ad33dfdf7ccc95ad7))
* **layout:** allow for holes in layout ([20ff5bc](https://github.com/ATOVproject/faderpunk/commit/20ff5bc92461369f145b13716ba3fe45f93e3e4c))
* validate layout after loading from fram ([848e2aa](https://github.com/ATOVproject/faderpunk/commit/848e2aa79130d134737f66309c023211e041f861))

## [0.4.0](https://github.com/ATOVproject/faderpunk/compare/libfp-v0.3.0...libfp-v0.4.0) (2025-08-23)


### Features

* **midi2cv:** add midi2cv prototype app ([c005ac1](https://github.com/ATOVproject/faderpunk/commit/c005ac1c0d0d7b4827dcde9ff5f7a7057a3b015f))


### Bug Fixes

* **calibration:** fixes for semi-automatic calibration ([932321b](https://github.com/ATOVproject/faderpunk/commit/932321bad07da39aaa704c64fcc023f7399ea835))

## [0.3.0](https://github.com/ATOVproject/faderpunk/compare/libfp-v0.2.2...libfp-v0.3.0) (2025-08-20)


### Features

* add -5V to 5V range to manual calibration ([f6cee85](https://github.com/ATOVproject/faderpunk/commit/f6cee85878316bb552e7ba28f405bb2b6b556fcb))
* **calibration:** add first version of automatic calibration ([2679d6b](https://github.com/ATOVproject/faderpunk/commit/2679d6b955d5b2e50e9ac3028050ecac5450f90a))
* **calibration:** move manual calibration to i2c startup ([83a0c03](https://github.com/ATOVproject/faderpunk/commit/83a0c03e97c0fba81c4545b0734cb066556f4e1e))
* **config:** separate layout from global config ([54d8690](https://github.com/ATOVproject/faderpunk/commit/54d869014c2299812519a4b47cc0b8a9a069a09f))
* **i2c:** prepare for i2c leader/follower/calibration modes ([2269d84](https://github.com/ATOVproject/faderpunk/commit/2269d841e35dd07a73397bd2a234977b944e2fc7))
* improve semi-automatic calibration ([71d1f4e](https://github.com/ATOVproject/faderpunk/commit/71d1f4e46590adc99d62477ad577860ae5554331))
* move Range to libfp ([a349b55](https://github.com/ATOVproject/faderpunk/commit/a349b55924c98180409e89da698f7b392b2b9323))
* **params:** add Color param for configurator ([a7b2ee6](https://github.com/ATOVproject/faderpunk/commit/a7b2ee65cca6d0047b82097bace0d895a24ce4d2))
* **params:** bump app max param size to 8 ([7900abc](https://github.com/ATOVproject/faderpunk/commit/7900abc2e749ac0311d6d2100eb5ed8b6c865325))
* **params:** use .into() instead of .get() for Color ([818391b](https://github.com/ATOVproject/faderpunk/commit/818391b30f2e99d281965a63a27f0e84031ead7b))

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
