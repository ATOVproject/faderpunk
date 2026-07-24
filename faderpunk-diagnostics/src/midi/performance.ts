export type MidiKind = "noteOn" | "noteOff" | "cc" | "nrpn" | "clock" | "transport" | "other";

export interface MidiEvent {
  t: number; // performance.now()
  kind: MidiKind;
  channel: number; // 1–16
  note?: number;
  velocity?: number;
  cc?: number;
  value?: number; // 0–1 normalized for continuous, or 0–127 raw for note vel
  rawValue?: number; // 0–127 or 0–16383 for NRPN
}

interface NrpnState {
  msb: number | null;
  lsb: number | null;
  dataMsb: number | null;
}

/** Parse Web MIDI bytes into high-level events (handles running status + NRPN). */
export class PerformanceParser {
  private runningStatus = 0;
  private nrpn: NrpnState[] = Array.from({ length: 16 }, () => ({
    msb: null,
    lsb: null,
    dataMsb: null,
  }));

  parse(data: Uint8Array, t = performance.now()): MidiEvent[] {
    const out: MidiEvent[] = [];
    let i = 0;
    while (i < data.length) {
      let status = data[i];
      if (status < 0x80) {
        if (!this.runningStatus) {
          i++;
          continue;
        }
        status = this.runningStatus;
      } else {
        i++;
        if (status < 0xf0) this.runningStatus = status;
      }

      if (status === 0xf8) {
        out.push({ t, kind: "clock", channel: 0 });
        continue;
      }
      if (status === 0xfa || status === 0xfb || status === 0xfc || status === 0xff) {
        out.push({ t, kind: "transport", channel: 0, rawValue: status });
        continue;
      }
      if (status >= 0xf0) {
        // Skip other system messages / sysex fragments on performance port
        if (status === 0xf0) {
          while (i < data.length && data[i] !== 0xf7) i++;
          if (i < data.length) i++;
        }
        continue;
      }

      const channel = (status & 0x0f) + 1;
      const cmd = status & 0xf0;
      const d1 = data[i++] ?? 0;
      const needs2 = cmd !== 0xc0 && cmd !== 0xd0;
      const d2 = needs2 ? (data[i++] ?? 0) : 0;

      if (cmd === 0x90) {
        if (d2 === 0) {
          out.push({ t, kind: "noteOff", channel, note: d1, velocity: 0, value: 0 });
        } else {
          out.push({
            t,
            kind: "noteOn",
            channel,
            note: d1,
            velocity: d2,
            value: d2 / 127,
            rawValue: d2,
          });
        }
      } else if (cmd === 0x80) {
        out.push({ t, kind: "noteOff", channel, note: d1, velocity: d2, value: 0 });
      } else if (cmd === 0xb0) {
        const nrpnEvent = this.consumeCc(channel, d1, d2, t);
        if (nrpnEvent) out.push(nrpnEvent);
        else {
          out.push({
            t,
            kind: "cc",
            channel,
            cc: d1,
            value: d2 / 127,
            rawValue: d2,
          });
        }
      }
    }
    return out;
  }

  private consumeCc(
    channel: number,
    cc: number,
    value: number,
    t: number,
  ): MidiEvent | null {
    const st = this.nrpn[channel - 1];
    if (cc === 99) {
      st.msb = value;
      return null;
    }
    if (cc === 98) {
      st.lsb = value;
      return null;
    }
    if (cc === 6) {
      st.dataMsb = value;
      return null;
    }
    if (cc === 38 && st.msb !== null && st.lsb !== null && st.dataMsb !== null) {
      const param = (st.msb << 7) | st.lsb;
      const raw = (st.dataMsb << 7) | value;
      st.dataMsb = null;
      return {
        t,
        kind: "nrpn",
        channel,
        cc: param & 0x7f,
        value: raw / 16383,
        rawValue: raw,
      };
    }
    return null;
  }
}
