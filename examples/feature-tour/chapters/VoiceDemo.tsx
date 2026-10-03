import type { LipSyncTrack } from '@celesta/react';
import { CharacterView, Dialogue, Easings, Group, Rect, Sequence, spring, useCurrentFrame, useLipSync, useVideoConfig } from '@celesta/react';
import { Label } from '../components/Label';
import { C, DW, DX } from '../constants';
import { clamp, progress } from '../helpers';
import { VOICE, VOICE_AT, VOICE_TEXT, akane, akaneView, lipSync, voiceBars, voiceSeconds } from '../voice';

// ── 06 · Voice: a PSD portrait, a subtitle, lip sync from the recording ───

export const VOWEL_KANA = { a: 'あ', i: 'い', u: 'う', e: 'え', o: 'お', closed: 'ん' } as const;

export function VoiceDemo() {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const cx = 1400;
  const cy = 480;
  const disc = spring({ frame: f - 2, fps, config: { damping: 14, stiffness: 120 } });
  const portraitIn = progress(f, 8, 16, Easings.easeOutExpo);
  const voiceFrames = Math.ceil(voiceSeconds * fps);
  const t = clamp((f - VOICE_AT) / voiceFrames);
  const wave = progress(f, 14, 14, Easings.easeOutExpo);
  const barsW = DW;
  const bw = barsW / voiceBars.length;
  return (
    <>
      <Rect x={cx} y={cy} anchorX={0.5} anchorY={0.5} width={500 * disc} height={500 * disc}
        cornerRadius={250 * disc} fill={C.blue} />
      <Rect x={cx} y={cy} anchorX={0.5} anchorY={0.5} width={580 * disc} height={580 * disc}
        cornerRadius={290 * disc} stroke={C.line} strokeWidth={1} />
      <Group opacity={portraitIn} y={30 * (1 - portraitIn)}>
        <CharacterView ref={akaneView} character={akane} x={cx} y={cy + 40} anchorX={0.5} anchorY={0.5} scale={0.2} />
      </Group>
      <Sequence from={VOICE_AT} durationInFrames={voiceFrames + 40}>
        <Dialogue character={akaneView} audio={VOICE} lipSync={lipSync ?? undefined}>{VOICE_TEXT}</Dialogue>
        {lipSync ? <Mouth track={lipSync} /> : <MouthLabel shape="closed" opacity={1} />}
      </Sequence>
      {f < VOICE_AT && <MouthLabel shape="closed" opacity={progress(f, 12, 10)} />}
      <Group opacity={wave}>
        {voiceBars.map((v, i) => {
          const played = i / voiceBars.length < t;
          const h = 4 + 70 * v * wave;
          return (
            <Rect key={i} x={DX + i * bw + bw / 2} y={800} anchorX={0.5} anchorY={0.5} width={Math.max(2, bw - 3)}
              height={h} cornerRadius={1} fill={played ? C.blue : C.dim} />
          );
        })}
        {f >= VOICE_AT && t < 1 && <Rect x={DX + barsW * t} y={750} width={2} height={100} fill={C.paper} />}
        <Label x={DX} y={728} size={15} font="mono" weight={400} color={C.grey} ay={0.5}>character-lipsync-demo.wav</Label>
        <Label x={DX + DW} y={728} size={15} font="mono" weight={400} color={C.grey} ax={1} ay={0.5}>
          {`${(t * voiceSeconds).toFixed(2)} / ${voiceSeconds.toFixed(2)} s`}
        </Label>
      </Group>
    </>
  );
}

function Mouth({ track }: { track: LipSyncTrack }) {
  return <MouthLabel shape={useLipSync(track)} opacity={1} />;
}

function MouthLabel({ shape, opacity }: { shape: keyof typeof VOWEL_KANA; opacity: number }) {
  return (
    <Group opacity={opacity}>
      <Label x={1760} y={330} size={150} font="jaDisplay" weight={400} ax={0.5} ay={0.5}
        color={shape === 'closed' ? C.ink : C.paper} stroke={shape === 'closed' ? { color: C.paper, width: 3 } : undefined}>
        {VOWEL_KANA[shape]}
      </Label>
      <Label x={1760} y={450} size={16} font="mono" weight={700} color={C.grey} ax={0.5} ay={0.5}>
        {`mouth: ${shape}`}
      </Label>
    </Group>
  );
}
