import { codeThemes } from '@celesta.studio/code';
import type { CodeTheme } from '@celesta.studio/code';
import { C } from './theme';

// codeThemes.dark keeps every token close to grey; on a video frame, seen
// for a few seconds, each kind of token needs its own hue. Keywords take the
// film's orange, names and tags its blue, strings green, numbers violet,
// and the punctuation steps back.
export const codeTheme: CodeTheme = {
  ...codeThemes.dark,
  foreground: C.paper,
  tokens: {
    ...codeThemes.dark.tokens,
    keyword: '#FF7A45', builtin: '#FF7A45',
    function: '#FFC861',
    tag_name: '#5AB2FF', type: '#5AB2FF', namespace: '#5AB2FF',
    string: '#7EE08E', template: '#7EE08E', regex: '#7EE08E',
    number: '#B9A2FF', constant: '#B9A2FF', boolean: '#B9A2FF',
    attr_name: '#9FD8F2', property: '#9FD8F2', parameter: '#F2B8D0',
    punctuation: '#7A7E88', operator: '#7A7E88', comment: '#6A6E78',
  },
};
