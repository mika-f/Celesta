# React audio: animated rates across a clipped head

Issue [#709](https://github.com/mika-f/Celesta/issues/709) concerns audio that
starts before an enclosing sequence's audible window. The source offset now
adds the integral of the authored playback rate over the hidden time to
`startFrom`. A rate ramp from 1 to 2 over two seconds advances 1.25 source
seconds in its first second; a static rate still advances by rate × time.
The remaining keyframes move earlier by the clipped duration, on the same
microsecond clock as preview scene cuts.

React clip collection, preview scene cuts, and browser export use the same
Rust-parity evaluator and integrator. Browser automation remains in the web
package and imports the evaluator through `@celesta.studio/react/internal/animated-number`
so the browser entry does not load the React reconciler. React's existing
media prop validation rejects empty keyframe lists.

The React Sequence regression checks the animated offset, the rebased
keyframes, and a static rate with an explicit `startFrom`. Web's keyframe
tests compare the shared code against the Rust fixture and check browser
automation. Run both with
`pnpm --filter @celesta.studio/react --filter @celesta.studio/web test`
(416 React tests, 154 web tests). The web production build and example
typechecks also passed.
