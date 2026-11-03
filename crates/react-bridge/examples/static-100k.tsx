// 100,000 still rects in one <Static> and 100 moving ones. Run with
// `cargo run --release -p celesta-react-bridge --example protocol-bench --
// crates/react-bridge/examples/static-100k.tsx 120`.
import { Static } from '@celesta.studio/react';

import { Film, InStatic } from './static-100k-scene';

export default function Root() {
  return <Film><InStatic Static={Static} /></Film>;
}
