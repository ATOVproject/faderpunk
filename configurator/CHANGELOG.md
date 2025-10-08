# Changelog

## [1.1.1](https://github.com/ATOVproject/faderpunk/compare/configurator-v1.1.0...configurator-v1.1.1) (2025-10-08)


### Bug Fixes

* **configurator:** disambiguate range names ([87947ef](https://github.com/ATOVproject/faderpunk/commit/87947eff463d2df42dd188c7e4e625f18bbcfc08))
* **configurator:** properly parse enum defaultValue ([00db2a0](https://github.com/ATOVproject/faderpunk/commit/00db2a0a3bf569ce80076519ba075b3a451232b6))

## [1.1.0](https://github.com/ATOVproject/faderpunk/compare/configurator-v1.0.0...configurator-v1.1.0) (2025-09-25)


### Features

* **configurator:** rename params, fix float field ([9349aa6](https://github.com/ATOVproject/faderpunk/commit/9349aa624432e3aef66b71a7a1a19e2b40dacef8))


### Bug Fixes

* **clock:** limit extra reset sources ([7fc8619](https://github.com/ATOVproject/faderpunk/commit/7fc861910648376d5f7963214c1c6f2a33df7bd5))
* **configurator:** add about tab and attributions ([8d9ab89](https://github.com/ATOVproject/faderpunk/commit/8d9ab8931922e0896094a5cd518bd5de71b207ca))

## [1.0.0](https://github.com/ATOVproject/faderpunk/compare/configurator-v0.4.0...configurator-v1.0.0) (2025-09-20)


### ⚠ BREAKING CHANGES

* **configurator:** release configurator 1.0

### Features

* **configurator:** connect page, minor additions ([1cdc8fa](https://github.com/ATOVproject/faderpunk/commit/1cdc8fa2aa7c5317e34098bbccf467846a3ef4a7))
* **configurator:** release configurator 1.0 ([92e3091](https://github.com/ATOVproject/faderpunk/commit/92e30914e5ff6fb1166a851732133617dbcc89ac))
* **configurator:** remove old configurator ([b7a6e8d](https://github.com/ATOVproject/faderpunk/commit/b7a6e8dbf9178e843c263c4dd770563a45285b53))
* **configurator:** save global settings ([f4327d5](https://github.com/ATOVproject/faderpunk/commit/f4327d5cf02dc863f2a128905cf3f416ac6e40ce))


### Bug Fixes

* **configurator:** disable popover when dragging in layout ([63dc2ba](https://github.com/ATOVproject/faderpunk/commit/63dc2bae4d2ace8bd0af23505d5678ba0ef9c79e))
* **configurator:** properly check activeId against null ([90fb701](https://github.com/ATOVproject/faderpunk/commit/90fb701aa63a5194b88faac822afe6193f6b051a))

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
