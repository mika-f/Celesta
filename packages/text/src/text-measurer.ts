import * as React from 'react';
import type { ReactNode } from 'react';

import type { TextMetrics, TextStyle } from '@celesta.studio/react';
import {
  CompositionRuntimeContext,
  ThemeContext,
  flattenTextContent,
  synchronousMeasurer,
  themeTextContent,
  themedTextStyle,
  useMeasurementFonts,
  withInheritedText,
  withTextRuns,
} from '@celesta.studio/react/internal';

/** Measures `content` with `style`, wrapped at `maxWidth` when given. */
export type TextMeasurer = (content: ReactNode, style: TextStyle, maxWidth?: number) => TextMetrics;

/**
 * A function that measures text during render, with the same fonts,
 * `<Theme>`, and shaping as `useTextMetrics()`, for a component that has to
 * measure a number of texts it only knows while rendering, such as
 * candidate pages of a caption. Each call measures again, so keep its
 * results in `React.useMemo()`.
 */
export function useTextMeasurer(options: { fonts?: string[] } = {}): TextMeasurer {
  const runtime = React.useContext(CompositionRuntimeContext);
  const theme = React.useContext(ThemeContext);
  const fonts = useMeasurementFonts(options.fonts);
  return (content, style, maxWidth) => {
    const flat = flattenTextContent(themeTextContent(content, theme), 'useTextMeasurer() text');
    const themed = themedTextStyle(style, {}, theme, 'text measurement') ?? style;
    const resolved = withTextRuns(withInheritedText(themed, runtime ?? {}), flat, 'useTextMeasurer()');
    return synchronousMeasurer('useTextMeasurer()')({
      text: flat.text,
      style: resolved,
      ...(maxWidth !== undefined ? { maxWidth } : {}),
      fonts,
    });
  };
}
