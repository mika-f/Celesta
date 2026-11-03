import { MediaFrame, PatternBackground } from '@celesta.studio/components';
import { preloadMedia } from '@celesta.studio/media-utils';
import type { MediaInfo } from '@celesta.studio/media-utils';
import { Composition, Easings, Font, Text, Theme, progress, useCurrentFrame } from '@celesta.studio/react';

// <MediaFrame> fits an image or a video into a frame of a given size on a
// <Panel>: `fit="contain"` shows all of it, letterboxed on the background;
// `"cover"` fills the frame, cropped. `kind` says which it is. A <Video>
// draws at its own size, so a video needs its size, read once here with
// preloadMedia(); its startFrom, playbackRate, and sound reach the <Video>
// unchanged.

let footage: MediaInfo;

export async function prepare() {
  footage = await preloadMedia('../../../examples/greenscreen/assets/footage.mp4');
}

const colors = { bg: '#0E1319', surface: '#182030', line: '#2C3A50', ink: '#F4F1EA', dim: '#8A97A8', accent: '#FF4D1F' };
const caption = { fontFamily: 'mono', fontSize: 13, fontWeight: 700, fill: { type: 'solid', color: colors.dim } } as const;

export default function WithMediaFrame() {
  const frame = useCurrentFrame();
  const pop = progress(frame, 0, 24, Easings.easeOutBack);
  const size = { sourceWidth: footage.video!.width, sourceHeight: footage.video!.height };
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={90}>
      <Font google="JetBrains Mono" weights={[700]} />
      <Theme fonts={{ mono: 'JetBrains Mono' }} colors={colors}>
        <PatternBackground background="bg" pattern="dots" spacing={24} size={2} color="ink" opacity={0.08} />
        <Text x={40} y={28} fontFamily="mono" fontSize={14} fontWeight={700} color="dim">16:9 VIDEO IN A 3:4 FRAME</Text>

        {/* The same landscape clip in portrait frames: contain letterboxes, cover crops the sides. */}
        <MediaFrame kind="video" src={footage.src} {...size} x={40} y={60} width={180} height={240}
          fit="contain" fill="surface" stroke="line" strokeWidth={1} cornerRadius={16}
          caption='fit="contain"' captionStyle={caption} />
        <MediaFrame kind="video" src={footage.src} {...size} x={244} y={60} width={180} height={240}
          fit="cover" startFrom={1} cornerRadius={16} caption='"cover" startFrom={1}' captionStyle={caption} />

        {/* A landscape still with a mat of padding; the media's corners follow the frame's. */}
        <MediaFrame kind="image" src="../../../examples/greenscreen/assets/still.jpg" x={472} y={60} width={448} height={252}
          fit="cover" padding={10} fill="accent" cornerRadius={22} shadow={{ color: '#00000099', blur: 24, offsetX: 0, offsetY: 12 }}
          caption="padding={10} cornerRadius={22}: the media's corners are 12" captionStyle={caption} />

        {/* A portrait image contained in a landscape frame: pillarboxed, centred. */}
        <MediaFrame kind="image" src="../../../examples/assets/dialogue-demo/portraits/komugi-normal.png" x={40} y={348} width={384} height={150}
          fit="contain" fill="surface" stroke="line" strokeWidth={1} cornerRadius={12}
          caption="portrait image, fit contain" captionStyle={caption} />

        {/* Anchored at its centre, the frame scales in with its media and its caption. */}
        <MediaFrame kind="video" src={footage.src} {...size} x={696} y={420} anchorX={0.5} anchorY={0.5}
          scale={pop} rotation={-4 * (1 - pop)} width={256} height={144} fit="cover" muted
          stroke="ink" strokeWidth={3} cornerRadius={14} caption="anchor 0.5, muted"
          captionStyle={{ ...caption, align: 'center' }} />
      </Theme>
    </Composition>
  );
}
