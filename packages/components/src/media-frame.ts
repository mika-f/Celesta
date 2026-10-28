// An image or a video fitted into a frame of a given size, on a <Panel>,
// with an optional caption under it.

import * as React from 'react';
import type { ReactNode } from 'react';

import type { Insets } from '@celesta/layout';
import { Group, Image, Text, Video, useTextMetrics } from '@celesta/react';
import type { AnimatedNumber, AssetInput, TextStyle } from '@celesta/react';

import { Panel } from './panel';
import type { PanelProps } from './panel';
import { validators } from './validate';

/** How the media fills the frame: `'contain'` shows all of it, letterboxed; `'cover'` fills the frame, cropped. */
export type MediaFit = 'contain' | 'cover';

/** The frame's decoration. Only `'plain'` (the panel alone) for now. */
export type MediaFrameVariant = 'plain';

interface MediaFrameCommonProps extends Omit<PanelProps, 'children' | 'clip'> {
  /** A path, URL, or asset reference, as `<Image>` and `<Video>` take it. */
  src: AssetInput;
  name?: string;
  /** Defaults to `'contain'`. */
  fit?: MediaFit;
  /** Text under the frame, wrapped to its width; `style.align` in `captionStyle` aligns it. Omit for none. */
  caption?: ReactNode;
  /** Defaults to `{ fontSize: 20 }`. */
  captionStyle?: TextStyle;
  /** Space between the frame's bottom edge and the caption's line box. Defaults to 12. */
  captionGap?: number;
  /** Defaults to `'plain'`. */
  variant?: MediaFrameVariant;
}

export interface MediaFrameImageProps extends MediaFrameCommonProps {
  kind: 'image';
  /** As on `<Image>`, for an animated GIF, APNG, or WebP. */
  startFrom?: number;
  playbackRate?: number;
  loop?: boolean;
}

export interface MediaFrameVideoProps extends MediaFrameCommonProps {
  kind: 'video';
  /**
   * The video's size in pixels, which the frame scales it from, as
   * `<Video>` has no `fit`: `preloadMedia(src)` in `prepare()` gives it as
   * `video.width` and `video.height`.
   */
  sourceWidth: number;
  sourceHeight: number;
  /** As on `<Video>`; passed through unchanged, so the picture and the sound play as a `<Video>` would. */
  startFrom?: number;
  playbackRate?: number;
  volume?: AnimatedNumber;
  muted?: boolean;
  loop?: boolean;
}

/** `kind` says which the media is; it is never guessed from the file name. */
export type MediaFrameProps = MediaFrameImageProps | MediaFrameVideoProps;

const DEFAULT_CAPTION_STYLE: TextStyle = { fontSize: 20 };

const { fail, nonNegative, insets } = validators('MediaFrame');

/** `props` without the keys whose value is undefined, so passed-through props match what was given. */
function defined<T extends object>(props: T): Partial<T> {
  return Object.fromEntries(Object.entries(props).filter(([, value]) => value !== undefined)) as Partial<T>;
}

/**
 * An image or a video fitted into a `width`×`height` frame drawn by
 * `<Panel>`, which gives it a background, a border over the media, rounded
 * corners, a shadow, and padding (a mat around the media). `kind` says
 * which the media is. With `fit="contain"` (the default) all of it shows,
 * centred, and the background fills the rest; with `"cover"` it fills the
 * area inside the padding and is cropped, centred. The media is clipped to
 * that area, its corners rounded by `cornerRadius` less the narrowest
 * padding edge.
 *
 * An image is an `<Image>` given the area and `fit`, so it fits whatever
 * its size. A `<Video>` has no `fit`, so a video is scaled to fit and needs
 * `sourceWidth` and `sourceHeight`, read once with `preloadMedia()` in
 * `prepare()`. Its `startFrom`, `playbackRate`, `volume`, `muted`, and
 * `loop` reach the `<Video>` unchanged: it starts with the enclosing
 * `<Sequence>` and its soundtrack is collected just as a bare `<Video>`'s.
 *
 * `caption` draws text under the frame, `captionGap` below it, wrapped to
 * the frame's width. The anchor and the transforms apply to the frame, the
 * media, and the caption together.
 *
 * ```tsx
 * let clip: MediaInfo;
 * export async function prepare() { clip = await preloadMedia('./demo.mp4'); }
 *
 * <MediaFrame kind="video" src={clip.src} sourceWidth={clip.video!.width} sourceHeight={clip.video!.height}
 *   width={960} height={540} fit="cover" cornerRadius={24} caption="アプリのプレビュー" />
 * ```
 */
export function MediaFrame(props: MediaFrameProps): ReturnType<typeof React.createElement> {
  const {
    kind, src, name, fit = 'contain', caption, captionStyle = DEFAULT_CAPTION_STYLE, captionGap = 12, variant = 'plain',
    width, height, padding = 0, cornerRadius = 0, ...rest
  } = props;
  // The media's own props, by kind; the rest are the panel's.
  const {
    sourceWidth, sourceHeight, startFrom, playbackRate, volume, muted, loop, ...panel
  } = rest as Partial<Omit<MediaFrameVideoProps, keyof MediaFrameCommonProps | 'kind'>> & Omit<PanelProps, 'children' | 'clip'>;
  if (kind !== 'image' && kind !== 'video') fail(`kind must be 'image' or 'video', not ${JSON.stringify(kind)}`);
  if (fit !== 'contain' && fit !== 'cover') fail(`fit must be 'contain' or 'cover', not ${JSON.stringify(fit)}`);
  switch (variant) {
    case 'plain':
      break;
    default:
      fail(`variant must be 'plain', not ${JSON.stringify(variant)}`);
  }
  nonNegative('captionGap', captionGap);
  nonNegative('cornerRadius', cornerRadius);
  const { top, right, bottom, left } = insets(padding);
  const area = { width: Math.max(0, width - left - right), height: Math.max(0, height - top - bottom) };
  const radius = Math.max(0, cornerRadius - Math.min(top, right, bottom, left));

  let media: ReturnType<typeof React.createElement>;
  if (kind === 'image') {
    media = React.createElement(Image, defined({
      src, name, width: area.width, height: area.height, fit, startFrom, playbackRate, loop,
    }) as React.ComponentProps<typeof Image>);
  } else {
    for (const [prop, value] of [['sourceWidth', sourceWidth], ['sourceHeight', sourceHeight]] as const) {
      if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
        fail(`kind="video" needs a positive ${prop}, the video's size: read it with preloadMedia() in prepare(); got ${value}`);
      }
    }
    const scale = (fit === 'contain' ? Math.min : Math.max)(area.width / sourceWidth!, area.height / sourceHeight!);
    media = React.createElement(Video, defined({
      src, name, startFrom, playbackRate, volume, muted, loop,
      x: (area.width - sourceWidth! * scale) / 2,
      y: (area.height - sourceHeight! * scale) / 2,
      scale,
    }) as React.ComponentProps<typeof Video>);
  }

  const metrics = useTextMetrics(caption ?? '', captionStyle, { maxWidth: width });
  return React.createElement(Panel, { ...panel, width, height, padding: { top, right, bottom, left }, cornerRadius },
    React.createElement(Group, { key: 'media', clip: { x: 0, y: 0, width: area.width, height: area.height, cornerRadius: radius } }, media),
    caption == null ? null : React.createElement(Text, {
      key: 'caption',
      x: -left,
      y: height - top + captionGap + metrics.ascent,
      anchorY: 'baseline',
      maxWidth: width,
      style: captionStyle,
      children: caption,
    }));
}
