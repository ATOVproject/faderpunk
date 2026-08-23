# I²C integration

Faderpunk uses its external I²C bus as a leader by default. The bus runs at
400 kHz on GPIO 21 (SCL) and GPIO 20 (SDA). This bus is separate from the
internal I²C bus used for FRAM storage.

## Electrical requirements

I²C requires pull-up resistors on both SDA and SCL. Enable one suitable set of
pull-ups on the bus—for example, the configurable pull-ups on a disting NT—if
no other connected device supplies them. Missing pull-ups can cause intermittent
messages, NACKs, or a stuck bus.

Power followers before Faderpunk. In leader mode, Faderpunk waits 10 seconds
and then scans the bus once. Devices connected or enabled after that scan are
not discovered until Faderpunk is rebooted.

Changing the I²C mode in the configurator is persisted immediately, but the bus
mode is selected only at startup, so the change requires a reboot.

## Recognized follower addresses

Faderpunk probes the complete normal I²C address range but sends controller
updates only when it discovers a recognized address:

| Address | Compatibility |
| --- | --- |
| `0x20` | monome Ansible |
| `0x31` | ER-301-compatible controller messages |
| `0x60`–`0x67` | TXo/Telexo device range |

For its 16 channels, current TXo routing sends to addresses `0x60`–`0x63`, with
four controller ports per address.

### disting NT compatibility

A disting NT can use the ER-301-compatible path. Configure the NT's follower
address as `0x31`, then map its I²C controllers to the desired parameters.
Faderpunk sends this four-byte payload to that address:

```text
0x11 <controller> <value MSB> <value LSB>
```

The disting NT documents `0x11` as “set I²C controller X to value Y,” so it
accepts the same message that Faderpunk sends for an ER-301. The NT's follower
address is configurable: `0x31` is the compatibility setting used here, not an
address permanently assigned to every NT. Do not place another `0x31` follower
on the same bus.

Controller numbers are zero-based physical Faderpunk channel numbers. Values
are scaled from Faderpunk's `0`–`4095` range to `0`–`16383`.

## Apps that produce controller messages

I²C leader mode does not automatically transmit every physical fader. Apps must
explicitly publish an I²C output:

- **Control** sends its processed controller value on its physical channel.
- **Panner** sends its processed left and right outputs on its two physical
  channels. This includes pan modulation, attenuation, mute, and slew.

Other apps do not currently produce I²C controller messages.

Continuous controller updates use latest-value semantics. Each physical channel
has at most one pending value; a newer value replaces an older value that has
not yet been transmitted. This prevents stale movement from playing out after
the fader stops and ensures that the final position is not discarded merely
because an update queue filled.

## Other modes

Normal follower mode at address `0x56` is currently unimplemented. Calibration
mode is a separate follower protocol at address `0x57`, using Postcard-encoded
commands for ADC, DAC, and calibration operations.
