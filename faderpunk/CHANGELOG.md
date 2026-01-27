# Changelog

## [1.6.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.5.3...faderpunk-v1.6.0) (2026-01-27)


### Features

* **faderpunk:** bump version to align with configurator ([50cdb88](https://github.com/ATOVproject/faderpunk/commit/50cdb88e26dff2a567411c84605adeeed557d504))

## [1.5.3](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.5.2...faderpunk-v1.5.3) (2026-01-25)


### Bug Fixes

* add hardware factory reset ([e3fef02](https://github.com/ATOVproject/faderpunk/commit/e3fef022b1dbe497aed6c3875c32e3c481ca1297))

## [1.5.2](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.5.1...faderpunk-v1.5.2) (2026-01-07)


### Bug Fixes

* **control:** remove filtering on MIDI CC ([fb71d00](https://github.com/ATOVproject/faderpunk/commit/fb71d004ab19ab57df2464ef979a65335d73fe19))

## [1.5.1](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.5.0...faderpunk-v1.5.1) (2025-12-10)


### Bug Fixes

* **ad:** support zero-velocity note-offs ([c8efc0b](https://github.com/ATOVproject/faderpunk/commit/c8efc0beb4e9956504a3cb010eb5f2d56eaa96db))
* **midi2cv:** re-trigger gate on legato ([8bf3697](https://github.com/ATOVproject/faderpunk/commit/8bf3697f00c1bf14c1a967719258727eed0b1499))
* **midi2cv:** support zero-velocity note-offs ([95ed381](https://github.com/ATOVproject/faderpunk/commit/95ed38114a1850a341d11b2068fe553fb61709d3))

## [1.5.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.4.0...faderpunk-v1.5.0) (2025-12-07)


### Features

* add possibility to save and recall app layouts & params ([e724ac0](https://github.com/ATOVproject/faderpunk/commit/e724ac087893a3beb53ea4e4f39449dbb238ea68))
* **calibration:** switch to fully automatic calibration ([f88604d](https://github.com/ATOVproject/faderpunk/commit/f88604d641a5aa1b2d2998f12faee275d71950f2))
* **clock_div:** add clock divider app ([8214a38](https://github.com/ATOVproject/faderpunk/commit/8214a381734eb4eae5093b052888349c3a432995))
* **clock:** start with running clock & save clock state ([bcbf044](https://github.com/ATOVproject/faderpunk/commit/bcbf044a754e9903d7a8b45fdaa2a9061b8f4b69))
* **configurator:** add factory reset function ([85dc047](https://github.com/ATOVproject/faderpunk/commit/85dc047d3a8b463a746925df9de9c8778841c739))
* **midi2cv:** add velocity to `Gate` and `Note Gate` mode ([0d1af52](https://github.com/ATOVproject/faderpunk/commit/0d1af527f043444cdca86e41e00f9963fcdd0339))
* **midi:** send Out1 copy to MIDI Out2 ([275ba63](https://github.com/ATOVproject/faderpunk/commit/275ba632b151fb26a0b371142d97b9a5363e2d71))
* **panner:** add panner app ([d990c75](https://github.com/ATOVproject/faderpunk/commit/d990c752e5bdd51704a1323dfc798c50b7ba33e6))
* **turing & turing+:** add range param ([cb1602f](https://github.com/ATOVproject/faderpunk/commit/cb1602fdd701edf2459ac3dc074684d72b4865e8))
* **turing+:** add base note param ([3189ff9](https://github.com/ATOVproject/faderpunk/commit/3189ff95067b43a4ee659a6de979a5efd973d8ae))


### Bug Fixes

* **apps:** clean up latch layer antipattern ([e2afe6a](https://github.com/ATOVproject/faderpunk/commit/e2afe6a297565231ddafcdd67e5bd070a26a2875))
* **lfo:** add some variable safety ([beba31e](https://github.com/ATOVproject/faderpunk/commit/beba31ecf9884aa9817a40b5b60f12c6d9778a2f))
* never erase calibration range ([5280e94](https://github.com/ATOVproject/faderpunk/commit/5280e94d0c970063dce257e39c0c660459da8946))
* **panner:** fix crash when pressing button 2 and moving fader 1 ([e674d84](https://github.com/ATOVproject/faderpunk/commit/e674d84861ccd9e850a4322b4de8f7b782b2504b))
* **panner:** make LED color more consistent ([be56867](https://github.com/ATOVproject/faderpunk/commit/be56867e8ed34846f2e3b082e3ba3334c7fbcbcd))

## [1.4.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.3.0...faderpunk-v1.4.0) (2025-10-24)


### Features

* **control:** add param enabling fader value storage ([921b139](https://github.com/ATOVproject/faderpunk/commit/921b1393ee308c1630dc1994290cbba70a4dd8f0))
* **LFO:** add range selection, add MIDI output ([d8a50f9](https://github.com/ATOVproject/faderpunk/commit/d8a50f930cacc3b3303993061adad9edd1ea0b10))
* **offset_att:** make buttons toggles of offset and attenuverter ([5074d77](https://github.com/ATOVproject/faderpunk/commit/5074d777019765ea7c41ed24a1a410ce487f696b))


### Bug Fixes

* **apps:** use debounced save for storage ([05387a4](https://github.com/ATOVproject/faderpunk/commit/05387a423944e36a5def7be4df276739d5dd1fc8))
* **control:** actually fix the bug causing CC not reaching 127 ([7cb3288](https://github.com/ATOVproject/faderpunk/commit/7cb32889f703d1b91c37b0da3318c1c559d80623))
* **quantizer:** fix led feedback ([1c93c86](https://github.com/ATOVproject/faderpunk/commit/1c93c862662b39a2a18bf52194389e9ae26ab934))
* **randomcvcc:** fix bug preventing  going into free running mode ([420ce0e](https://github.com/ATOVproject/faderpunk/commit/420ce0ea4fa52043ec0fa5e3a6d57540284aa3ca))
* **turing & turing+:** fix MIDI CC number offset ([6783f71](https://github.com/ATOVproject/faderpunk/commit/6783f71e5cc07c299ea7845d7e64d7612604a37e))

## [1.3.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.2.2...faderpunk-v1.3.0) (2025-10-08)


### Features

* **clkcvrnd:** add slew to CV and bipolar param ([b0d5525](https://github.com/ATOVproject/faderpunk/commit/b0d552562c646393004f0461a4d60b25bc26e650))
* **control:** add invert param ([4304662](https://github.com/ATOVproject/faderpunk/commit/4304662f0f079a94f15a0449863066a3a2615ce5))
* **rndcvcc:** add free running mode ([a5dba5b](https://github.com/ATOVproject/faderpunk/commit/a5dba5b2b9215bdbb0a388f16b59aee7784c4460))
* **rndcvcc:** change clkcvrnd name to rndcvcc ([5638b80](https://github.com/ATOVproject/faderpunk/commit/5638b80b3e891fe62a4f0117173fc13f9b0cd1bd))


### Bug Fixes

* **clock:** always pass through analog ticks ([6ed1093](https://github.com/ATOVproject/faderpunk/commit/6ed109396ecb01c703e32a2b921cf23854eaafc7))
* **clock:** apply correct pull for analog clock inputs ([89e778b](https://github.com/ATOVproject/faderpunk/commit/89e778b477a2a31876a757426d04621403ccad69))
* **max:** reduce number of sample readings for adc ([577646e](https://github.com/ATOVproject/faderpunk/commit/577646e032bcc98b5a5d09eb58c9e7cbe4315da3))
* **midi:** add midi throttling to 500 messages per second ([bfb502d](https://github.com/ATOVproject/faderpunk/commit/bfb502d52ef54cc934263cfdc8c53786da12242b))
* **probatrigger:** param change making the app crash ([c8b233d](https://github.com/ATOVproject/faderpunk/commit/c8b233db5163029bee857f973d18c29412478018))
* **rndcvcc:** Change description ([edda804](https://github.com/ATOVproject/faderpunk/commit/edda804da43eff04339bef417ec4a1e04791fec0))
* **storage:** enable calibration data migration ([b8f140e](https://github.com/ATOVproject/faderpunk/commit/b8f140e1548559ea25032ef32868ae597800e0aa))

## [1.2.2](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.2.1...faderpunk-v1.2.2) (2025-10-01)


### Bug Fixes

* **midi2cv:** fix param size ([9362331](https://github.com/ATOVproject/faderpunk/commit/93623319a56ba4117437b47e19b4f195416f00b5))

## [1.2.1](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.2.0...faderpunk-v1.2.1) (2025-10-01)


### Bug Fixes

* **clkcvrnd:** fix crash on unmute ([152456d](https://github.com/ATOVproject/faderpunk/commit/152456d5fe6474dfe78a03216d7befcae74d2d5a))
* **clkcvrnd:** move color change to clock handler ([4aba760](https://github.com/ATOVproject/faderpunk/commit/4aba760da1566a447b9c2be58e0fa0d6911206d4))
* **euclid:** fix warping issues making it loose phase ([47e0ff1](https://github.com/ATOVproject/faderpunk/commit/47e0ff1a42eb018b4d2ca337f0dec777fa11ab07))

## [1.2.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.1.0...faderpunk-v1.2.0) (2025-09-25)


### Features

* **configurator:** rename params, fix float field ([9349aa6](https://github.com/ATOVproject/faderpunk/commit/9349aa624432e3aef66b71a7a1a19e2b40dacef8))
* **lfo:** add speed  param ([4c3cef2](https://github.com/ATOVproject/faderpunk/commit/4c3cef20f0b52515a395a8418aa0728e688b67bf))
* **turing:** add base note and gate % param ([f41908e](https://github.com/ATOVproject/faderpunk/commit/f41908e9ee75b89def7c91c78d48586acc3778dc))


### Bug Fixes

* **clkcvrnd:** fix LED on mute ([a78f3c7](https://github.com/ATOVproject/faderpunk/commit/a78f3c75f78dcc471a3cc7f128cceaf2702b8fab))
* **clkcvrnd:** fix mute control ([e13539a](https://github.com/ATOVproject/faderpunk/commit/e13539aec8f0ba5c63622a5c77921e326b1d458c))
* **clkturing:** fix saving of the registers ([4dd4dfe](https://github.com/ATOVproject/faderpunk/commit/4dd4dfe82fd411236a5357ebf9f31cb43f5b6128))
* **clock:** fix clock transport commands ([4e690ac](https://github.com/ATOVproject/faderpunk/commit/4e690ac818723ab11a309d71b1008a4e32923080))
* **clock:** limit extra reset sources ([7fc8619](https://github.com/ATOVproject/faderpunk/commit/7fc861910648376d5f7963214c1c6f2a33df7bd5))
* **control:** app not sending midi ([1ce6d83](https://github.com/ATOVproject/faderpunk/commit/1ce6d83aeb7a97fff627b11d4ce9d198c5e640a9))
* fix subdivision numbers ([bb99959](https://github.com/ATOVproject/faderpunk/commit/bb99959d27df505b97fcddce64ee4c690c455277))
* **turing:** fix issues with sequences length is 16 ([67aa1b8](https://github.com/ATOVproject/faderpunk/commit/67aa1b8cc416dc5184d5f343b08831c432d43d94))

## [1.1.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v1.0.0...faderpunk-v1.1.0) (2025-09-20)


### Features

* **configurator:** add saved confirmation ([9c12c2f](https://github.com/ATOVproject/faderpunk/commit/9c12c2fc404874721ec5caa75e622834d2cecd3e))
* **configurator:** new app overview, get and set app params ([06bf6c3](https://github.com/ATOVproject/faderpunk/commit/06bf6c338f6abd07688952d88dcebd06dfadb8c6))


### Bug Fixes

* **apps:** fix sticky params race condition ([0d9d817](https://github.com/ATOVproject/faderpunk/commit/0d9d817e85a156683b03c243bce849dca56b6154))
* **configurator:** retain storage and parameters when app is moved ([6ea3cab](https://github.com/ATOVproject/faderpunk/commit/6ea3cab3c1e5ae7a8213c57c10b453972b2b48c0))

## [1.0.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.9.0...faderpunk-v1.0.0) (2025-09-13)


### ⚠ BREAKING CHANGES

* the phoenix has risen from the ashes

### Features

* **ad:** add gate indicator ([afd94f5](https://github.com/ATOVproject/faderpunk/commit/afd94f5caa64856bcab8fedde4552fa4ed4b1aad))
* **ad:** add the ability to deactivate the midi input ([ec9ffb8](https://github.com/ATOVproject/faderpunk/commit/ec9ffb82475ff4a824ae77f737cb8d6d46867bfa))
* **ad:** add use_midi param ([e5f4510](https://github.com/ATOVproject/faderpunk/commit/e5f4510e68671ec771d8e2f8f14e72cdbb5925db))
* **api:** add midi aftertouch and pitch bend API ([43c849c](https://github.com/ATOVproject/faderpunk/commit/43c849c3dc57b650fda0ee38b74971dc10980ebc))
* **app:** add color and icon config to apps ([35d19f9](https://github.com/ATOVproject/faderpunk/commit/35d19f92412597c0cb090c60d2c2ed06b4688342))
* **calibration:** move to fixed point calibration ([574d899](https://github.com/ATOVproject/faderpunk/commit/574d89908ff705ab428a040bd6e8b095978e82ee))
* **clock:** add analog clock out from internal clock ([7c3b619](https://github.com/ATOVproject/faderpunk/commit/7c3b619545862a5e22bd65f07dd9c37c0e3ca7c4))
* **clock:** add really long clock divisions ([1a15f70](https://github.com/ATOVproject/faderpunk/commit/1a15f70c0bf96e1b3351e92d5a31a69c9084b6df))
* **clock:** add reset out aux config option ([d021133](https://github.com/ATOVproject/faderpunk/commit/d02113302ed7f3cd45837acd013ff6b35e96eb3c))
* **clock:** passthrough midi clock usb&lt;-&gt;uart ([d9ad686](https://github.com/ATOVproject/faderpunk/commit/d9ad6869f2fd62134bdb3b0b158ff375007a576a))
* **clock:** refactor clock to allow for improved routing ([1d76deb](https://github.com/ATOVproject/faderpunk/commit/1d76deb00552e08e32c9ab00029916753ebde427))
* **clock:** remove all midi passthrough ([3ff3708](https://github.com/ATOVproject/faderpunk/commit/3ff3708966ef12944f0e7d11047ce62e72416e3b))
* **clock:** send midi clock ticks when using internal clock ([919899d](https://github.com/ATOVproject/faderpunk/commit/919899d2358734d73cced6230604a9a1638c402f))
* **clock:** start internal clock with scene+shift ([c8f9343](https://github.com/ATOVproject/faderpunk/commit/c8f9343131b1bbae418531d0f2b0c8d680dd1316))
* **configurator:** add range param ([f5014a0](https://github.com/ATOVproject/faderpunk/commit/f5014a0ee0a53ffa0102d5e39f9750813ebf2ef6))
* **configurator:** use enum for midi modes in midi2cv and turing ([9342d74](https://github.com/ATOVproject/faderpunk/commit/9342d7499ad7b24873cebf69c005b27b93356ffe))
* **fram:** add crc check ([31247c0](https://github.com/ATOVproject/faderpunk/commit/31247c02d3c89e47f91016c9a4ddbaecccaf516e))
* **i2c:** i2c leader (16n compatibility mode) ([0123546](https://github.com/ATOVproject/faderpunk/commit/012354629fbc6462891a9df604250e9fa34cbea4))
* **midi2cv:** add gate on note mode ([e9c494f](https://github.com/ATOVproject/faderpunk/commit/e9c494f613e90f1f5f60004669ac700e5b51a131))
* select color and icons for all app. Rework app order ([e97a390](https://github.com/ATOVproject/faderpunk/commit/e97a390490ff0f9187f809f8231f308718efab98))
* **seq8:** add octave selection in shift functions ([26a0efa](https://github.com/ATOVproject/faderpunk/commit/26a0efaa5c22d1223380e508162f1f10756fc7d2))
* **seq8:** add range option ([1d0faca](https://github.com/ATOVproject/faderpunk/commit/1d0facacc5694d93efb9fa9fbd922b5af3222264))
* the phoenix has risen from the ashes ([17d9fdf](https://github.com/ATOVproject/faderpunk/commit/17d9fdf5aa92fd44e809fbb961465127000acf22))


### Bug Fixes

* **ad:** only conditionally run midi handler ([85181a1](https://github.com/ATOVproject/faderpunk/commit/85181a16217db1ff7d53c5420a3b46e202c3060f))
* **calibration:** fix input calibration for -5 to 5V range ([e5e93b7](https://github.com/ATOVproject/faderpunk/commit/e5e93b74b1252f5204fd7cd150abf5c1f129d77f))
* **calibration:** fix startup button press calibration ([c2178b4](https://github.com/ATOVproject/faderpunk/commit/c2178b4e89ab57c036195207685e8afe87b58200))
* **clock:** do not send reset when external clock is used ([86c8c77](https://github.com/ATOVproject/faderpunk/commit/86c8c77e6b84227e951d96224b5a9daf5bff4527))
* **clock:** prevent drift and stutter while changing bpm ([da6af19](https://github.com/ATOVproject/faderpunk/commit/da6af19d93e2e8b9ac3bd4814442ac2bfdda9238))
* **clock:** reduce clock out trigger length to 5ms ([7166b69](https://github.com/ATOVproject/faderpunk/commit/7166b6993c064bdea5195e7d19a2c659eb4f9ca3))
* **clock:** send reset event when internal clock is stopped ([6b66b05](https://github.com/ATOVproject/faderpunk/commit/6b66b0585ee3cf738842d9db66331b2b8d8a8724))
* exponential and logarithmic curves were switched ([6a2f311](https://github.com/ATOVproject/faderpunk/commit/6a2f3111712a0cb3a993bf2fae294efe3a6667bf))
* **fram:** add initialization check ([3e43ec3](https://github.com/ATOVproject/faderpunk/commit/3e43ec3bcbb9eeeed6a4c69ebef00645c24dee78))
* **leds:** use exponential fader curve for global led brightness ([7001725](https://github.com/ATOVproject/faderpunk/commit/7001725daccc9589d6064b631448ec09b7c19c1f))
* **notefader:** reset now actually reset to 0 ([b3691b1](https://github.com/ATOVproject/faderpunk/commit/b3691b18ff37936f6fee1654475b3dc0c0854c13))
* **seq8:** rearrange the shift functions ([a8728fb](https://github.com/ATOVproject/faderpunk/commit/a8728fb3c5d43a974d85900b9884247982c5e3d9))

## [0.9.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.8.1...faderpunk-v0.9.0) (2025-09-08)


### Features

* **cv2midinote:** add quantizer on input ([36be3fe](https://github.com/ATOVproject/faderpunk/commit/36be3fe256a0131070be6d91d71dc06ce54e2e19))
* **cv2midinote:** add semitone offset toggle ([aca8f02](https://github.com/ATOVproject/faderpunk/commit/aca8f0281fd38adbb758796ae1b89bb7bdfcc205))
* **follower:** add input gain control ([ee3e1b9](https://github.com/ATOVproject/faderpunk/commit/ee3e1b920473321c83bb377b273b666c92c2c530))
* **offset_att:** increase max gain to 2x ([6bde1c8](https://github.com/ATOVproject/faderpunk/commit/6bde1c87ea5c23a4f9bdc99033a05c38ec4e4c33))
* **quantizer:** add offset toggles ([ef7676b](https://github.com/ATOVproject/faderpunk/commit/ef7676b8dae87c0278a4afd30074c8b7ce9d1dcd))


### Bug Fixes

* 1 bar division  was wrong ([359b549](https://github.com/ATOVproject/faderpunk/commit/359b549108899ca460678b80b68b854d823c624e))
* **control:** reduce slew on mute ([041a460](https://github.com/ATOVproject/faderpunk/commit/041a46055c45a6c08dfc6a9d849a943ab071f55a))
* **cv2midinote & probatrigger:** fix led brightness inconsistency ([330b595](https://github.com/ATOVproject/faderpunk/commit/330b59572a11b42e92d35b8fd062596bae26ef64))
* **cv2midinote:** fix issue crash when changing to certain params ([392f2d4](https://github.com/ATOVproject/faderpunk/commit/392f2d4262da3decc1634063583065ac33789da6))
* **cv2midinote:** remove CC param ([be640a5](https://github.com/ATOVproject/faderpunk/commit/be640a5400a034a5f5d1e76ef99407e97ca32b00))
* **euclid:** bug upon changing the resolution ([f1c73ae](https://github.com/ATOVproject/faderpunk/commit/f1c73ae41be289d8f6d103da6dd7712dcb84890c))
* **euclid:** Fix default values on rotation and randomization ([94cf7cc](https://github.com/ATOVproject/faderpunk/commit/94cf7cceca81fed110ad889130ba8d6093278112))
* **euclid:** fix division setting not being recalled ([baebf9d](https://github.com/ATOVproject/faderpunk/commit/baebf9de0e89004d00f27217087c01f9f22b23a2))
* **euclid:** fix params not being applied ([9300a87](https://github.com/ATOVproject/faderpunk/commit/9300a8727414aa9ff9b857755c05868b6a989d72))
* **follower & slew:** set offset to correct default value ([894e979](https://github.com/ATOVproject/faderpunk/commit/894e9791e68be22b53aa16957384cc0ceb007ed0))
* **notefader:** rework to implement new button API ([f67dbf4](https://github.com/ATOVproject/faderpunk/commit/f67dbf49fcd1098069973f9174c9f979259299eb))
* **probatrigger:** remove fader curve param ([ca57153](https://github.com/ATOVproject/faderpunk/commit/ca5715384b64077f324cda314b6e23a625d39751))
* **turing &  turing+:** fix attenuation curve ([bf5fc19](https://github.com/ATOVproject/faderpunk/commit/bf5fc1951ed0ccc09889a844c806e0d723b078ac))

## [0.8.1](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.8.0...faderpunk-v0.8.1) (2025-09-04)


### Bug Fixes

* prefixed commit ([3f3c80b](https://github.com/ATOVproject/faderpunk/commit/3f3c80b24aac772bafbd3c62fd520be81c2c3421))

## [0.8.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.7.0...faderpunk-v0.8.0) (2025-09-04)


### Features

* **ad:** add trigger on button, add midi trigger, add trigger to gate ([d80449b](https://github.com/ATOVproject/faderpunk/commit/d80449b2fb654aee8c765f31de7d5d799810548b))
* **api:** add button release API ([2ef8cf7](https://github.com/ATOVproject/faderpunk/commit/2ef8cf7cb90f376b3735c3279796ec101ed5c29a))
* **api:** complete rework of app parameter implementation ([8a6f44d](https://github.com/ATOVproject/faderpunk/commit/8a6f44dcefe066d20a1db0e81c96a3fa3caa1832))
* **api:** globals are now sync ([626b25f](https://github.com/ATOVproject/faderpunk/commit/626b25f0472d06ba4b7678620e779a90cb980f35))
* **api:** new color api and improved color consistency ([056761f](https://github.com/ATOVproject/faderpunk/commit/056761ff42a336f8836da01ec7a58c773b6e5598))
* **api:** use new latch in default and lfo ([9abe1cd](https://github.com/ATOVproject/faderpunk/commit/9abe1cdf27c78bd8dfc72cb3b2b946b15d2ea95d))
* **app:** rename default to control ([4aac807](https://github.com/ATOVproject/faderpunk/commit/4aac807921910b1f22b8b758876587ba074c9454))
* **clkcvrnd:** add slew, refine LEDs ([7c59374](https://github.com/ATOVproject/faderpunk/commit/7c59374402b6d347725d28ad9914e3951b6f6b3a))
* **config:** add ability to change global config via faders ([9374b77](https://github.com/ATOVproject/faderpunk/commit/9374b779429b8bb6f242dca0ae5078368ad4ecd5))
* **config:** introduce more global settings, config task loop ([17e48d4](https://github.com/ATOVproject/faderpunk/commit/17e48d4a9f1fcf43130984e9adaa0505c5e2dae6))
* **cv2midi:** add cv2midi app ([ab4864f](https://github.com/ATOVproject/faderpunk/commit/ab4864f3714c9907dc485aaf77893b2ae5cd3d09))
* **cv2midinote:** add cv2midinote app ([125c0a7](https://github.com/ATOVproject/faderpunk/commit/125c0a725a3086f80b060b75e973f12b8bac76d6))
* **cv2midinote:** add octave and semitone shift, add mute, add led feedback ([fc3e7a5](https://github.com/ATOVproject/faderpunk/commit/fc3e7a5e7e86891966f9cb4daeb4739db0671b8e))
* **default:** renamed to "Control", make curve symmetrical around 0 when bipolar ([67b6d8d](https://github.com/ATOVproject/faderpunk/commit/67b6d8d1e8e796d195eea019dffd07d5a11c7187))
* **euclid:** add euclid app ([2eaff71](https://github.com/ATOVproject/faderpunk/commit/2eaff715aacbcf0c1e643768ce9a9cf8348f67e4))
* **latch:** add third latch layer ([c827400](https://github.com/ATOVproject/faderpunk/commit/c82740005ad0829f4fd7eee9ef80a01389dc23cf))
* **leds:** bring back startup animation ([214de9e](https://github.com/ATOVproject/faderpunk/commit/214de9e6332b027b5ea4ce6ad60c2492fbe2fdab))
* **leds:** colorize scene and shift button ([d5cdaaf](https://github.com/ATOVproject/faderpunk/commit/d5cdaafe69ede8025fbc32be40899c507db980cd))
* **leds:** improve led brightness and color apis ([4ff24e2](https://github.com/ATOVproject/faderpunk/commit/4ff24e20b812fcbcaa332297c126f82b072e2848))
* **offset+attenuator:** add Offset+Attenuator app ([8aa8e1c](https://github.com/ATOVproject/faderpunk/commit/8aa8e1c25f8c2700b42f030568031ff9c2012986))
* **quantizer:** add quantizer app ([062cfc4](https://github.com/ATOVproject/faderpunk/commit/062cfc460a007c68c5389cd67792ad5f40427626))
* **quantizer:** rewrite quantizer, make it more predictable ([0c14ef6](https://github.com/ATOVproject/faderpunk/commit/0c14ef6f9d8561f74b9b85f157de4acbeaf19c08))


### Bug Fixes

* **ad:** fix led ([4885e91](https://github.com/ATOVproject/faderpunk/commit/4885e918c8fa48dcf6bf359fbf33dde5b4c2c267))
* **ad:** implement new latching system ([17f66aa](https://github.com/ATOVproject/faderpunk/commit/17f66aaa4b228fbd63ad1302ad11440adf9b8f53))
* **automator:** remove automator from app list ([54f0289](https://github.com/ATOVproject/faderpunk/commit/54f0289d54458245b3c0cf9c6e1eaae4860c179d))
* **clock:** adjust clock config only when it was changed ([9d53f36](https://github.com/ATOVproject/faderpunk/commit/9d53f36edf53b4cd33089df2a9dac831d012eab1))
* **cv2midi:** implement new latching system ([2f6173f](https://github.com/ATOVproject/faderpunk/commit/2f6173f1ea2d24b0ba94d885ed9c4e8b6b2325ef))
* **cv2midinote:** add description ([9154f73](https://github.com/ATOVproject/faderpunk/commit/9154f73c88e4968c94bd3ece6944fa4635c78d98))
* do not panic in app macro functions ([2bb337d](https://github.com/ATOVproject/faderpunk/commit/2bb337d75d50e430a3e2befac7eae74377213767))
* **euclid:** change description ([549611e](https://github.com/ATOVproject/faderpunk/commit/549611e2aa7ba9f721e04164a477ca0f5d0e58fa))
* **euclid:** fix midi notes ([2fc0102](https://github.com/ATOVproject/faderpunk/commit/2fc0102431213d73e5ee70d9de68a2aceca7521f))
* **follower, slew:** update to new new latching ([00ac12d](https://github.com/ATOVproject/faderpunk/commit/00ac12dc3f29076fc3fb34d2f1a30fd4b6f89b26))
* **layout:** allow for holes in layout ([20ff5bc](https://github.com/ATOVproject/faderpunk/commit/20ff5bc92461369f145b13716ba3fe45f93e3e4c))
* **leds:** adjust minimum led brightness ([34261f7](https://github.com/ATOVproject/faderpunk/commit/34261f7a2d5e887124b945ca245d6d3afae5830d))
* **max:** scale fader readings across the dead zone ([7279540](https://github.com/ATOVproject/faderpunk/commit/7279540da4624894699c583572a0e4ff7a4bef7b))
* **midi2cv:** update to new latching system ([38db61c](https://github.com/ATOVproject/faderpunk/commit/38db61c4ed10fe0420764ea8327c59a089280094))
* **notefader:** implement new latching system ([596ad56](https://github.com/ATOVproject/faderpunk/commit/596ad56c112d276b4b1b94a8a286cc9866083a71))
* **offset_att:** modify name and description ([892b590](https://github.com/ATOVproject/faderpunk/commit/892b590d8407a4ad1fde721cf87f2c009d2d997f))
* **offset_att:** update to new latching system ([4fedf99](https://github.com/ATOVproject/faderpunk/commit/4fedf998d6b370872647b2161877a873738ad38d))
* **probatrigger:** upgrade to new latching system ([fe20724](https://github.com/ATOVproject/faderpunk/commit/fe20724f16d1a7b3afe4a3b44ed60ebb2f17f8a3))
* **quantizer:** add description ([8738017](https://github.com/ATOVproject/faderpunk/commit/8738017d07b829572a3fea6b9e00e64995e3a1c0))
* **sequencer:** fix consistency issue between midi output and V/oct ([b18ebd2](https://github.com/ATOVproject/faderpunk/commit/b18ebd2da509a05a423c2aa982e8b428688d015e))
* **turing+:** implement new latching system, refine code ([e06c559](https://github.com/ATOVproject/faderpunk/commit/e06c5599b68f81a9def2a8d7f85829cafbfe799e))
* **turing:** implement new latching system ([d706882](https://github.com/ATOVproject/faderpunk/commit/d706882f61344269ad640ec81083cac28ffdedf3))
* validate layout after loading from fram ([848e2aa](https://github.com/ATOVproject/faderpunk/commit/848e2aa79130d134737f66309c023211e041f861))

## [0.7.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.6.1...faderpunk-v0.7.0) (2025-08-23)


### Features

* add color parameters to most apps ([79c716f](https://github.com/ATOVproject/faderpunk/commit/79c716fe5df24caf1c3ff8e846985e668adb212a))
* **default:** add mute on release proto function ([ef01239](https://github.com/ATOVproject/faderpunk/commit/ef012394756ecf2ff9ef85bcf26d805616107e07))
* **lfo:** reset messages resets the LFO ([afe84e9](https://github.com/ATOVproject/faderpunk/commit/afe84e92108f2b681072c75b2701dae23d592fac))
* **midi2cv:** add midi2cv prototype app ([c005ac1](https://github.com/ATOVproject/faderpunk/commit/c005ac1c0d0d7b4827dcde9ff5f7a7057a3b015f))
* **midi2cv:** add mute and led feedback ([fc0b17e](https://github.com/ATOVproject/faderpunk/commit/fc0b17ec25e20e0ad051326ff0d1f0b643c9b827))


### Bug Fixes

* **calibration:** fixes for semi-automatic calibration ([932321b](https://github.com/ATOVproject/faderpunk/commit/932321bad07da39aaa704c64fcc023f7399ea835))
* **midi2cv:** adjust color order for consistency ([fdfbe3b](https://github.com/ATOVproject/faderpunk/commit/fdfbe3b24914f5632c5890915172b9797d6b5379))
* **midi2cv:** fix small led bug ([4cac8c5](https://github.com/ATOVproject/faderpunk/commit/4cac8c54ff873bcde61940b23b258dd291ab9120))
* **midi:** improve midi subscriber instantiation ([46e20f2](https://github.com/ATOVproject/faderpunk/commit/46e20f2763ca7582c6b938ee800d47efb7e26492))

## [0.6.1](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.6.0...faderpunk-v0.6.1) (2025-08-20)


### Bug Fixes

* make a change to force rebuild ([5ba1572](https://github.com/ATOVproject/faderpunk/commit/5ba1572459a6b40e63b009938e9fc017a456d2c9))

## [0.6.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.5.0...faderpunk-v0.6.0) (2025-08-20)


### Features

* add -5V to 5V range to manual calibration ([f6cee85](https://github.com/ATOVproject/faderpunk/commit/f6cee85878316bb552e7ba28f405bb2b6b556fcb))
* **calibration:** add first version of automatic calibration ([2679d6b](https://github.com/ATOVproject/faderpunk/commit/2679d6b955d5b2e50e9ac3028050ecac5450f90a))
* **calibration:** move manual calibration to i2c startup ([83a0c03](https://github.com/ATOVproject/faderpunk/commit/83a0c03e97c0fba81c4545b0734cb066556f4e1e))
* **config:** separate layout from global config ([54d8690](https://github.com/ATOVproject/faderpunk/commit/54d869014c2299812519a4b47cc0b8a9a069a09f))
* **i2c:** prepare for i2c leader/follower/calibration modes ([2269d84](https://github.com/ATOVproject/faderpunk/commit/2269d841e35dd07a73397bd2a234977b944e2fc7))
* improve semi-automatic calibration ([71d1f4e](https://github.com/ATOVproject/faderpunk/commit/71d1f4e46590adc99d62477ad577860ae5554331))
* move Range to libfp ([a349b55](https://github.com/ATOVproject/faderpunk/commit/a349b55924c98180409e89da698f7b392b2b9323))


### Bug Fixes

* actually respond to i2c read requests ([0295d37](https://github.com/ATOVproject/faderpunk/commit/0295d37b3a53708652b073a89a9f122e641a24d1))
* **midi:** improve midi subscriber instantiation ([a43277a](https://github.com/ATOVproject/faderpunk/commit/a43277ace4ea4a64ac6c68fa8c85f64acc9d2fe6))
* **midi:** increase uart rx buffer size ([49194df](https://github.com/ATOVproject/faderpunk/commit/49194df521d6739d7d285faa42c795f544f45b7f))
* **midi:** make wait_for_message method public ([5618706](https://github.com/ATOVproject/faderpunk/commit/56187062690ac313c656db5d703c12da4c1ca451))
* **midi:** properly handle larger usb midi packets ([94c757f](https://github.com/ATOVproject/faderpunk/commit/94c757f7d7e35f874c1a849c5adeac501d50c2e5))

## [0.5.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.4.2...faderpunk-v0.5.0) (2025-08-14)


### Features

* **notefader:** add notefader app ([5dc1dd2](https://github.com/ATOVproject/faderpunk/commit/5dc1dd221d50bc0b47f6c6d0cd891c6eb4764314))


### Bug Fixes

* **apps:** load params at app startup ([59a74df](https://github.com/ATOVproject/faderpunk/commit/59a74dfa5c34653183357546a3d5b76822b564c3))
* **automator:** fix bipolar recall, update slewing to new method ([43e498e](https://github.com/ATOVproject/faderpunk/commit/43e498e3ba64dfa4729f259139f50f976a562b02))
* **default:** fix curve, slew and bipolar recall ([968d4df](https://github.com/ATOVproject/faderpunk/commit/968d4dfca3812f1f3f4084d8a9448b81b70a7603))
* **default:** use the common red color value ([a645ea6](https://github.com/ATOVproject/faderpunk/commit/a645ea62c22c02c779f60be88faffc07667e5e6d))
* **follower & slew:** small changes to make it easier to change LED colors ([961a03e](https://github.com/ATOVproject/faderpunk/commit/961a03e8abf8328f365c619655e9a9e2542d2e64))
* **follower and slew:** change offset default value to 0 ([ee68d5d](https://github.com/ATOVproject/faderpunk/commit/ee68d5d59f416279c6e5d0db8cbec112dbf30556))
* **fram:** better read buffer error handling ([2aa1af6](https://github.com/ATOVproject/faderpunk/commit/2aa1af64eb3d68f793c384e6d827f1c056dd18ab))
* **lfo:** change clock switch procedure ([4cbb93b](https://github.com/ATOVproject/faderpunk/commit/4cbb93b1b1e3c8c329c220ce341ce72e17f7e6ce))
* **lfo:** fix crashes on certain recalled values ([97a0392](https://github.com/ATOVproject/faderpunk/commit/97a0392bdc83a14d047c99103071e3178e0d7afd))
* **notefader:** small led fix, fix note trigger when changing mode ([8ac632d](https://github.com/ATOVproject/faderpunk/commit/8ac632dfc433b93520b347ae46df4ac131bc3f3c))
* **quantizer:** reactivate quantizer ([2a7fada](https://github.com/ATOVproject/faderpunk/commit/2a7fadae6392bf1bee65d226b9580ab11e784142))
* **sequencer:** remove running light when stopped ([094cc66](https://github.com/ATOVproject/faderpunk/commit/094cc66a844d21dffccc4d890de310af70d6a5f7))

## [0.4.2](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.4.1...faderpunk-v0.4.2) (2025-08-12)


### Bug Fixes

* **buttons:** improve debounce and add button state sync ([f12cc04](https://github.com/ATOVproject/faderpunk/commit/f12cc04beaef36fe155c222e1de7892d62e7de7e))

## [0.4.1](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.4.0...faderpunk-v0.4.1) (2025-08-08)


### Bug Fixes

* **api:** rename Sawinv to SawInv ([9b18e3c](https://github.com/ATOVproject/faderpunk/commit/9b18e3c5f6fd4134e83119d209608b06f5a863e0))
* **lfo:** fix crash on certain fader positions ([0bf3aaa](https://github.com/ATOVproject/faderpunk/commit/0bf3aaa289e22940a2ec2f92549a39d97d78bc57))

## [0.4.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.3.0...faderpunk-v0.4.0) (2025-08-08)


### Features

* **api:** add MidiOutput, MidiInput, MidiDuplex APIs ([710358e](https://github.com/ATOVproject/faderpunk/commit/710358e9d0f5276816c2b1414f92add927ec81bd))
* **automator:** add attenuation ([6374638](https://github.com/ATOVproject/faderpunk/commit/637463831752bbb34370cb8c6647a983daea70e4))
* **automator:** add bipolar and curve param ([b35180c](https://github.com/ATOVproject/faderpunk/commit/b35180c0396d89b0d2b36ada003baa375b683994))
* **constants:** introduced some standard LED colors and intensities ([2e2baa3](https://github.com/ATOVproject/faderpunk/commit/2e2baa3f92c27a83cb1f276791162070a4610914))
* **default:** add attenuation ([6ce5df1](https://github.com/ATOVproject/faderpunk/commit/6ce5df1d578dd2a8ae9af856a40e6370c6a6fe47))
* **default:** add bipolar param ([f68b6fa](https://github.com/ATOVproject/faderpunk/commit/f68b6fac6cf3ae495190afa22d05ae0cebee7180))
* **default:** add clickless mute, remove stepping ([785eb00](https://github.com/ATOVproject/faderpunk/commit/785eb000e654a4b7ca0edbed75219c8c5a01990d))
* **default:** add MIDI CC selection ([c3b36ed](https://github.com/ATOVproject/faderpunk/commit/c3b36edc765da824c4b51462d37869b977785cca))
* **deps:** downgrade embassy-executor and embassy-rp for now ([ca286f4](https://github.com/ATOVproject/faderpunk/commit/ca286f425fdb17974840c39dd3020a428919acf5))
* **lfo:** add clocked mode ([94b104c](https://github.com/ATOVproject/faderpunk/commit/94b104c8c3fdc07449cb24a19c1f1ae81766f550))
* **lfo:** add inverted saw waveform ([b32c7bc](https://github.com/ATOVproject/faderpunk/commit/b32c7bc923010eb65ae5a7ba5b0072cf674aebc5))
* **probatrigger:** add fader curve param for testing ([a0d4506](https://github.com/ATOVproject/faderpunk/commit/a0d450665318b2fa5d5e69c61a0a889d15caeae7))
* **quantizer:** add quantizer to apps ([3fc4ef0](https://github.com/ATOVproject/faderpunk/commit/3fc4ef06737292aafa9a027cf1c0a50a04f5e5aa))
* **quantizer:** add quantizer utility ([201a47b](https://github.com/ATOVproject/faderpunk/commit/201a47b3dc9beeaefd57f0f84931c4565e129385))
* **rgbtest:** add rgb test app ([4599471](https://github.com/ATOVproject/faderpunk/commit/45994715b0fb4d723a5ce2a105d19bc0e48adeaf))
* **sequencer:** add legato ([e60b3ea](https://github.com/ATOVproject/faderpunk/commit/e60b3ea0cc56dc7d0d5663d92db181f37b6a761f))
* **utils:** add clickless function as public ([819042b](https://github.com/ATOVproject/faderpunk/commit/819042b4f788d795168c841473c8dd4ca56fc96b))


### Bug Fixes

* **automator:** add interpolation to remove stepping ([03e8bd3](https://github.com/ATOVproject/faderpunk/commit/03e8bd3731a849b3f882ac080c2239e73e3ccda4))
* **automator:** kill previous buffer before recording a new one ([f54e4fd](https://github.com/ATOVproject/faderpunk/commit/f54e4fd3f80331f12e2c3e11ed8eb03d9c0f62cb))
* housekeeping ([4346be3](https://github.com/ATOVproject/faderpunk/commit/4346be3a248d3dbc289f13d86155063e102dd854))
* **quantizer:** disable quantizer for now ([0ceb4e0](https://github.com/ATOVproject/faderpunk/commit/0ceb4e0fc604dba62a4be59dcd8a82bb64188de0))
* **sequencer:** CH4 not recalling gate and resolution ([eedc9ff](https://github.com/ATOVproject/faderpunk/commit/eedc9ffc1676988dc774495bc0e4f81ba7139637))
* **turing & turing+:** bigger dead zones on probability ([be19ccd](https://github.com/ATOVproject/faderpunk/commit/be19ccdc9b0069ad04308b5048a8b7be0c209a9f))
* **turing & turing+:** make bit flip more direct ([460e28d](https://github.com/ATOVproject/faderpunk/commit/460e28d443fe553f837f7df6b06ee564f77f13c8))

## [0.3.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.2.0...faderpunk-v0.3.0) (2025-07-27)


### Features

* **app:** return is_shift_pressed from any button press ([2480dd1](https://github.com/ATOVproject/faderpunk/commit/2480dd1ea2d85ffe87ea43748be72870ca220775))
* **calibration:** add undo for the last calibration step ([463cfd7](https://github.com/ATOVproject/faderpunk/commit/463cfd72f71fa1444b549976448bdfd0b9b6c5fd))
* **die:** improve die roll function signature ([a337680](https://github.com/ATOVproject/faderpunk/commit/a33768009cdb4d9f999c0495b9c15bc755742a5c))
* update all dependencies ([6d941bb](https://github.com/ATOVproject/faderpunk/commit/6d941bb183164367aa34550fb642c4efc6522556))
* **usb:** fix webusb windows compatibility ([fb01f98](https://github.com/ATOVproject/faderpunk/commit/fb01f981c64beb133b50f6072ae73fe30f113e3b))
* **usb:** remove usb logging for now ([7ebe4ae](https://github.com/ATOVproject/faderpunk/commit/7ebe4aedf8f0f50138c2f1c44358c1270b5bcf66))
* **usb:** use auto-generated device version ([a0e79f5](https://github.com/ATOVproject/faderpunk/commit/a0e79f555537dc3c823c108ae441e3949f5b4cec))


### Bug Fixes

* **calibration:** add a little more leeway for the calibration range ([7a8f3f2](https://github.com/ATOVproject/faderpunk/commit/7a8f3f2b232e07071a7abefe8433d7553acec755))
* **fram:** make READ_BUFFERS pointer cast a bit more ideomatic ([7ab304e](https://github.com/ATOVproject/faderpunk/commit/7ab304e13c230423620c68b9a7a2bb360b2cac42))

## [0.2.0](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.1.1...faderpunk-v0.2.0) (2025-07-19)


### Features

* **ad:** add latest ad version ([08029de](https://github.com/ATOVproject/faderpunk/commit/08029de8927f087996ce5a1e1eb0737b59350069))
* **app:** improve Curve api ([493f4b7](https://github.com/ATOVproject/faderpunk/commit/493f4b7aaaee73a0f5a4a2f558e11a919fe253c4))
* **apps:** improve fader api ([e54c554](https://github.com/ATOVproject/faderpunk/commit/e54c5544da87b0497a59db03bde5d2363272e81e))
* **apps:** remove test apps ([a9b9373](https://github.com/ATOVproject/faderpunk/commit/a9b9373ebd1b580123bc70955044394cc6096438))
* **automator:** add latest automator version ([99c8a6b](https://github.com/ATOVproject/faderpunk/commit/99c8a6be7f906cfc426d696b81d6c96e51652244))
* **calibration:** add output calibration over i2c ([d8b25a1](https://github.com/ATOVproject/faderpunk/commit/d8b25a1d09294f39396d8960110223bdc71d24a6))
* **calibration:** i2c proto ping pong ([2c1d190](https://github.com/ATOVproject/faderpunk/commit/2c1d190ccb7a76c5bc61cc96cae9749a6277a833))
* **calibrator:** add manual calibration app ([98db6fc](https://github.com/ATOVproject/faderpunk/commit/98db6fcda9af6157009d4bcc2f3eaecbfb781e56))
* **clkturing:** add latest version of clkturing app ([b3972fe](https://github.com/ATOVproject/faderpunk/commit/b3972fe40eb2d464d243dcca9231184fc6b5f463))
* **follower:** add latest version of follower app ([853496f](https://github.com/ATOVproject/faderpunk/commit/853496f8cf899c744d741bef35743823faabb391))
* **leds:** add ability to use effects in apps ([a0298a8](https://github.com/ATOVproject/faderpunk/commit/a0298a8929d1151b68ba762c855df3c4d4a2ac8c))
* **lfo:** add latest lfo version ([b6a5a1e](https://github.com/ATOVproject/faderpunk/commit/b6a5a1e2073ea57d645d03c505b76f12341dfcf1))
* **max:** load calibration data, use it in max task ([12018b6](https://github.com/ATOVproject/faderpunk/commit/12018b64f5f810e5b8ceff44954f93dec5c30895))
* **max:** set fader refresh rate to 1ms ([dc09296](https://github.com/ATOVproject/faderpunk/commit/dc09296c366af785a32c3a8566038ff179eb5ed1))
* merge config crate into libfp ([d69da45](https://github.com/ATOVproject/faderpunk/commit/d69da45ed8b4a60fd020ce567328b348cf475319))
* **probatrigger:** add latest version of probatrigger app ([a02d8ba](https://github.com/ATOVproject/faderpunk/commit/a02d8ba83d92a372209596ef736c046bf4f33d1a))
* **seq8:** add latest seq8 version ([f6518a2](https://github.com/ATOVproject/faderpunk/commit/f6518a25b851f14cd243ecd9baed1f17265e5c71))
* **slew:** add latest version of slew app ([4bb9cd7](https://github.com/ATOVproject/faderpunk/commit/4bb9cd7282a71060f10fbb9017527ab69c95e972))
* **turing:** add latest version of turing app ([0bae27b](https://github.com/ATOVproject/faderpunk/commit/0bae27b1c4f0a6d6dc54687b54f3d97638ee059b))


### Bug Fixes

* **ad:** fix bug where curve was not properly applied ([0727b48](https://github.com/ATOVproject/faderpunk/commit/0727b480bd0372bfbcbacbf923857ec21c355a08))
* **ad:** fix led and latching bug ([8d83615](https://github.com/ATOVproject/faderpunk/commit/8d8361598964e5927e9086983434fc94c7ae396f))
* **automator:** full rework ([e6956bb](https://github.com/ATOVproject/faderpunk/commit/e6956bb6bae6ba4fb84d59aa7be89736363537e5))
* **automator:** remove saving ([169a4f9](https://github.com/ATOVproject/faderpunk/commit/169a4f9555e7790853e3ad9131c32d0e7e4d09e6))
* **calibration:** correct input value properly ([f62b92f](https://github.com/ATOVproject/faderpunk/commit/f62b92fa9a1b5ce44c9593212122055e7582d13b))
* **clkcvrnd:** add attenuation fix leds ([6202e8f](https://github.com/ATOVproject/faderpunk/commit/6202e8ff60da6fc0fdc13bf24125077d97f84e67))
* **clkcvrnd:** fix  a little LED bug ([2787163](https://github.com/ATOVproject/faderpunk/commit/2787163f41198fb19ea44266691ed45f59f89547))
* **clkcvrnd:** full rework ([cacfa73](https://github.com/ATOVproject/faderpunk/commit/cacfa737a380bcd60da9745f6c43662189d139bd))
* **clkturing:** full rework ([1ed6531](https://github.com/ATOVproject/faderpunk/commit/1ed6531656f891f0d4bb5b3ee723509397737884))
* **clkturing:** rename ([176a0f0](https://github.com/ATOVproject/faderpunk/commit/176a0f00376810fb6b498721707eb3aa27c3e829))
* **default:** add led feedback ([7c785c0](https://github.com/ATOVproject/faderpunk/commit/7c785c07ec38fe43fe5ca36b4ff51f2e886a72c6))
* fix recurring mistake when using ticks ([f084c50](https://github.com/ATOVproject/faderpunk/commit/f084c501d1c5d324e845c612dcb71d51fc043cae))
* **follower:** refine ([009f80b](https://github.com/ATOVproject/faderpunk/commit/009f80b681b45133fd6df6e6b39b0f8b99fb9f54))
* **lfo:** some led changes ([fc62eac](https://github.com/ATOVproject/faderpunk/commit/fc62eacd229e42e7665d30a94503777c10164e63))
* **lfo:** use utils functions ([30c51bd](https://github.com/ATOVproject/faderpunk/commit/30c51bd637ac300e617c2d18395e394641164b87))
* **probatrigger:** change name and description ([ab0a827](https://github.com/ATOVproject/faderpunk/commit/ab0a827480f6bed74c6cf98867ae07322c3c8b6a))
* **probatrigger:** full rework ([20669b1](https://github.com/ATOVproject/faderpunk/commit/20669b14ed7c2056a36fcbbe5cda761b3a40b8db))
* **seq8:** make it recall resolution and gate length on boot ([0e6b6c1](https://github.com/ATOVproject/faderpunk/commit/0e6b6c1b84c977889cba0468fcb08fc180d0cf58))
* **slew:** full rework ([5686a04](https://github.com/ATOVproject/faderpunk/commit/5686a040b7367518e441d88d9a9c48393f8567d5))
* **slew:** refine ([f7e0fc1](https://github.com/ATOVproject/faderpunk/commit/f7e0fc11c33164ee70722ad88ece3e6f33344b00))
* **turing:** added param, saving and led feedback ([5e3fe40](https://github.com/ATOVproject/faderpunk/commit/5e3fe407a9b091d337a7d73e91a0c2a0db4cfe55))

## [0.1.1](https://github.com/ATOVproject/faderpunk/compare/faderpunk-v0.1.0...faderpunk-v0.1.1) (2025-07-08)


### Bug Fixes

* **leds:** add separate channel for led overlay effects ([88c5b8c](https://github.com/ATOVproject/faderpunk/commit/88c5b8cb9c473932d5836ea552df18d7d0a09fa9))

## 0.1.0 (2025-07-08)


### Features

* (very) simple button debounce ([a0cacbe](https://github.com/ATOVproject/faderpunk/commit/a0cacbe5c1c97ce107116f5c28ea2912cf9712ba))
* add and set params for apps ([55317b9](https://github.com/ATOVproject/faderpunk/commit/55317b90ed6b0cb6c315737603fbe55b6cc37220))
* add app cleanup method ([07b4963](https://github.com/ATOVproject/faderpunk/commit/07b496396e540eee2257bdaa25a96cca5777d660))
* add AppParams macro and storage ([1a6618b](https://github.com/ATOVproject/faderpunk/commit/1a6618b20734042830b5761397e4d97f6e34deb9))
* add button debounce, long press ([a81902b](https://github.com/ATOVproject/faderpunk/commit/a81902b927bd97820e62d32e4bf0acfde3e6728a))
* add gen-bindings, restructure project ([0628406](https://github.com/ATOVproject/faderpunk/commit/06284069ff090d442f921713c12f794181328aab))
* add led overlay effects and flash effect ([032b577](https://github.com/ATOVproject/faderpunk/commit/032b5773eed626cdce704042efac3c78a4756ec6))
* add midi input message forwarding ([410878b](https://github.com/ATOVproject/faderpunk/commit/410878b8dfb76f67e2fb5af41854ab9056596450))
* add modify method to Global ([2bb0799](https://github.com/ATOVproject/faderpunk/commit/2bb079960bef98ec320ecc58b38576dba5a41a0c))
* add mute led to default app ([3b308ac](https://github.com/ATOVproject/faderpunk/commit/3b308acc9ea4534cd07ec3b26b9c811c035ee251))
* add param and cleanup loops to all apps ([4f16579](https://github.com/ATOVproject/faderpunk/commit/4f165790502ad9f9fda16be7056efe36cb6fd3a6))
* add param load and save for apps ([d47f7dd](https://github.com/ATOVproject/faderpunk/commit/d47f7dda3c6707f23b49f84fbad1d241b5b20cf6))
* add postcard encoded app config list ([e8889cd](https://github.com/ATOVproject/faderpunk/commit/e8889cdf681f7d432e7dd9eb648a76410ab0928d))
* add sequential storage using eeprom ([58f8e50](https://github.com/ATOVproject/faderpunk/commit/58f8e50db05bb69c06dd0c9b50fc24eac50e2187))
* add temporary scene save and recall effects ([406b6a7](https://github.com/ATOVproject/faderpunk/commit/406b6a754861b2bd5bf94b98a9a54a924635faeb))
* add usb windows compatibility ([ed90f86](https://github.com/ATOVproject/faderpunk/commit/ed90f86571bd495dc86b49727f521adf8b8079e1))
* add wait_for_any_long_press function to app ([2b0a013](https://github.com/ATOVproject/faderpunk/commit/2b0a013383510902ee595b31640d830e3e12bc77))
* **app:** allow storing arrays ([2a36f09](https://github.com/ATOVproject/faderpunk/commit/2a36f09071ae22baddcfc2cba7ab666875850e1c))
* **config:** always require params() in config macro ([4759fdf](https://github.com/ATOVproject/faderpunk/commit/4759fdf3d0a2c38b07c3cdd335d27e119d216cb9))
* **config:** move storage globals into app_config ([debf92f](https://github.com/ATOVproject/faderpunk/commit/debf92f3e3466c35d1636cd164007730c7838765))
* **config:** retrieve app state from configurator ([1b9d105](https://github.com/ATOVproject/faderpunk/commit/1b9d10513b0fccf923d367e88b76872f50467938))
* **config:** set a param from configurator ([de47407](https://github.com/ATOVproject/faderpunk/commit/de47407a0ea913dcefe5767019b7a988b2661d00))
* **configurator:** implement layout setting ([17cb7b3](https://github.com/ATOVproject/faderpunk/commit/17cb7b338c8764302ada0ed4b54e7c74fbd5e2db))
* **configurator:** set custom layouts ([8902af6](https://github.com/ATOVproject/faderpunk/commit/8902af6f3f433e0046f3a445e4d1d1ed91483a10))
* decode large configuration messages ([e415f13](https://github.com/ATOVproject/faderpunk/commit/e415f13e740f2ac7efae0b40bdc85e65598376de))
* **eeprom:** read-before-write ([b1bf8cf](https://github.com/ATOVproject/faderpunk/commit/b1bf8cfb8148cfb8b3e2345d4423e093da48f301))
* improve lfo ([1a718f4](https://github.com/ATOVproject/faderpunk/commit/1a718f4ee6fbd2a1e0155e3d9c63998864b5bf45))
* **leds:** add glitchy startup animation ([7ac48a8](https://github.com/ATOVproject/faderpunk/commit/7ac48a82494bd8cdb07f159746b6ea6ccdcf536b))
* **leds:** set shift and scene button to white ([cac86fb](https://github.com/ATOVproject/faderpunk/commit/cac86fbece15da63e412682ff67489f547e5e1b3))
* **leds:** use Signals instead of Channel ([427206f](https://github.com/ATOVproject/faderpunk/commit/427206ff4dbc6011ca83aec2b1536211fac8b59c))
* make max and midi channels CriticalSectionRawMutex Channels ([e0617e5](https://github.com/ATOVproject/faderpunk/commit/e0617e556b9a887034b695d6cd118cb8672d4d64))
* make midi channel configurable in default app ([cb528ff](https://github.com/ATOVproject/faderpunk/commit/cb528ff3d77196376e8ad0798cc42e42235d6f25))
* **midi:** add MidiIn and MidiUSB clock sources ([29d4114](https://github.com/ATOVproject/faderpunk/commit/29d41147f06e9ed8b0e919815000328bb93985c8))
* **midi:** send custom cc value ([fed8bfa](https://github.com/ATOVproject/faderpunk/commit/fed8bfabb860348e6672328a173c237a54ec2e4a))
* move BrightnessExt to libfp ([0972e2d](https://github.com/ATOVproject/faderpunk/commit/0972e2d192cc615ebb831a273bf71dedaa7c2af0))
* move param handler into param store ([27aee71](https://github.com/ATOVproject/faderpunk/commit/27aee71d40f784e74e65201195e7d071e3d9fca0))
* ParamStore -&gt; Store, impl ser and des for Store ([fdb3e68](https://github.com/ATOVproject/faderpunk/commit/fdb3e68b45f6bbc7ad18aa3b45d5ef7fa1a21334))
* re-spawn apps on param change ([1a6fe4e](https://github.com/ATOVproject/faderpunk/commit/1a6fe4e1c46ede8136dfd9c4d27c8291ebecf696))
* redesign app parts, restructure waiters ([eac9486](https://github.com/ATOVproject/faderpunk/commit/eac9486752420a92150752d413ca6e8fba07e693))
* refactor leds a bit, add chan clamping ([264fb3c](https://github.com/ATOVproject/faderpunk/commit/264fb3c4b81767201acdcb4fe6a743d37c19785f))
* refactor leds to allow for effects ([1279610](https://github.com/ATOVproject/faderpunk/commit/1279610023795ba4fc9f6173031e645fbc961b3d))
* refactor midi into struct ([4706af2](https://github.com/ATOVproject/faderpunk/commit/4706af27bf07007a53701269fb9e9fb06d48053e))
* restructure Arr and AppStorage ([78accef](https://github.com/ATOVproject/faderpunk/commit/78accef69e5533398fc05729f6722fc403e0a922))
* **scene:** add simple scene implementation for StorageSlots ([89a7725](https://github.com/ATOVproject/faderpunk/commit/89a77254f70a5fba0d73152729b50db35969dd20))
* **scene:** integrate scenes with scene button ([f4df680](https://github.com/ATOVproject/faderpunk/commit/f4df680fb91271b7ed05d95e985eb928169e610b))
* simplify cross core message routing ([7030d14](https://github.com/ATOVproject/faderpunk/commit/7030d14cc1027c85a48fc73501f91bbe267496bb))
* **storage:** add wait_for_scene_change method ([83838ae](https://github.com/ATOVproject/faderpunk/commit/83838ae9447b6d29cf07942222b9445ccd82dc8a))
* **storage:** allow long arrays for storage slots ([ef0e8aa](https://github.com/ATOVproject/faderpunk/commit/ef0e8aa0b663ac4a76e76ee8c5d024b5eceb494b))
* **storage:** pre-load everything from eeprom ([d3371b9](https://github.com/ATOVproject/faderpunk/commit/d3371b9965fbd38080c43a84aba85f722709e4cc))
* StorageSlot is now dependent on Store ([6e5e122](https://github.com/ATOVproject/faderpunk/commit/6e5e122e45db456b26cdc1d09bdbe142f8ee684c))
* store and recall current values using rpc ([0b29b4c](https://github.com/ATOVproject/faderpunk/commit/0b29b4c61e9367d50d461f0a51e2a3e6e7b478df))
* store GlobalConfig in FRAM ([99a135a](https://github.com/ATOVproject/faderpunk/commit/99a135a1187d6e8f80ab4ffb8dac325ba7bcbd2e))
* use batch messages for app listing ([da76ce1](https://github.com/ATOVproject/faderpunk/commit/da76ce1f72f577b91a74a1f3b4c101f88b33cfa9))
* use ClockEvent instead of bool for clock Watch ([ccbfbac](https://github.com/ATOVproject/faderpunk/commit/ccbfbacbecac8b22b3e4fa2a0a487064ba9c3a79))
* use PubSubChannel for clock ([8052ffb](https://github.com/ATOVproject/faderpunk/commit/8052ffb6e363b35bc251105d0c1ebfeed2e07a1c))
* use static buffer for fram reads ([e2acd5b](https://github.com/ATOVproject/faderpunk/commit/e2acd5b3791b00e3a8c278ef8688a7410074357f))
* use StorageSlots for app storage values ([fbe85cb](https://github.com/ATOVproject/faderpunk/commit/fbe85cb87b677bf3b012f118981ca09dcbb4aa8c))
* vastly improve Storage API ([aafd4a1](https://github.com/ATOVproject/faderpunk/commit/aafd4a1c0d0fd975ef4e12b44ee89c0869f5630f))


### Bug Fixes

* alter macro to account for apps without params ([81ba108](https://github.com/ATOVproject/faderpunk/commit/81ba1082c07043371e54e7f4ce84abf8c6f2d20a))
* **buttons:** improve scene save debounce ([af7bfa4](https://github.com/ATOVproject/faderpunk/commit/af7bfa46cac98ddf5869f6ea86490dc8c90d725b))
* **buttons:** scene load/save debounce ([4aacba9](https://github.com/ATOVproject/faderpunk/commit/4aacba9c1f0d5f0308342f5354f6ef8830f7d8e9))
* clock fixes and clock debug app ([2f39258](https://github.com/ATOVproject/faderpunk/commit/2f392588048dae6c361383c2fe4aac4ee508c464))
* **clock:** improve clock reset behavior ([7d18fa6](https://github.com/ATOVproject/faderpunk/commit/7d18fa62521d41dd265436b7aaa50ffb3c0ba81f))
* **configurator:** fix param fetch ([df69657](https://github.com/ATOVproject/faderpunk/commit/df69657cda82a3d63865c5efca7e4fa504990626))
* drop guard for storage before saving ([1b17cee](https://github.com/ATOVproject/faderpunk/commit/1b17cee9621e0b3d8e67819b74e029a2c83d6331))
* **eeprom:** raise storage bytes limit ([dd72301](https://github.com/ATOVproject/faderpunk/commit/dd72301e66ebdd6c6f8d1896088d02bf514a3d6f))
* **leds:** only update LEDs at actual refresh rate ([19a490b](https://github.com/ATOVproject/faderpunk/commit/19a490b7f883ab276a363c348480a1e4ecbc3693))
* **leds:** run led tasks in parallel ([e3bf449](https://github.com/ATOVproject/faderpunk/commit/e3bf4498d68653403f01915d4a38ac6edf85c2bb))
* loading of Globalconfig ([f9f4249](https://github.com/ATOVproject/faderpunk/commit/f9f42492677d7a9f16462ec23244345f1f991e17))
* midi uart message drops ([7a8c62c](https://github.com/ATOVproject/faderpunk/commit/7a8c62cf65409be6e9add6beacec7d42bfa9ff28))
* **midi:** proper midi 1 implementation using midly ([ea38aca](https://github.com/ATOVproject/faderpunk/commit/ea38aca53bb42330f03e86fbb0a78933aeedeb91))
* **midi:** quick fix for midi tx over uart. remove running status ([efae65c](https://github.com/ATOVproject/faderpunk/commit/efae65cbb96de116c89c54e5e9753c1ee255c1fc))
* move build profiles to workspace ([ac28eaa](https://github.com/ATOVproject/faderpunk/commit/ac28eaae5b336b7cffbf07880c011f87f8263945))
* potential mutex deadlocks ([f52b35a](https://github.com/ATOVproject/faderpunk/commit/f52b35aef8aad9bbfa5fd232b1c3febead0256e4))
* restructure GlobalConfig to be Serialize, Deserialize ([b69c2ff](https://github.com/ATOVproject/faderpunk/commit/b69c2ff00d051807032c862c7e4320439dbb04e5))
* scene 0 should not recall "current" values ([ced46a4](https://github.com/ATOVproject/faderpunk/commit/ced46a45f7a1e96614bf4bdee02cab2d7ed88cab))
* **scenes:** extend scenes to 16 ([08a7337](https://github.com/ATOVproject/faderpunk/commit/08a7337f52a51dac0b1e911a43c12a8324db3b82))
* sequentialize FRAM reads and writes ([1acabc4](https://github.com/ATOVproject/faderpunk/commit/1acabc4ec943c733ebce7bf2e93235f75e249837))
* serialize large arrays ([fe2e65a](https://github.com/ATOVproject/faderpunk/commit/fe2e65a6d5304ed060ad85f592674390976dfeab))
* use correct mutex type for FRAM buffers ([def9269](https://github.com/ATOVproject/faderpunk/commit/def9269ed5bac918bf5f9e3a9416ee438afb8577))
* use direct memory access fram read buffers ([266374b](https://github.com/ATOVproject/faderpunk/commit/266374b2f79d1717d6f8ea00c5221ada9f79a604))
* use permanent receiver for clock ([534a088](https://github.com/ATOVproject/faderpunk/commit/534a0889c5d5560b06e96881b49b5fb5590a49c7))
* use read buffer pool for fram reads ([1e17020](https://github.com/ATOVproject/faderpunk/commit/1e170208238a043508a3708990540828db0d0792))
* use Signal instead of Watch for ParamStore ([33c36d3](https://github.com/ATOVproject/faderpunk/commit/33c36d35f5d540a5f5f4ae76f6942bbb4e7a4323))
* use stack buffer for fram reads for callers ([92ec92d](https://github.com/ATOVproject/faderpunk/commit/92ec92d809a14ac960c14a2b8b9bb6b26db9b5e9))
* wait for fram to be ready on startup ([e4e3a2c](https://github.com/ATOVproject/faderpunk/commit/e4e3a2c326a221934899464acf0859ca501f82ac))
