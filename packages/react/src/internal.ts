// `@celesta/react/internal`: the runtime hooks the other Celesta packages and
// the Celesta CLI build on. Not for compositions; nothing here is stable.

export { createResolver, mount } from './render';
export type {
  AudioClipDescriptor,
  ComponentResolution,
  ComponentResolutionRequest,
  EntryComponent,
  MountedComposition,
  ProjectFrame,
  ResolutionRuntime,
  Resolver,
} from './render';
export type { HostNode } from './reconciler';
export { registerHostElement } from './host-elements';
export type { HostElement, HostVisit, HostWalk } from './host-elements';

export {
  CompositionRuntimeContext,
  FreezeFrameContext,
  ProjectLayersContext,
  ProjectTrackLayersContext,
  RerenderRequestContext,
  RootRuntimeContext,
  resolveTextLanguage,
} from './hooks';
export type { CompositionRuntimeContextValue } from './hooks';

export {
  asynchronousMeasurer,
  prepareFonts,
  setTextMeasurer,
  synchronousMeasurer,
  useMeasurementFonts,
  withTextLanguage,
} from './text-measure';
export type { MeasureTextRequest } from './text-measure';
export { flattenTextContent, sliceTextRuns, withTextRuns } from './rich-text';
export type { FlatText } from './rich-text';

export { probeMedia, setMediaProbe } from './media';
export type { MediaAudioInfo, MediaInfo, MediaVideoInfo, ProbedMediaInfo } from './media';

export { listComponentSchemas, resolveComponent } from './registry';
export {
  declaredDefault,
  holdProjectPropertyValues,
  inputProjectProperty,
  setProjectPropertyValues,
} from './properties';

export { SECONDS_TIMESCALE, secondsFromTime, secondsToTime } from './time';
export { entryRelativePath, isRemoteUrl } from './entry-dir';

// Rust types of the project document (`celesta-project`), which
// `@celesta/project` and `@celesta/character` re-export; ts-rs generates them
// into this package beside the scene types they reference.
export type { Project } from './generated/Project';
export type { Asset } from './generated/Asset';
export type { AssetSource } from './generated/AssetSource';
export type { Character as CharacterDefinition } from './generated/Character';
export type { LipSyncCue } from './generated/LipSyncCue';
export type { LipSyncDefinition } from './generated/LipSyncDefinition';
export type { MouthShape } from './generated/MouthShape';
export type { PortraitDefinition } from './generated/PortraitDefinition';
export type { SubtitleDefinition } from './generated/SubtitleDefinition';
export type { ProjectSettings } from './generated/ProjectSettings';
export type { ProjectVersion } from './generated/ProjectVersion';
export type { SourceRange } from './generated/SourceRange';
export type { Track } from './generated/Track';
export type { TrackKind } from './generated/TrackKind';
export type { TimelineItem } from './generated/TimelineItem';
export type { TimelineContent } from './generated/TimelineContent';
