// What the export measured for this film's mix, copied from the JSON of
//   Celesta-export --json --loudness streaming --react examples/delta/film-0.9.0.tsx delta-0.9.0.mp4
// (the "wrote" event's output.loudness). The mix is the score, its fades and
// ducking, and the three voices: the picture does not change it, so the
// values hold until the audio does. Re-measure after changing any of them.
export const MEASURED = {
  /** integratedLufs after the gain. */
  integratedLufs: -14.05,
  /** truePeakDbtp after the gain. */
  truePeakDbtp: -1.0,
  /** gainDb that --loudness applied. */
  gainDb: 2.36,
};

/** The mix's loudness as it was before the gain. */
export const BEFORE_LUFS = MEASURED.integratedLufs - MEASURED.gainDb;
