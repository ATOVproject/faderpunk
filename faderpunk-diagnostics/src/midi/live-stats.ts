/** High-frequency session counters — mutated without React; UI polls. */
export const liveStats = {
  clockCount: 0,
  ccCount: 0,
  noteCount: 0,
  loopbackCount: 0,
};

export function resetLiveStats(): void {
  liveStats.clockCount = 0;
  liveStats.ccCount = 0;
  liveStats.noteCount = 0;
  liveStats.loopbackCount = 0;
}
