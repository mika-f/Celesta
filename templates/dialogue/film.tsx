import * as fs from 'node:fs';
import * as path from 'node:path';
import * as React from 'react';

import {
  Assets,
  Audio,
  Composition,
  Easings,
  Font,
  Group,
  Image,
  Rect,
  defineProjectProperties,
  getProjectProperty,
  interpolate,
  progress,
  useCurrentFrame,
  useTextMetrics,
} from '@celesta.studio/react';
import type { AssetReference, TextStyle } from '@celesta.studio/react';
import {
  Character,
  CharacterView,
  DialogueSeries,
  lipSyncFromKeyframes,
  loadLipSync,
  planDialogue,
  vowelShapes,
} from '@celesta.studio/character';
import type {
  CharacterSubtitle,
  CharacterViewReference,
  DialogueLine,
  DialoguePlan,
  ImageCharacterPortrait,
  SubtitleRenderProps,
} from '@celesta.studio/character';
import { useProjectProperty } from '@celesta.studio/project';
import { TextBox } from '@celesta.studio/text';

// Two characters talk, line by line, with a subtitle band and lip sync.
// A line with a recorded voice lasts as long as the recording and plays it;
// a line without one is timed from the length of its text, and the mouth
// moves through its vowels. Each line is followed by a short gap, with a
// longer pause where the scene changes.
//
// - props.json holds the values this project renders with: the title shown
//   at the top, the format (16:9 / 9:16), the script file, and the
//   background (an image, or a solid color) and music.
// - script.json holds the cast and the lines. Each character has a name, a
//   color for its subtitle band, a portrait per expression, and the mouth
//   images for lip sync; each line names its speaker and its text, and
//   optionally its voice file (a WAV), the kana reading the lip sync
//   follows, the expression, and the scene. Paths resolve from the folder
//   holding script.json, and the video gets longer or shorter with the
//   lines.
// - assets/ holds the sample portraits. Replace them with your own, and put
//   voice recordings in assets/voices/ to give lines a `voice`.
//
// Every layout number and motion timing is in the constants block below.

// ---- Constants --------------------------------------------------------------

const FPS = 30;
// Timing, in seconds. OPENING_SECONDS of quiet come before the first line
// and ENDING_SECONDS after the last; each line before the last is followed
// by LINE_GAP_SECONDS, and the first line of a new scene (a line whose
// `scene` differs from the line before) waits SCENE_LEAD_IN_SECONDS more.
// The music fades out over the last MUSIC_FADE_SECONDS.
const OPENING_SECONDS = 1;
const ENDING_SECONDS = 1;
const LINE_GAP_SECONDS = 0.3;
const SCENE_LEAD_IN_SECONDS = 1.5;
const MUSIC_FADE_SECONDS = 1;
// A line without a voice lasts SECONDS_PER_CHAR for each character of its
// text (punctuation and spaces included), and at least MIN_LINE_SECONDS.
// Its mouth opens on each vowel of its reading for MOUTH_OPEN_SHARE of the
// vowel's even share of the line, then closes until the next.
const SECONDS_PER_CHAR = 0.15;
const MIN_LINE_SECONDS = 1.5;
const MOUTH_OPEN_SHARE = 0.7;
// The music plays at MUSIC_VOLUME, under the voices.
const MUSIC_VOLUME = 0.25;
// Motion, in frames. The title fades in over TITLE_IN. The subtitle band
// fades in and out over BAND_FADE. When a character starts a line, its
// portrait hops HOP_HEIGHT pixels: up over the first HOP_UP frames of the
// line, and back down by frame HOP_DOWN.
const TITLE_IN = 18;
const BAND_FADE = 6;
const HOP_HEIGHT = 20;
const HOP_UP = 5;
const HOP_DOWN = 12;

const FONT = 'Noto Sans JP';
const FONT_SRC = 'https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;700';

const COLORS = {
  text: '#ffffff',
  // The subtitle band, edged in the speaker's color.
  band: '#141824e0',
};

// Layout, in pixels. Each format sets the canvas and the numbers that differ
// between a wide and a tall frame. The title sits `titleTop` below the top
// edge. The subtitle band is `bandHeight` tall with its bottom `bandBottom`
// above the bottom edge; its text holds `subtitleLines` lines. The portraits
// stand side by side in the stage, from `stageTop` to `stageBottom` (both
// measured from the top edge), each in an equal share of its width, as large
// as their share allows. 16:9 puts the portraits above the band; 9:16 puts
// them along the bottom and the band above them.
type Format = '16:9' | '9:16';
interface FormatLayout {
  width: number;
  height: number;
  titleTop: number;
  titleSize: number;
  titleLines: number;
  subtitleSize: number;
  subtitleLines: number;
  bandBottom: number;
  stageTop: number;
  stageBottom: number;
}
const FORMATS: Record<Format, FormatLayout> = {
  '16:9': {
    width: 1920, height: 1080, titleTop: 44, titleSize: 56, titleLines: 1,
    subtitleSize: 56, subtitleLines: 2, bandBottom: 40, stageTop: 150, stageBottom: 790,
  },
  '9:16': {
    width: 1080, height: 1920, titleTop: 220, titleSize: 80, titleLines: 2,
    subtitleSize: 60, subtitleLines: 3, bandBottom: 990, stageTop: 960, stageBottom: 1520,
  },
};
const MARGIN = 64;

// Text that does not fit shrinks, down to its minimum, wrapping onto at most
// its line count; text still too long at the minimum is cut off.
const TITLE_MIN_SIZE = 32;
const SUBTITLE_MIN_SIZE = 36;
const NAME_SIZE = 32;
const NAME_MIN_SIZE = 20;
// The height of a line, as a multiple of its font size.
const LINE_HEIGHT = 1.3;
// The subtitle band: its corner radius and edge width, and the room between
// its edges and the text. The top keeps more room, for the name plate
// centered on the band's top edge.
const BAND_RADIUS = 28;
const BAND_STROKE = 4;
const BAND_PADDING_X = 48;
const BAND_PADDING_TOP = 44;
const BAND_PADDING_BOTTOM = 28;
// The name plate: its height, the room either side of the name, its
// distance from the band's left edge, and its widest.
const PLATE_HEIGHT = 52;
const PLATE_PADDING_X = 28;
const PLATE_INSET = 40;
const PLATE_MAX_WIDTH = 480;
// The size portraits are assumed to be when it cannot be read from the file
// (it is read from PNG files).
const DEFAULT_PORTRAIT_SIZE = { width: 360, height: 400 };

defineProjectProperties({
  title: { type: 'string', label: 'Title', defaultValue: 'セリフの並べ方' },
  format: { type: 'select', label: 'Format', defaultValue: '16:9', options: ['16:9', '9:16'] },
  script: { type: 'path', label: 'Script', defaultValue: './script.json' },
  background: { type: 'path', label: 'Background image', defaultValue: '' },
  backgroundColor: { type: 'color', label: 'Background color', defaultValue: '#1f2433' },
  music: { type: 'path', label: 'Music', defaultValue: '' },
});

const MOUTHS = ['closed', 'a', 'i', 'u', 'e', 'o'] as const;

interface CastMember {
  id: string;
  name: string;
  color: string;
  portrait: ImageCharacterPortrait;
  // The size of the portrait images, in pixels.
  size: { width: number; height: number };
  asset: React.RefObject<AssetReference | null>;
  view: React.RefObject<CharacterViewReference | null>;
}

type Line = DialogueLine & { speaker: string };

let cast: CastMember[] = [];
// Filled by prepare(): every line's timing, from its voice or its text.
let plan: DialoguePlan<Line>;

// `getProjectProperty` sees the same values the render does, so the script
// read here and the duration planned from it always match.
export async function prepare(): Promise<void> {
  const file = getProjectProperty<string>('script');
  let parsed: unknown;
  try {
    parsed = JSON.parse(await fs.promises.readFile(file, 'utf8'));
  } catch (error) {
    throw new Error(`${file}: ${error instanceof Error ? error.message : String(error)}`);
  }
  const { characters, lines } = (parsed ?? {}) as { characters?: unknown; lines?: unknown };
  if (!isRecord(characters) || Object.keys(characters).length === 0) {
    throw new Error(`${file}: characters must be an object of { name, color, portraits, mouths } by speaker`);
  }
  if (!Array.isArray(lines) || lines.length === 0) {
    throw new Error(`${file}: lines must be a non-empty array of { speaker, text, voice?, ... }`);
  }
  const resolve = (relative: string) => path.resolve(path.dirname(file), relative);

  cast = Object.entries(characters).map(([id, entry]): CastMember => {
    const { name, color, portraits, mouths } = (entry ?? {}) as Record<string, unknown>;
    const files = (what: string, record: unknown, keys?: readonly string[]): Record<string, string> => {
      if (!isRecord(record)) throw new Error(`${file}: character "${id}" has no ${what}`);
      return Object.fromEntries((keys ?? Object.keys(record)).map((key) => {
        const value = record[key];
        if (typeof value !== 'string' || value === '') {
          throw new Error(`${file}: character "${id}" has no "${key}" ${what.replace(/s$/, '')}`);
        }
        const resolved = resolve(value);
        if (!fs.existsSync(resolved)) {
          throw new Error(`${file}: character "${id}" ${what.replace(/s$/, '')} "${key}" ${resolved} does not exist`);
        }
        return [key, resolved];
      }));
    };
    if (typeof name !== 'string' || typeof color !== 'string') {
      throw new Error(`${file}: character "${id}" needs a string "name" and "color"`);
    }
    const expressions = files('portraits', portraits);
    const lipSync = files('mouths', mouths, MOUTHS);
    // The first portrait listed is the expression shown when a line names none.
    const [defaultExpression] = Object.keys(expressions);
    if (defaultExpression === undefined) throw new Error(`${file}: character "${id}" has no portraits`);
    return {
      id,
      name,
      color,
      portrait: { defaultExpression, expressions, lipSync: lipSync as Record<(typeof MOUTHS)[number], string> },
      size: imageSize(expressions[defaultExpression]),
      asset: React.createRef<AssetReference>(),
      view: React.createRef<CharacterViewReference>(),
    };
  });

  const checked = lines.map((entry: unknown, i) => {
    const { speaker, text, reading, expression, voice, scene } = (entry ?? {}) as Record<string, unknown>;
    const member = cast.find(({ id }) => id === speaker);
    if (member === undefined) {
      throw new Error(`${file}: line ${i}: speaker "${String(speaker)}" is not in characters`);
    }
    if (expression !== undefined && !(typeof expression === 'string' && Object.hasOwn(member.portrait.expressions, expression))) {
      throw new Error(`${file}: line ${i}: ${member.id} has no "${String(expression)}" portrait`);
    }
    if (typeof text !== 'string') {
      throw new Error(`${file}: line ${i} needs a string "text"`);
    }
    if ([reading, scene, voice].some((value) => value !== undefined && typeof value !== 'string')) {
      throw new Error(`${file}: line ${i} has a "reading", "scene", or "voice" that is not a string`);
    }
    const audio = voice === undefined || voice === '' ? undefined : resolve(voice as string);
    if (audio !== undefined && !fs.existsSync(audio)) {
      throw new Error(`${file}: line ${i}: voice ${audio} does not exist`);
    }
    return { speaker: member.id, text, reading: reading as string | undefined, expression, audio, scene: scene as string | undefined };
  });
  const planned = await Promise.all(checked.map(async ({ reading, ...line }, i): Promise<Line> => {
    // Lip sync follows the kana reading; kanji in `text` have no vowels to follow.
    const spoken = reading ?? line.text;
    const timing = line.audio !== undefined
      ? { lipSync: await loadVoice(file, i, line.audio, spoken) }
      : textTiming(line.text, spoken);
    return {
      ...line,
      ...timing,
      ...(i === 0 ? { leadIn: OPENING_SECONDS } : {}),
      ...(i === checked.length - 1 ? { gap: ENDING_SECONDS } : {}),
    };
  }));
  plan = await planDialogue(planned, { fps: FPS, gap: LINE_GAP_SECONDS, sceneLeadIn: SCENE_LEAD_IN_SECONDS });
}

// A recorded line's lip sync, with a failure to read the recording (a file
// that is not an uncompressed WAV, say) named by script, line, and voice.
async function loadVoice(file: string, i: number, src: string, text: string) {
  try {
    return await loadLipSync({ src, text });
  } catch (error) {
    throw new Error(`${file}: line ${i}: voice ${src}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

// A line without a voice: its length from its text, and a mouth that opens
// on each vowel of `spoken` in turn, evenly over the line. Text without
// vowels to follow opens the mouth once per character.
function textTiming(text: string, spoken: string): Pick<Line, 'durationInFrames' | 'lipSync'> {
  const characters = [...text].length;
  // Rounded up to whole frames, like a voice; the tolerance keeps float error
  // (0.15 * 12 * 30 = 54.00000000000001) from adding a frame.
  const durationInFrames = Math.ceil(Math.max(MIN_LINE_SECONDS, characters * SECONDS_PER_CHAR) * FPS - 1e-6);
  const seconds = durationInFrames / FPS;
  const vowels = vowelShapes(spoken);
  const shapes = vowels.length > 0 ? vowels : Array.from({ length: characters }, () => 'a' as const);
  const share = seconds / Math.max(1, shapes.length);
  const lipSync = lipSyncFromKeyframes(
    shapes.flatMap((mouth, index) => [
      { seconds: index * share, mouth },
      { seconds: (index + MOUTH_OPEN_SHARE) * share, mouth: 'closed' as const },
    ]),
    seconds,
  );
  return { durationInFrames, lipSync };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

// A PNG's width and height from its header; other images are assumed to be
// DEFAULT_PORTRAIT_SIZE.
function imageSize(file: string): { width: number; height: number } {
  const header = fs.readFileSync(file).subarray(0, 24);
  const png = header.length === 24 && header.readUInt32BE(0) === 0x89504e47 && header.toString('ascii', 12, 16) === 'IHDR';
  return png ? { width: header.readUInt32BE(16), height: header.readUInt32BE(20) } : DEFAULT_PORTRAIT_SIZE;
}

function useLayout() {
  const format = FORMATS[useProjectProperty<Format>('format')];
  const bandHeight = BAND_PADDING_TOP + format.subtitleLines * format.subtitleSize * LINE_HEIGHT + BAND_PADDING_BOTTOM;
  const band = { x: MARGIN, y: format.height - format.bandBottom - bandHeight, width: format.width - 2 * MARGIN, height: bandHeight };
  return { ...format, band };
}

function Background() {
  const { width, height } = useLayout();
  const image = useProjectProperty<string>('background');
  return (
    <>
      <Rect width={width} height={height} fill={useProjectProperty<string>('backgroundColor')} />
      {image !== '' && <Image src={image} width={width} height={height} fit="cover" />}
    </>
  );
}

function Title() {
  const frame = useCurrentFrame();
  const { width, titleTop, titleSize, titleLines } = useLayout();
  return (
    <TextBox
      x={MARGIN}
      y={titleTop}
      width={width - 2 * MARGIN}
      height={titleLines * titleSize * LINE_HEIGHT}
      minFontSize={TITLE_MIN_SIZE}
      maxFontSize={titleSize}
      maxLines={titleLines}
      lineHeight={LINE_HEIGHT}
      opacity={progress(frame, 0, TITLE_IN, Easings.easeOutCubic)}
      style={{ fontFamily: FONT, fontWeight: 700, align: 'center', fill: { type: 'solid', color: COLORS.text } }}
    >
      {useProjectProperty<string>('title')}
    </TextBox>
  );
}

// A character's portrait, standing on the bottom of its share of the stage.
// The speaker hops when its line starts.
function Member({ member, index }: { member: CastMember; index: number }) {
  const frame = useCurrentFrame();
  const { width, stageTop, stageBottom } = useLayout();
  const speaking = plan.lines.find(({ from, durationInFrames, line }) =>
    line.speaker === member.id && frame >= from && frame < from + durationInFrames);
  const share = (width - 2 * MARGIN) / cast.length;
  const scale = Math.min(share / member.size.width, (stageBottom - stageTop) / member.size.height);
  const hop = speaking
    ? interpolate(frame - speaking.from, [0, HOP_UP, HOP_DOWN], [0, -HOP_HEIGHT, 0], {
      extrapolateRight: 'clamp',
      easing: Easings.easeOutQuad,
    })
    : 0;
  return (
    <Group
      x={MARGIN + (index + 0.5) * share - (member.size.width * scale) / 2}
      y={stageBottom - member.size.height * scale + hop}
      scale={scale}
    >
      <CharacterView ref={member.view} character={member.asset} mouth="closed" />
    </Group>
  );
}

const NAME_STYLE: TextStyle = { fontFamily: FONT, fontWeight: 700, fontSize: NAME_SIZE };

// The subtitle band: a rounded panel edged in the speaker's color, with the
// speaker's name on a plate across its top edge. Between lines (`held`) the
// band stays without the text; it fades in when a run of lines starts and
// out when it ends.
function SubtitleBand({ text, character, held, frame, durationInFrames }: SubtitleRenderProps) {
  const { band, subtitleSize, subtitleLines } = useLayout();
  const color = cast.find(({ id }) => id === character.id)?.color ?? COLORS.text;
  const plateWidth = Math.min(PLATE_MAX_WIDTH, useTextMetrics(character.displayName, NAME_STYLE).width + 2 * PLATE_PADDING_X);
  const fade = Math.min(BAND_FADE, durationInFrames / 3);
  const opacity = interpolate(frame + 0.5, [0, fade, durationInFrames - fade, durationInFrames], [0, 1, 1, 0], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  return (
    <Group opacity={opacity}>
      <Rect width={band.width} height={band.height} cornerRadius={BAND_RADIUS} fill={COLORS.band} stroke={color} strokeWidth={BAND_STROKE} />
      <Rect x={PLATE_INSET} y={-PLATE_HEIGHT / 2} width={plateWidth} height={PLATE_HEIGHT} cornerRadius={PLATE_HEIGHT / 2} fill={color} />
      <TextBox
        x={PLATE_INSET + PLATE_PADDING_X}
        y={-PLATE_HEIGHT / 2}
        width={plateWidth - 2 * PLATE_PADDING_X}
        height={PLATE_HEIGHT}
        minFontSize={NAME_MIN_SIZE}
        maxFontSize={NAME_SIZE}
        maxLines={1}
        lineHeight={LINE_HEIGHT}
        verticalAlign="middle"
        style={{ ...NAME_STYLE, align: 'center', fill: { type: 'solid', color: COLORS.text } }}
      >
        {character.displayName}
      </TextBox>
      {!held && (
        <TextBox
          x={BAND_PADDING_X}
          y={BAND_PADDING_TOP}
          width={band.width - 2 * BAND_PADDING_X}
          height={subtitleLines * subtitleSize * LINE_HEIGHT}
          minFontSize={SUBTITLE_MIN_SIZE}
          maxFontSize={subtitleSize}
          maxLines={subtitleLines}
          lineHeight={LINE_HEIGHT}
          verticalAlign="middle"
          style={{ fontFamily: FONT, fontWeight: 700, align: 'center', fill: { type: 'solid', color: COLORS.text } }}
        >
          {text}
        </TextBox>
      )}
    </Group>
  );
}

// The music from the start, starting over when it is shorter than the
// conversation, fading out over the last MUSIC_FADE_SECONDS.
function Music() {
  const music = useProjectProperty<string>('music');
  if (music === '') return null;
  return <Audio src={music} loop volume={MUSIC_VOLUME} fadeOut={MUSIC_FADE_SECONDS} />;
}

export default function Conversation() {
  const { width, height, band, subtitleSize } = useLayout();
  // The band draws its own text in a <TextBox>; `style` here only measures it.
  const subtitle: CharacterSubtitle = {
    x: band.x,
    y: band.y,
    style: { fontFamily: FONT, fontWeight: 700, fontSize: subtitleSize },
    render: (props) => <SubtitleBand {...props} />,
  };
  return (
    <Composition width={width} height={height} fps={FPS} durationInFrames={plan.durationInFrames}>
      <Font src={FONT_SRC} />
      <Assets>
        {cast.map((member) => (
          <Character key={member.id} ref={member.asset} id={member.id} name={member.name} portrait={member.portrait} subtitle={subtitle} />
        ))}
      </Assets>
      <Background />
      <Title />
      {cast.map((member, index) => <Member key={member.id} member={member} index={index} />)}
      <DialogueSeries plan={plan} views={Object.fromEntries(cast.map(({ id, view }) => [id, view]))} holdSubtitle />
      <Music />
    </Composition>
  );
}
