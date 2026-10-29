export { Character, CharacterView, Dialogue, expressionAt } from './components';
export type {
  CharacterLipSync,
  CharacterPortrait,
  CharacterProps,
  CharacterReference,
  CharacterSubtitle,
  CharacterViewProps,
  CharacterViewReference,
  DialogueExpression,
  DialogueProps,
  ExpressionCue,
  ImageCharacterBlink,
  ImageCharacterPortrait,
  PsdCharacterBlink,
  PsdCharacterLipSync,
  PsdCharacterPortrait,
  PsdExpression,
  SubtitleCharacter,
  SubtitleRenderProps,
} from './components';

export { DialogueSeries, planDialogue } from './dialogue-series';
export type {
  DialogueLine,
  DialoguePlan,
  DialogueRange,
  DialogueScene,
  DialogueSeriesProps,
  PlanDialogueOptions,
  PlannedDialogueLine,
} from './dialogue-series';

export { paginateSubtitles } from './paginate-subtitles';
export type { PaginateSubtitlesOptions } from './paginate-subtitles';
export type {
  ResolvedSubtitlePage,
  SubtitlePage,
  SubtitlePageInfo,
  SubtitlePageTiming,
  SubtitleSegment,
} from './subtitle-pages';

export { loadDialogueLipSync, spokenTextOf } from './spoken-text';
export type { LoadDialogueLipSyncOptions } from './spoken-text';

export { importVoiceFolder, naturalCompare } from './voice-folder';
export type {
  ImportVoiceFolderOptions,
  VoiceLine,
  VoiceManifest,
  VoiceManifestEntry,
  VoiceTextDecoder,
  VoiceTextEncoding,
} from './voice-folder';

export { blinkPhase } from './blink';
export type { BlinkPhase, BlinkTiming } from './blink';

export {
  buildEnvelope,
  decodeWav,
  decodeWavChannels,
  loadLipSync,
  lipSyncFromKeyframes,
  lipSyncTimeline,
  readWavHeader,
  useLipSync,
  vowelShapes,
} from './lipsync';
export type { LipSyncOptions, LipSyncTrack, MouthKeyframe, WavAudio, WavChannels, WavHeader } from './lipsync';

export {
  VOICE_LEVEL_ANALYSIS_VERSION,
  analyzeVoiceLevel,
  matchVoiceLevels,
  measureVoiceLevel,
  scaleVolume,
} from './voice-level';
export type {
  MatchedVoiceLevels,
  VoiceLevelAnalysis,
  VoiceLevelLimit,
  VoiceLevelLineReport,
  VoiceLevelLineSetting,
  VoiceLevelOptions,
  VoiceLevelReport,
  VoiceLevelSpeaker,
} from './voice-level';

export { duckingVolume, speechIntervals } from './ducking';

export { lipSyncFromSpeechTiming, speechVowelShape } from './speech-timing';
export type {
  SpeechAccentPhrase,
  SpeechMora,
  SpeechPause,
  SpeechPhoneme,
  SpeechSpan,
  SpeechTiming,
} from './speech-timing';

export { loadPsdPreset, parsePfv, resolveVisibleLayers } from './psd-preset';
export type { LoadPsdPresetOptions, ParsedPfv, PfvFavorite } from './psd-preset';

export type { MouthShape } from '@celesta/react/internal';
