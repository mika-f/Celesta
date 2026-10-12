// A stand-in for an AivisSpeech voice: a WAV whose phonemes have known
// lengths, so the estimated times can be checked. Vowels are a loud
// harmonic tone, ん and voiced consonants quieter ones, fricatives noise,
// stops a silent closure then a burst; pauses, っ and the pre/post silence
// are silent.

const VOICED = new Set(['n', 'm', 'r', 'w', 'y', 'z', 'd', 'g', 'b', 'j', 'ny', 'my', 'ry', 'gy', 'by']);
const STOPS = new Set(['k', 't', 'p', 'ch', 'ts', 'ky', 'py']);

// The zundamon line 「こんにちは、ずんだもんなのだ。」 from VOICEVOX Engine,
// as AivisSpeech phrases it: punctuation is a 'pau' mora with a length
// here, and its mark as text.
export const KONNICHIWA = [
  [
    ['コ', 'k', 0.091, 'o', 0.146], ['ン', null, 0, 'N', 0.071], ['ニ', 'n', 0.033, 'i', 0.1],
    ['チ', 'ch', 0.085, 'i', 0.068], ['ワ', 'w', 0.071, 'a', 0.167], [',', null, 0, 'pau', 0.322],
  ],
  [
    ['ズ', 'z', 0.11, 'u', 0.112], ['ン', null, 0, 'N', 0.085], ['ダ', 'd', 0.035, 'a', 0.094],
    ['モ', 'm', 0.07, 'o', 0.117], ['ン', null, 0, 'N', 0.065], ['ナ', 'n', 0.038, 'a', 0.086],
  ],
  [['ノ', 'n', 0.055, 'o', 0.09], ['ダ', 'd', 0.05, 'a', 0.205], ['.', null, 0, 'pau', 0]],
];

/** The AivisSpeech query for phrases of [text, consonant, length, vowel, length]: lengths are dummy zeros. */
export function queryOf(phrases, extra = {}) {
  return {
    accent_phrases: phrases.map((moras) => ({
      moras: moras.map(([text, consonant, , vowel]) => ({
        text, consonant, consonant_length: consonant ? 0 : null, vowel, vowel_length: 0, pitch: 0,
      })),
      accent: 1,
      pause_mora: null,
      is_interrogative: false,
    })),
    speedScale: 1,
    prePhonemeLength: 0.1,
    postPhonemeLength: 0.1,
    kana: phrases.flat().map(([text]) => text).join(''),
    ...extra,
  };
}

/**
 * The WAV for phrases at a query's speed and pre/post silence, and the
 * true start and end of every mora (in seconds).
 */
export function synthesize(phrases, query, sampleRate = 16000) {
  const speed = query.speedScale ?? 1;
  const segments = [{ kind: 'silence', seconds: (query.prePhonemeLength ?? 0) / speed }];
  const moras = [];
  let clock = segments[0].seconds;
  for (const moras_ of phrases) {
    for (const [text, consonant, consonantLength, vowel, vowelLength] of moras_) {
      const start = clock;
      if (consonant) {
        segments.push({ kind: VOICED.has(consonant) ? 'hum' : STOPS.has(consonant) ? 'stop' : 'noise', seconds: consonantLength / speed });
        clock += consonantLength / speed;
      }
      const kind = vowel === 'pau' || vowel === 'cl' ? 'silence' : vowel === 'N' ? 'hum' : 'AIUEO'.includes(vowel) ? 'noise' : 'tone';
      segments.push({ kind, seconds: vowelLength / speed });
      clock += vowelLength / speed;
      moras.push({ text, start, end: clock });
    }
  }
  segments.push({ kind: 'silence', seconds: (query.postPhonemeLength ?? 0) / speed });

  const samples = [];
  let seed = 1;
  const noise = () => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 1073741824 - 1;
  };
  for (const { kind, seconds } of segments) {
    const count = Math.round(seconds * sampleRate);
    for (let i = 0; i < count; i += 1) {
      const t = (samples.length + i) / sampleRate;
      const harmonics = Math.sin(2 * Math.PI * 150 * t) + 0.5 * Math.sin(2 * Math.PI * 300 * t) + 0.3 * Math.sin(2 * Math.PI * 450 * t);
      let value = 0;
      if (kind === 'tone') value = 0.3 * harmonics;
      else if (kind === 'hum') value = 0.1 * harmonics;
      else if (kind === 'noise') value = 0.06 * noise();
      else if (kind === 'stop') value = i < count * 0.6 ? 0 : 0.1 * noise();
      samples.push(value);
    }
  }
  return { wav: encodeWav(samples, sampleRate), samples: Float32Array.from(samples), sampleRate, moras };
}

/** 16-bit mono PCM. */
export function encodeWav(samples, sampleRate) {
  const bytes = new Uint8Array(44 + samples.length * 2);
  const view = new DataView(bytes.buffer);
  const tag = (at, text) => [...text].forEach((c, i) => view.setUint8(at + i, c.charCodeAt(0)));
  tag(0, 'RIFF');
  view.setUint32(4, 36 + samples.length * 2, true);
  tag(8, 'WAVE');
  tag(12, 'fmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  tag(36, 'data');
  view.setUint32(40, samples.length * 2, true);
  samples.forEach((value, i) => view.setInt16(44 + i * 2, Math.max(-1, Math.min(1, value)) * 0x7fff, true));
  return bytes;
}
