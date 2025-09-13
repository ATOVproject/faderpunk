# Changelog

## [0.4.0](https://github.com/ATOVproject/faderpunk/compare/configurator-v0.3.0...configurator-v0.4.0) (2025-09-13)


### Features

* **app:** add color and icon config to apps ([35d19f9](https://github.com/ATOVproject/faderpunk/commit/35d19f92412597c0cb090c60d2c2ed06b4688342))
* **clock:** add analog clock out from internal clock ([7c3b619](https://github.com/ATOVproject/faderpunk/commit/7c3b619545862a5e22bd65f07dd9c37c0e3ca7c4))
* **clock:** add really long clock divisions ([1a15f70](https://github.com/ATOVproject/faderpunk/commit/1a15f70c0bf96e1b3351e92d5a31a69c9084b6df))
* **clock:** add reset out aux config option ([d021133](https://github.com/ATOVproject/faderpunk/commit/d02113302ed7f3cd45837acd013ff6b35e96eb3c))
* **configurator:** add note param (in case we need it) ([22f50b3](https://github.com/ATOVproject/faderpunk/commit/22f50b368c90d894d3ee6f791fe342f732906b52))
* **configurator:** add range param ([f5014a0](https://github.com/ATOVproject/faderpunk/commit/f5014a0ee0a53ffa0102d5e39f9750813ebf2ef6))
* select color and icons for all app. Rework app order ([e97a390](https://github.com/ATOVproject/faderpunk/commit/e97a390490ff0f9187f809f8231f308718efab98))


### Bug Fixes

* **clock:** prevent drift and stutter while changing bpm ([da6af19](https://github.com/ATOVproject/faderpunk/commit/da6af19d93e2e8b9ac3bd4814442ac2bfdda9238))
* **configurator:** use stream parsing for cobs frames ([7d2b604](https://github.com/ATOVproject/faderpunk/commit/7d2b604dd94b06a5bde68a6fbcb8cbb8227f4649))
* **quantizer:** use the first 16 scale from o_C ([9a0e5c7](https://github.com/ATOVproject/faderpunk/commit/9a0e5c7ae073458aee048ab5aa3ddba1b1bb5131))

## [0.3.0](https://github.com/ATOVproject/faderpunk/compare/configurator-v0.2.1...configurator-v0.3.0) (2025-09-04)


### Features

* **api:** new color api and improved color consistency ([056761f](https://github.com/ATOVproject/faderpunk/commit/056761ff42a336f8836da01ec7a58c773b6e5598))
* **config:** introduce more global settings, config task loop ([17e48d4](https://github.com/ATOVproject/faderpunk/commit/17e48d4a9f1fcf43130984e9adaa0505c5e2dae6))


### Bug Fixes

* **clock:** adjust clock config only when it was changed ([9d53f36](https://github.com/ATOVproject/faderpunk/commit/9d53f36edf53b4cd33089df2a9dac831d012eab1))
* **configurator:** add all colors to configurator ([2fee27d](https://github.com/ATOVproject/faderpunk/commit/2fee27d80254cb632c97dc5a0c8915218978d4f3))
* **configurator:** refresh layout after setting it ([8db5827](https://github.com/ATOVproject/faderpunk/commit/8db58270e173ab628aa857634bf7ee6ca686ff22))
* **configurator:** send correct layout with channel sizes ([c1310c6](https://github.com/ATOVproject/faderpunk/commit/c1310c61547f6457d36bb37b9d847f6e1985b62a))

## [0.2.1](https://github.com/ATOVproject/faderpunk/compare/configurator-v0.2.0...configurator-v0.2.1) (2025-08-21)


### Bug Fixes

* **configurator:** fix color param not being sent ([9ba5bc9](https://github.com/ATOVproject/faderpunk/commit/9ba5bc90c3f8f7cfe6ddf721e7f45ae085234d3e))

## [0.2.0](https://github.com/ATOVproject/faderpunk/compare/configurator-v0.1.0...configurator-v0.2.0) (2025-08-21)


### Features

* add and set params for apps ([55317b9](https://github.com/ATOVproject/faderpunk/commit/55317b90ed6b0cb6c315737603fbe55b6cc37220))
* add HeroUI based suuuuper basic configurator ([6c9d8f8](https://github.com/ATOVproject/faderpunk/commit/6c9d8f883761ea245638a462122535bff55e4091))
* add postcard encoded app config list ([e8889cd](https://github.com/ATOVproject/faderpunk/commit/e8889cdf681f7d432e7dd9eb648a76410ab0928d))
* **config:** retrieve app state from configurator ([1b9d105](https://github.com/ATOVproject/faderpunk/commit/1b9d10513b0fccf923d367e88b76872f50467938))
* **config:** separate layout from global config ([54d8690](https://github.com/ATOVproject/faderpunk/commit/54d869014c2299812519a4b47cc0b8a9a069a09f))
* **config:** set a param from configurator ([de47407](https://github.com/ATOVproject/faderpunk/commit/de47407a0ea913dcefe5767019b7a988b2661d00))
* **configurator:** deploy to Github pages ([a84aa5f](https://github.com/ATOVproject/faderpunk/commit/a84aa5f0d548b33d78e2722e2de2ae2b764ae791))
* **configurator:** implement layout setting ([17cb7b3](https://github.com/ATOVproject/faderpunk/commit/17cb7b338c8764302ada0ed4b54e7c74fbd5e2db))
* **configurator:** set custom layouts ([8902af6](https://github.com/ATOVproject/faderpunk/commit/8902af6f3f433e0046f3a445e4d1d1ed91483a10))
* decode large configuration messages ([e415f13](https://github.com/ATOVproject/faderpunk/commit/e415f13e740f2ac7efae0b40bdc85e65598376de))
* implement dynamic scene changes ([0a12ed6](https://github.com/ATOVproject/faderpunk/commit/0a12ed65d04c60a72a0a9dc9b218d6b34c605894))
* make max and midi channels CriticalSectionRawMutex Channels ([e0617e5](https://github.com/ATOVproject/faderpunk/commit/e0617e556b9a887034b695d6cd118cb8672d4d64))
* move param handler into param store ([27aee71](https://github.com/ATOVproject/faderpunk/commit/27aee71d40f784e74e65201195e7d071e3d9fca0))
* **params:** add Color param component in configurator ([8428a20](https://github.com/ATOVproject/faderpunk/commit/8428a2069de88721c4c2373792bc46f95794d57b))
* set clock sources using the configurator ([08f5312](https://github.com/ATOVproject/faderpunk/commit/08f53126e9e02a33855cb07861ad49d1c4b3c8cc))
* show params in configurator temp page ([99f8e69](https://github.com/ATOVproject/faderpunk/commit/99f8e696ff35a273907058d69d09a4ed2c1d87f2))
* **usb:** establish basic webusb connection ([6f3a418](https://github.com/ATOVproject/faderpunk/commit/6f3a4183bc3ab75ac49c3c28462d2f952a51ceee))
* **usb:** fix webusb windows compatibility ([fb01f98](https://github.com/ATOVproject/faderpunk/commit/fb01f981c64beb133b50f6072ae73fe30f113e3b))
* use batch messages for app listing ([da76ce1](https://github.com/ATOVproject/faderpunk/commit/da76ce1f72f577b91a74a1f3b4c101f88b33cfa9))


### Bug Fixes

* **configurator:** adjust for i2c global params ([0378d9b](https://github.com/ATOVproject/faderpunk/commit/0378d9b49e18e37b0179a113acb33ce53192f07d))
* **configurator:** adjust transformValues for 8 params ([34c7e08](https://github.com/ATOVproject/faderpunk/commit/34c7e0865c7476c1535dd17d778e71f093751869))
* **configurator:** check in pnpm-lock.yaml ([cc564fd](https://github.com/ATOVproject/faderpunk/commit/cc564fdc36461a7c818a7364ec19adf0e5bd2a64))
* restructure GlobalConfig to be Serialize, Deserialize ([b69c2ff](https://github.com/ATOVproject/faderpunk/commit/b69c2ff00d051807032c862c7e4320439dbb04e5))
