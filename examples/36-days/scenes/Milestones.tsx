import { Camera, Easings, Group, Rect, TextReveal, interpolate, progress, useCue, useCurrentFrame } from '@celesta/react';
import { Exit } from '../components/Exit';
import { Header } from '../components/Header';
import { Label, textStyle } from '../components/Label';
import { BAR, C, H, W } from '../constants';
import { DAYS } from '../data';

const MILESTONES = [
  { day: 0, en: 'FIRST COMMIT', ja: '合成モデル、評価器、GPUI エディタ、MP4 書き出し。\n初日だけで 25 コミット。' },
  { day: 1, en: '<SEQUENCE>', ja: 'React で時間を組む。\n<Sequence> と <Audio> が入る。' },
  { day: 4, en: 'LIP SYNC', ja: '5 母音の口パクと PSD 立ち絵。\nキャラクターがしゃべり出す。' },
  { day: 13, en: 'GPUI-KIT', ja: 'エディタを gpui-kit で作り直す。\nこの日だけで 21 コミット。' },
  { day: 31, en: 'CELESTA', ja: 'Frameweave から Celesta へ。\nWeb サイトと配布パッケージ。' },
  { day: 34, en: 'FILMS', ja: 'Reel、Afterimage、Signal、Feature Tour。\n作品を作って確かめる。' },
].map((m, i) => ({ ...m, at: 30 + i * 45 }));
const SPACING = 1000;
// Days are unevenly spaced so that every milestone gets the same room.
const worldX = (day: number) =>
  interpolate(day, [...MILESTONES.map((m) => m.day), 35], [...MILESTONES.map((_, i) => i * SPACING), 5 * SPACING + 300]);
const RAIL = 560;

export function Milestones() {
  const f = useCurrentFrame();
  const stop = useCue(MILESTONES);
  const from = stop?.previous ? worldX(stop.previous.day) : -900;
  const to = stop ? worldX(stop.cue.day) : -900;
  const move = stop ? progress(stop.frame, 0, 24, Easings.easeInOutCubic) : 1;

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Exit>
        <Header en="MILESTONES" ja="36 日間の道のり" />
        <Label x={W - 160} y={150} size={22} font="mono" color={C.grey} ax={1} ay={0.5}>
          {`0${(stop?.index ?? 0) + 1} / 0${MILESTONES.length}`}
        </Label>
        <Camera x={from + (to - from) * move + 420} y={RAIL} zoom={1 - 0.06 * Math.sin(Math.PI * move)} shake={4}>
          <Rect x={-2000} y={RAIL - 1} width={5 * SPACING + 4400} height={2} fill={C.line} />
          {DAYS.map((d, i) => (
            <Group key={d.date} x={worldX(i)} y={RAIL}>
              <Rect x={-1} y={-8} width={2} height={16} fill={C.grey} opacity={0.6} />
              {d.commits > 0 && (
                <Rect y={-40} anchorX={0.5} anchorY={0.5} width={4 + d.commits * 1.2} height={4 + d.commits * 1.2}
                  cornerRadius={20} fill={C.mint} opacity={0.35} />
              )}
            </Group>
          ))}
          {MILESTONES.map((m, i) => {
            const active = i === stop?.index;
            return (
              <Group key={m.en} x={worldX(m.day)} y={RAIL}>
                <Rect anchorX={0.5} anchorY={0.5} width={26} height={26} cornerRadius={13}
                  fill={active ? C.mint : C.ink} stroke={active ? undefined : C.grey} strokeWidth={active ? undefined : 3} />
                <Label y={-100} size={24} font="mono" weight={700} color={active ? C.mint : C.grey} ay={0.5}>
                  {DAYS[m.day].date}
                </Label>
                <TextReveal y={50} lineHeight={90} baseline={0.84} from={active ? m.at + 10 : -BAR}
                  style={textStyle('display', 80, active ? C.paper : C.dim)}>
                  {m.en}
                </TextReveal>
                <Label y={170} size={28} font="ja" weight={500} lineHeight={46} color={C.soft}
                  opacity={active ? progress(f, m.at + 16, 14) : 0}>{m.ja}</Label>
              </Group>
            );
          })}
        </Camera>
      </Exit>
    </>
  );
}
