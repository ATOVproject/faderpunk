/** Pitch-class names (C = 0). */
export const PC_NAMES = [
  "C",
  "C♯",
  "D",
  "D♯",
  "E",
  "F",
  "F♯",
  "G",
  "G♯",
  "A",
  "A♯",
  "B",
] as const;

/** Major-scale semitone offsets from tonic. */
export const MAJOR_DEGREES = [0, 2, 4, 5, 7, 9, 11] as const;

export type MonitorNote = {
  /** Scale degree 0…6 within the global key (major). */
  degree: number;
  /** MIDI octave of the tonic (C3 = octave 3 → MIDI 48). */
  octave: number;
};

export const DEFAULT_MONITOR_NOTE: MonitorNote = { degree: 0, octave: 3 };

export function clampKeyPc(pc: number): number {
  return ((Math.round(pc) % 12) + 12) % 12;
}

export function clampDegree(d: number): number {
  return Math.max(0, Math.min(6, Math.round(d)));
}

export function clampOctave(o: number): number {
  return Math.max(1, Math.min(6, Math.round(o)));
}

/** MIDI note number for key + degree + octave (tonic octave). */
export function monitorMidiNote(keyPc: number, note: MonitorNote): number {
  const tonic = clampKeyPc(keyPc) + (clampOctave(note.octave) + 1) * 12;
  const midi = tonic + MAJOR_DEGREES[clampDegree(note.degree)]!;
  return Math.max(0, Math.min(127, midi));
}

export function midiToHz(midi: number): number {
  return 440 * 2 ** ((midi - 69) / 12);
}

export function formatPc(pc: number): string {
  return PC_NAMES[clampKeyPc(pc)] ?? "C";
}

export function formatMonitorNote(keyPc: number, note: MonitorNote): string {
  const midi = monitorMidiNote(keyPc, note);
  const pc = midi % 12;
  const oct = Math.floor(midi / 12) - 1;
  return `${PC_NAMES[pc]}${oct}`;
}

/** Options for a <select>: value = "degree:octave". High pitches first, lows last. */
export function keyNoteOptions(
  keyPc: number,
  octaves: number[] = [2, 3, 4],
): { value: string; label: string }[] {
  const out: { value: string; label: string }[] = [];
  for (const octave of [...octaves].sort((a, b) => b - a)) {
    for (let degree = 6; degree >= 0; degree--) {
      const note = { degree, octave };
      out.push({
        value: `${degree}:${octave}`,
        label: formatMonitorNote(keyPc, note),
      });
    }
  }
  return out;
}

export function parseMonitorNoteValue(raw: string): MonitorNote {
  const [d, o] = raw.split(":").map(Number);
  return {
    degree: clampDegree(Number.isFinite(d) ? d : 0),
    octave: clampOctave(Number.isFinite(o) ? o : 3),
  };
}

export function monitorNoteValue(note: MonitorNote): string {
  return `${clampDegree(note.degree)}:${clampOctave(note.octave)}`;
}
