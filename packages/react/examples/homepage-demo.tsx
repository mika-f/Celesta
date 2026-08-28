// A Mikan reproduction of Remotion's homepage "Interactive Demo"
// (https://github.com/remotion-dev/remotion/blob/ee1ae5ea/packages/promo-pages/src/components/homepage/Demo/Comp.tsx),
// scoped down to what a deterministic frame renderer can reproduce:
// - Visuals and animation only. The original Player lets a viewer drag cards
//   around and click buttons; Mikan has no browser-side interactive Player,
//   so this is a fixed, non-interactive composition instead.
// - Live GitHub-trending/weather data, fetched once via this module's
//   `prepare` export (see below) rather than per frame, in place of the
//   original's `getDataAndProps`.
// - Mikan-native replacements for Remotion-only packages: `<Rect>` (a Mikan
//   addition made for this composition — see HANDOFF.md) draws each card's
//   flat rounded background/border in place of CSS, a plain emoji glyph
//   `<Text>` replaces `@remotion/animated-emoji`'s Lottie animation, and the
//   repo's own voice fixture replaces `@remotion/media`'s reaction sound.

import type { ReactNode } from 'react';

import { Audio, Composition, Group, Rect, Text, interpolate, spring, useCurrentFrame, useVideoConfig } from '@mikan/react';

const WIDTH = 640;
const HEIGHT = 360;
const FPS = 30;
const DURATION_IN_FRAMES = 120;

const PADDING = 20;
const CARD_WIDTH = (WIDTH - PADDING * 3) / 2;
const CARD_HEIGHT = (HEIGHT - PADDING * 3) / 2;

const THEME = {
  background: '#1b1c20',
  card: '#2c2d33',
  cardBorder: '#3a3b42',
  text: '#f5f5f7',
  subtleText: '#9a9ba3',
  accent: '#0b84f3',
} as const;

function cardPosition(index: number): { x: number; y: number } {
  return {
    x: index % 2 === 0 ? PADDING : CARD_WIDTH + PADDING * 2,
    y: index < 2 ? PADDING : CARD_HEIGHT + PADDING * 2,
  };
}

/** Every card enters with the same staggered pop-in the original's release
 * animation used (a damped spring), rather than being draggable. */
function Card({ index, children }: { index: number; children: ReactNode }) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const { x, y } = cardPosition(index);
  const scale = spring({
    frame,
    fps,
    delay: index * 4,
    config: { damping: 14, mass: 0.6 },
  });
  const opacity = interpolate(frame, [index * 4, index * 4 + 8], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });

  return (
    <>
      <Rect
        x={x + CARD_WIDTH / 2}
        y={y + CARD_HEIGHT / 2}
        width={CARD_WIDTH}
        height={CARD_HEIGHT}
        fill={THEME.card}
        stroke={THEME.cardBorder}
        strokeWidth={1}
        cornerRadius={13}
        scale={scale}
        opacity={opacity}
      />
      <Group x={x + CARD_WIDTH / 2} y={y + CARD_HEIGHT / 2} scale={scale} opacity={opacity}>
        {children}
      </Group>
    </>
  );
}

const TRENDING_REPO_COUNT = 3;

// Filled in once by `prepare()` below, before the first frame is rendered —
// see that function for why a plain module-level variable is enough here.
let trendingRepos: string[] = ['mika-f/mikan', 'octocat/hello-world', 'your-org/your-repo'];
let temperatureCelsius = 24;

function TrendingReposCard() {
  return (
    <>
      <Text
        y={-CARD_HEIGHT / 2 + 22}
        style={{ fontFamily: 'sans-serif', fontSize: 14, fontWeight: 700, fill: { type: 'solid', color: THEME.accent }, align: 'center' }}
      >
        Trending on GitHub
      </Text>
      {trendingRepos.map((repo, index) => (
        <Text
          key={repo}
          y={-CARD_HEIGHT / 2 + 52 + index * 22}
          style={{ fontFamily: 'sans-serif', fontSize: 13, fill: { type: 'solid', color: THEME.text }, align: 'center' }}
        >
          {repo}
        </Text>
      ))}
    </>
  );
}

function TemperatureCard() {
  const frame = useCurrentFrame();
  // Counts up to the fetched temperature over the card's first second,
  // echoing the original's `TemperatureNumber` digit-wheel reveal.
  const celsius = Math.round(
    interpolate(frame, [0, 30], [0, temperatureCelsius], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }),
  );

  return (
    <>
      <Text
        y={-CARD_HEIGHT / 2 + 22}
        style={{ fontFamily: 'sans-serif', fontSize: 14, fontWeight: 700, fill: { type: 'solid', color: THEME.accent }, align: 'center' }}
      >
        Weather
      </Text>
      <Text
        y={-6}
        style={{ fontFamily: 'sans-serif', fontSize: 40, fontWeight: 700, fill: { type: 'solid', color: THEME.text }, align: 'center' }}
      >
        {`${celsius}°C`}
      </Text>
      <Text
        y={34}
        style={{ fontFamily: 'sans-serif', fontSize: 13, fill: { type: 'solid', color: THEME.subtleText }, align: 'center' }}
      >
        Tokyo, Japan
      </Text>
    </>
  );
}

function CurrentCountryCard() {
  return (
    <>
      <Text
        y={-CARD_HEIGHT / 2 + 22}
        style={{ fontFamily: 'sans-serif', fontSize: 14, fontWeight: 700, fill: { type: 'solid', color: THEME.accent }, align: 'center' }}
      >
        Current Location
      </Text>
      <Text y={4} style={{ fontFamily: 'sans-serif', fontSize: 44, align: 'center' }}>
        🇯🇵
      </Text>
      <Text
        y={44}
        style={{ fontFamily: 'sans-serif', fontSize: 13, fill: { type: 'solid', color: THEME.text }, align: 'center' }}
      >
        Japan
      </Text>
    </>
  );
}

// Stands in for `@remotion/animated-emoji`'s Lottie playback: a plain emoji
// glyph, cycled every 40 frames, with a small spring pop on each swap.
const EMOJIS = ['🥳', '🔥', '🥲'];

function EmojiCard() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const stepLength = 40;
  const step = Math.floor(frame / stepLength) % EMOJIS.length;
  const frameWithinStep = frame % stepLength;
  const pop = spring({ frame: frameWithinStep, fps, config: { damping: 10, mass: 0.5 } });

  return (
    <>
      <Text
        y={-CARD_HEIGHT / 2 + 22}
        style={{ fontFamily: 'sans-serif', fontSize: 13, fontWeight: 500, fill: { type: 'solid', color: THEME.accent }, align: 'center' }}
      >
        Choose an emoji
      </Text>
      <Text y={4} scale={0.4 + pop * 0.6} style={{ fontFamily: 'sans-serif', fontSize: 64, align: 'center' }}>
        {EMOJIS[step]}
      </Text>
    </>
  );
}

interface GitHubSearchResponse {
  items: { full_name: string }[];
}

interface OpenMeteoResponse {
  current: { temperature_2m: number };
}

// Tokyo — matches `CurrentCountryCard`'s fixed 🇯🇵/"Japan" below, so the
// weather card's location and the location card agree without a second,
// unrelated network round trip just to resolve "where is this render for".
const WEATHER_LATITUDE = 35.6762;
const WEATHER_LONGITUDE = 139.6503;

/**
 * `mikan-react-render` (`packages/react/src/cli.ts`) awaits this exact export
 * name exactly once, before mounting the composition and before the first
 * `renderAt()` — see the comment at its call site. That is the one place in
 * the render pipeline async work is allowed: `renderAt()` itself, called once
 * per requested frame, is fully synchronous end-to-end (Rust blocks on a
 * single-line reply). So data fetched here is fetched once for the whole
 * export/preview session, cached in the module-level `trendingRepos` /
 * `temperatureCelsius` above, and then just read synchronously by every
 * frame's render — never re-fetched per frame.
 *
 * Network failures fall back to the mock values those variables start out
 * with (logged, not thrown) so the example still renders deterministically
 * when offline, e.g. in a sandboxed CI runner.
 */
export async function prepare(): Promise<void> {
  const since = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
  const results = await Promise.allSettled([
    fetch(
      `https://api.github.com/search/repositories?q=created:>${since}&sort=stars&order=desc&per_page=${TRENDING_REPO_COUNT}`,
    ).then((response) => response.json() as Promise<GitHubSearchResponse>),
    fetch(
      `https://api.open-meteo.com/v1/forecast?latitude=${WEATHER_LATITUDE}&longitude=${WEATHER_LONGITUDE}&current=temperature_2m`,
    ).then((response) => response.json() as Promise<OpenMeteoResponse>),
  ]);

  const [repos, weather] = results;
  if (repos.status === 'fulfilled') {
    trendingRepos = repos.value.items.slice(0, TRENDING_REPO_COUNT).map((item) => item.full_name);
  } else {
    console.error('homepage-demo: failed to fetch trending repos, using fallback data', repos.reason);
  }
  if (weather.status === 'fulfilled') {
    temperatureCelsius = Math.round(weather.value.current.temperature_2m);
  } else {
    console.error('homepage-demo: failed to fetch weather, using fallback data', weather.reason);
  }
}

export default function HomepageDemo() {
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={DURATION_IN_FRAMES}>
      <Rect x={WIDTH / 2} y={HEIGHT / 2} width={WIDTH} height={HEIGHT} fill={THEME.background} />
      <Card index={0}>
        <TrendingReposCard />
      </Card>
      <Card index={1}>
        <TemperatureCard />
      </Card>
      <Card index={2}>
        <CurrentCountryCard />
      </Card>
      <Card index={3}>
        <EmojiCard />
      </Card>
      <Audio src="../../../examples/assets/voices/001.wav" volume={0.6} />
    </Composition>
  );
}
