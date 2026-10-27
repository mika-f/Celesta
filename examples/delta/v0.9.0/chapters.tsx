import { AddDemo } from './demos/AddDemo';
import { CalloutDemo } from './demos/CalloutDemo';
import { CanvasDemo } from './demos/CanvasDemo';
import { CaptionsDemo } from './demos/CaptionsDemo';
import { DialogueDemo } from './demos/DialogueDemo';
import { DuckingDemo } from './demos/DuckingDemo';
import { FadesDemo } from './demos/FadesDemo';
import { LoudnessDemo } from './demos/LoudnessDemo';
import { LowerThirdDemo } from './demos/LowerThirdDemo';
import { PropertiesDemo } from './demos/PropertiesDemo';
import { ScopeDemo } from './demos/ScopeDemo';
import { TimelineDemo } from './demos/TimelineDemo';
import { hi } from './program';
import type { Chapter } from './program';

// The twelve features from v0.8.0 to main, in running order: what goes on
// screen (LOWER THIRD to DIALOGUE), what goes into the mix (FADES to
// LOUDNESS), and the editor writing the TSX (PROPERTIES to ADD). MORE, the
// twelve cards after them, is in More.tsx.

export const CHAPTERS: readonly Chapter[] = [
  {
    key: 'name and role captions',
    title: 'LOWER THIRD',
    headline: <>Say who is {hi('on screen')}.</>,
    caption: 'LowerThird puts a name and a role on a band sized to them, in any corner, inside the safe area.',
    file: 'program.tsx',
    code: '<LowerThird title="しずく"\n  subtitle="ナレーター" accent="orange" />',
    merged: [[442, 'LowerThird']],
    meta: '<LayoutBounds 1000×563>',
    Demo: LowerThirdDemo,
  },
  {
    key: 'notes that point',
    title: 'CALLOUT',
    headline: <>Point at it, and {hi('keep pointing')}.</>,
    caption: 'Callout draws a note sized to its text and a line to its target, and the line follows a moving target.',
    file: 'CalloutDemo.tsx',
    code: '<Callout target={apex} x={60} y={60}\n  width={300} title="target={apex}"\n  lineProgress={grow} />',
    merged: [[443, 'Callout']],
    Demo: CalloutDemo,
  },
  {
    key: 'captions from data',
    title: 'CAPTIONS',
    headline: <>Captions that {hi('follow the voice')}.</>,
    caption: 'CaptionTrack draws timed captions from data and lights them a word or a phrase at a time.',
    file: 'CaptionsDemo.tsx',
    code: '<CaptionTrack cues={cues} wordUnit="phrase"\n  highlight={{ background: "#F26A2E" }} />',
    merged: [[451, 'CaptionTrack'], [499, 'word by word']],
    meta: 'voice-1.wav',
    Demo: CaptionsDemo,
  },
  {
    key: 'what it shows, what it says',
    title: 'DIALOGUE',
    headline: <>Show one thing, {hi('say another')}.</>,
    caption: 'A dialogue line says its spokenText, shows its text a page at a time, and emphasizes the words that matter.',
    file: 'voices.ts',
    code: "{ text, spokenText: 'おんせいと…',\n  subtitlePages, emphasis }",
    merged: [[452, 'spokenText'], [453, 'subtitle pages'], [466, 'dialogue ducking']],
    meta: 'voice-2.wav',
    Demo: DialogueDemo,
  },
  {
    key: 'in and out, in seconds',
    title: 'FADES',
    headline: <>Fade in. {hi('Fade out.')}</>,
    caption: 'fadeIn and fadeOut on Audio, in seconds, at the edges of what is heard, following the sequence when it changes.',
    file: 'film-0.9.0.tsx',
    code: '<Audio src={SCORE_SRC}\n  fadeIn={2.4} fadeOut={3.6} />',
    merged: [[444, 'fadeIn / fadeOut']],
    meta: 'score-0.9.0.wav',
    Demo: FadesDemo,
  },
  {
    key: 'music under the voice',
    title: 'DUCKING',
    headline: <>The music {hi('steps back')}.</>,
    caption: 'Mark the voice with duckGroup, give the music duck, and it dips while the voice plays, in preview and export alike.',
    file: 'film-0.9.0.tsx',
    code: "<Audio src={SCORE_SRC} duck={{\n  under: 'voice', to: { db: -12 } }} />",
    merged: [[465, 'duck'], [466, 'from a dialogue plan'], [463, 'browser export']],
    meta: 'voice-10.wav',
    Demo: DuckingDemo,
  },
  {
    key: 'measured, then matched',
    title: 'LOUDNESS',
    headline: <>Every export, {hi('at -14 LUFS')}.</>,
    caption: 'Every export measures its mix, and --loudness brings it to a target with one gain and a true-peak limit.',
    file: 'package.json',
    language: 'bash',
    code: 'Celesta-export --loudness streaming \\\n  --react film-0.9.0.tsx delta-0.9.0.mp4',
    merged: [[479, '--loudness'], [480, 'Export… target']],
    Demo: LoudnessDemo,
  },
  {
    key: 'edit what the code says',
    title: 'PROPERTIES',
    headline: <>Every prop, {hi('back in the source')}.</>,
    caption: 'The Properties tab edits every prop of a primitive and a component call, and shows which scenes an edit changes.',
    file: 'film.tsx',
    code: '<Card x={300} y={120}\n  title="ON AIR 0.9.0" tone="tally" />',
    merged: [[525, 'Properties tab'], [516, 'component props'], [508, 'selection impact'], [550, 'required props']],
    meta: 'drawn',
    Demo: PropertiesDemo,
  },
  {
    key: 'pivot, size and text',
    title: 'CANVAS',
    headline: <>Grab it {hi('where it turns')}.</>,
    caption: 'On the canvas, turn about the center or a pivot, size one edge at a time, and edit text where it is drawn.',
    file: 'film.tsx',
    code: '<Card rotation={12}\n  originX={0} originY={120}>ON AIR</Card>',
    merged: [[530, 'pivot and size'], [535, 'inline text']],
    meta: 'drawn',
    Demo: CanvasDemo,
  },
  {
    key: 'one scene or every scene',
    title: 'EDIT SCOPE',
    headline: <>This scene, or {hi('every scene')}.</>,
    caption: 'When a component draws the element in several scenes, choose whether an edit changes this scene or the value they share.',
    file: 'PriceTag.tsx',
    code: 'function PriceTag({ rectWidth = 200 })\n<PriceTag label="Sale" rectWidth={260} />',
    merged: [[555, 'This scene / Shared value']],
    meta: 'drawn',
    Demo: ScopeDemo,
  },
  {
    key: 'timing, dragged',
    title: 'TIMELINE',
    headline: <>Drag the {hi('timing')}.</>,
    caption: "Drag a band's edges or middle, or a scene's end, on the timeline, and from and durationInFrames are written back.",
    file: 'Main.tsx',
    code: '<Sequence from={45}\n  durationInFrames={45}>',
    merged: [[552, 'timing on the timeline']],
    meta: 'drawn',
    Demo: TimelineDemo,
  },
  {
    key: 'draw it in',
    title: 'ADD',
    headline: <>Draw it, and it is {hi('in the code')}.</>,
    caption: 'Choose Rect, Ellipse, Circle, Text, Image or Video from Add, draw it on the canvas, and it is written into the group.',
    file: 'Main.tsx',
    code: "import { Ellipse } from '@celesta/shapes';\n<Ellipse x={340} y={150} width={300}\n  height={170} fill=\"#4F7CFF\" />",
    merged: [[553, 'Add menu']],
    meta: 'drawn',
    Demo: AddDemo,
  },
];
