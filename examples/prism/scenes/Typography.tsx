import { Grid, Group, Rect, Sequence, Text, Transition } from '@celesta/react';
import { Label } from '../components/Label';
import { Rails } from '../components/Rails';
import { BLUE, GREY, H, INK, PAPER, RED, W } from '../constants';

export function Typography() {
  return <>
    <Rect width={W} height={H} fill={PAPER} />
    <Rails chapter="04 / TYPE & LAYOUT" light />
    <Label y={167} size={175} color={INK}>YOUR TYPE. YOUR RULES.</Label>
    <Label y={385} size={23} mono color={INK}>CUSTOM FONTS. OUTLINED TEXT. REUSABLE LAYOUTS.</Label>
    <Group x={80} y={463}>
      <Grid columns={3} columnWidth={560} columnGap={40} rowHeight={420}>
        {[0, 1, 2].map(i => <Sequence key={i} from={i * 8} durationInFrames={150 - i * 8}>
          <Transition type="slide" slideFrom="bottom" distance={100} durationInFrames={24}>
            <Rect width={560} height={420} fill={[INK, BLUE, RED][i]} />
            <Label x={28} y={28} size={18} mono color={i === 2 ? INK : PAPER}>
              {['01 / LOAD A FONT', '02 / STYLE THE TEXT', '03 / COMPOSE A GRID'][i]}</Label>
            {i === 0 && <>
              <Label x={27} y={100} size={156}>Aa / 01</Label>
              <Label x={32} y={278} size={42} mono>Abc. 0123.</Label>
              <Label x={32} y={368} size={16} mono color={GREY}>BEBAS NEUE + IBM PLEX MONO</Label>
            </>}
            {i === 1 && <>
              <Text x={28} y={115} style={{ fontFamily: 'Bebas Neue', fontSize: 159,
                fill: { type: 'solid', color: BLUE },
                stroke: { paint: { type: 'solid', color: PAPER }, width: 2 } }}>MAKE IT</Text>
              <Label x={28} y={251} size={124}>YOUR OWN.</Label>
            </>}
            {i === 2 && <>
              <Group x={32} y={112}><Grid columns={4} columnWidth={110} columnGap={17} rowHeight={117}>
                {Array.from({ length: 8 }, (_, k) => <Rect key={k} width={110} height={100}
                  fill={k % 3 === 0 ? PAPER : INK} cornerRadius={k % 3 === 0 ? 50 : 0} />)}
              </Grid></Group>
              <Label x={32} y={368} size={16} mono color={INK}>GRID / STACK / SAFE AREA / FIT</Label>
            </>}
          </Transition>
        </Sequence>)}
      </Grid>
    </Group>
    <Label y={931} size={22} mono color={INK}>DESIGN ONCE. REUSE THROUGHOUT THE FILM.</Label>
  </>;
}
