import { Assets, Audio, Composition, Font } from '@celesta/react';
import { TransitionSeries } from '@celesta/transitions';
import { Background, SafeGuides } from './components/Frame';
import { Header } from './components/Header';
import {
  BUILD_LEN, DURATION, FADE, FONT_URLS, FPS, H, HOOK_LEN, OUTRO_LEN, W, WIPE,
} from './constants';
import { DEMOS } from './demos';
import { useTip } from './properties';
import { Build } from './scenes/Build';
import { Hook } from './scenes/Hook';
import { Outro } from './scenes/Outro';

// "Celesta Tips." A vertical short, one Tip per video: the code on top, what
// it draws running below. 1080×1920, 30 fps, 24 s on a 120 BPM score (one
// beat = 15 frames, one bar = 60 frames).
//
//   bar 0      the finished result and a hook line, from the first frame
//   bars 1–7   the snippet types in line by line; the demo builds up with it
//   bars 8–11  the closing line over the running demo
//
// One source, many videos: properties.ts declares the inputs and each file in
// variants/ fills them for one Tip.
//   celesta-exporter --react examples/shorts-tips/film.tsx spring.mp4 \
//     --props-file examples/shorts-tips/variants/spring.json
//
//   properties.ts  the template's inputs        plan.ts      code layout, reveal times, prepare()
//   demos/         one live demo per Tip ID     scenes/      Hook, Build, Outro
//   components/    Header, CodePanel, the demo clock, background and safe-area guides

// Checks the variant's code and stage lines before the first frame.
export { prepare } from './plan';

export default function Root() {
  const { tip, number, title, hook, closing, accent, guides } = useTip();
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={DURATION} lang="ja-JP">
      <Assets>
        {FONT_URLS.map((src) => <Font key={src} src={src} />)}
      </Assets>
      <Background accent={accent} />
      <TransitionSeries>
        <TransitionSeries.Sequence durationInFrames={HOOK_LEN}>
          <Hook tip={tip} hook={hook} accent={accent} stages={DEMOS[tip].stages} />
        </TransitionSeries.Sequence>
        <TransitionSeries.Transition type="wipe" from="left" durationInFrames={WIPE} />
        <TransitionSeries.Sequence durationInFrames={BUILD_LEN}>
          <Build tip={tip} accent={accent} />
        </TransitionSeries.Sequence>
        <TransitionSeries.Transition type="crossfade" durationInFrames={FADE} />
        <TransitionSeries.Sequence durationInFrames={OUTRO_LEN}>
          <Outro tip={tip} closing={closing} number={number} accent={accent} />
        </TransitionSeries.Sequence>
      </TransitionSeries>
      <Header number={number} title={title} accent={accent} />
      {guides && <SafeGuides />}
      <Audio src="./score.wav" />
    </Composition>
  );
}
