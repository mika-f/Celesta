// The parts of an AivisSpeech Engine `audio_query` result that Celesta reads.
// AivisSpeech serves VOICEVOX's API, but its Style-Bert-VITS2 model decides
// every phoneme's length itself: the query's `consonant_length` and
// `vowel_length` are always dummy zeros and synthesis ignores them, as it
// does `pauseLength`, `pauseLengthScale` and interrogative upspeak.
// Punctuation stays in `moras` as a mora whose vowel is 'pau' and whose
// `text` is the normalized mark (',', '.', '!', '?', '…'); `pause_mora` is
// null unless an editor added one, which is then read as a ','.
// Fields not listed here are allowed and ignored.

/** One mora of an AivisSpeech AudioQuery; lengths and pitch are ignored. */
export interface AivisSpeechMora {
  text: string;
  consonant?: string | null;
  vowel: string;
  consonant_length?: number | null;
  vowel_length?: number;
  pitch?: number;
}

export interface AivisSpeechAccentPhrase {
  moras: readonly AivisSpeechMora[];
  pause_mora?: AivisSpeechMora | null;
  accent?: number;
  is_interrogative?: boolean;
}

/**
 * The JSON returned by AivisSpeech Engine's `POST /audio_query` (and passed
 * to `POST /synthesis`).
 */
export interface AivisSpeechAudioQuery {
  accent_phrases: readonly AivisSpeechAccentPhrase[];
  speedScale?: number;
  prePhonemeLength?: number;
  postPhonemeLength?: number;
  /** The text read aloud; AivisSpeech uses it for intonation, not for the phonemes. */
  kana?: string | null;
}
