# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/ATOVproject/faderpunk/releases/tag/v0.1.0) - 2025-06-20

### Added

- use static buffer for fram reads
- restructure Arr and AppStorage
- vastly improve Storage API
- use PubSubChannel for clock
- make max and midi channels CriticalSectionRawMutex Channels
- use ClockEvent instead of bool for clock Watch
- add param load and save for apps
- re-spawn apps on param change
- add and set params for apps
- store GlobalConfig in FRAM
- add param and cleanup loops to all apps
- move param handler into param store
- add app cleanup method
- *(configurator)* set custom layouts
- *(configurator)* implement layout setting
- store and recall current values using rpc
- StorageSlot is now dependent on Store
- ParamStore -> Store, impl ser and des for Store
- *(storage)* allow long arrays for storage slots
- *(eeprom)* read-before-write
- *(storage)* pre-load everything from eeprom
- *(storage)* add wait_for_scene_change method
- *(scene)* integrate scenes with scene button
- *(scene)* add simple scene implementation for StorageSlots
- *(config)* move storage globals into app_config
- *(config)* always require params() in config macro
- *(config)* set a param from configurator
- *(config)* retrieve app state from configurator
- add AppParams macro and storage
- simplify cross core message routing
- *(app)* allow storing arrays
- use StorageSlots for app storage values
- add sequential storage using eeprom
- *(leds)* add glitchy startup animation
- *(midi)* add MidiIn and MidiUSB clock sources
- *(leds)* set shift and scene button to white
- *(midi)* send custom cc value
- make midi channel configurable in default app
- refactor midi into struct
- refactor leds a bit, add chan clamping
- add wait_for_any_long_press function to app
- improve lfo
- add button debounce, long press
- redesign app parts, restructure waiters
- add mute led to default app
- (very) simple button debounce
- use batch messages for app listing
- decode large configuration messages
- add postcard encoded app config list
- add gen-bindings, restructure project

### Fixed

- drop guard for storage before saving
- move build profiles to workspace
- loading of Globalconfig
- midi uart message drops
- wait for fram to be ready on startup
- restructure GlobalConfig to be Serialize, Deserialize
- *(eeprom)* raise storage bytes limit
- *(buttons)* improve scene save debounce
- alter macro to account for apps without params
- use Signal instead of Watch for ParamStore
- *(midi)* quick fix for midi tx over uart. remove running status
- serialize large arrays
- *(midi)* proper midi 1 implementation using midly
- *(clock)* improve clock reset behavior
- clock fixes and clock debug app
- use permanent receiver for clock

### Other

- global config not loading
- change apps to be individual tasks
- Merge pull request #30 from ATOVproject/dependabot/cargo/defmt-1.0.1
- faster eeprom reads using an index
