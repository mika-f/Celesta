// The parts of a VOICEVOX Engine `audio_query` result that Celesta reads.
// Engines that share VOICEVOX's API (AivisSpeech, COEIROINK v2, ...) return
// the same shape, but AivisSpeech ignores the phoneme and pause lengths when
// synthesizing, so times computed from its queries do not match its WAVs.
// Fields not listed here are allowed and ignored.

/** One mora of a VOICEVOX AudioQuery. */
export interface VoicevoxMora {
  text?: string;
  consonant?: string | null;
  consonant_length?: number | null;
  vowel: string;
  vowel_length: number;
  pitch?: number;
}

export interface VoicevoxAccentPhrase {
  moras: readonly VoicevoxMora[];
  pause_mora?: VoicevoxMora | null;
  is_interrogative?: boolean;
}

/**
 * The JSON returned by VOICEVOX Engine's `POST /audio_query` (and passed to
 * `POST /synthesis`).
 */
export interface VoicevoxAudioQuery {
  accent_phrases: readonly VoicevoxAccentPhrase[];
  speedScale?: number;
  prePhonemeLength?: number;
  postPhonemeLength?: number;
  /** Newer engines: when not null, replaces the length of every pause mora. */
  pauseLength?: number | null;
  /** Newer engines: multiplies the length of every pause mora. */
  pauseLengthScale?: number;
}
