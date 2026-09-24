export const palettes = [
  { name: 'Lilac dream', label: 'ライラック', light: '#e2d6fa', mid: '#b5a0e2', dark: '#8870c2', ink: '#a18acc', wash: '#eee8f7' },
  { name: 'Peach sorbet', label: 'ピーチ', light: '#ffe1ce', mid: '#f4b6a1', dark: '#d9867c', ink: '#d78f86', wash: '#faebe3' },
  { name: 'Mint daydream', label: 'ミント', light: '#d5eee2', mid: '#9dcebd', dark: '#619f93', ink: '#77af9b', wash: '#e6f2ec' },
  { name: 'Blue hour', label: 'ブルー', light: '#d5e5fb', mid: '#9cbae9', dark: '#6e91c9', ink: '#85a5d7', wash: '#e6edf7' },
];
export const concepts = [
  { name: 'Starlight', subtitle: 'ひとつの星から、物語がはじまる。', description: 'やわらかな星に、小さな再生ボタン。\nつくる時間を、ちょっとときめく時間に。' },
  { name: 'Moon ribbon', subtitle: 'ひらめきを、月のリボンにのせて。', description: '三日月をくるりと巻いた、Celesta の C。\n夜の創作にそっと寄り添うアイコン。' },
  { name: 'Little bloom', subtitle: '小さなひらめきが、ぱっと咲く。', description: 'ふっくらとした花びらに、再生のしるし。\nあなたの物語が花ひらく、その瞬間を。' },
];

export function iconSvg(concept = 0, palette = 0, tile = true, id = "celesta-icon"): string {
  const p = palettes[palette];
  const play = `<path d="M119 105Q114 102 114 109V150Q114 157 120 153L153 134Q159 130 153 126Z" fill="${p.ink}"/>`;
  const shape = concept === 0
    ? `<path d="M121 58Q127 39 134 59L149 93Q152 100 161 103L195 116Q215 123 195 132L163 146Q156 149 153 157L138 194Q131 213 123 193L109 160Q105 150 97 147L63 134Q42 126 63 118L96 103Q105 100 108 91Z" fill="url(#${id}-cream)"/>${play}`
    : concept === 1
      ? `<path d="M176 69C143 41 91 52 70 90C46 134 69 188 113 200C143 209 176 197 192 174C154 189 113 166 108 130C104 103 121 78 148 74Q161 71 176 69Z" fill="url(#${id}-cream)"/><path d="M150 103Q145 100 145 107V146Q145 153 151 150L181 132Q187 128 181 124Z" fill="#fff8e7"/>`
      : `<path d="M128 69C148 35 185 57 178 91C216 87 229 126 198 146C226 173 200 208 168 192C161 228 117 227 108 194C74 213 45 179 69 152C34 133 50 91 84 94C73 60 109 39 128 69Z" fill="url(#${id}-cream)"/>${play}`;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256" fill="none" role="img" aria-label="Celesta ${concepts[concept].name}">
    <defs>
      <linearGradient id="${id}-tile" x1="38" y1="16" x2="205" y2="250" gradientUnits="userSpaceOnUse"><stop stop-color="${p.light}"/><stop offset=".55" stop-color="${p.mid}"/><stop offset="1" stop-color="${p.dark}"/></linearGradient>
      <linearGradient id="${id}-cream" x1="97" y1="62" x2="158" y2="199" gradientUnits="userSpaceOnUse"><stop stop-color="#fffef4"/><stop offset="1" stop-color="#ffedcb"/></linearGradient>
      <linearGradient id="${id}-edge" x2="0" y2="1"><stop stop-color="#fff" stop-opacity=".8"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>
      <filter id="${id}-shadow" x="-50%" y="-50%" width="200%" height="210%"><feDropShadow dx="0" dy="8" stdDeviation="6" flood-color="${p.dark}" flood-opacity=".4"/></filter>
    </defs>
    ${tile ? `<rect x="12" y="12" width="232" height="232" rx="60" fill="url(#${id}-tile)"/><rect x="15" y="15" width="226" height="226" rx="57" stroke="url(#${id}-edge)" stroke-width="2"/><path d="M38 76Q38 38 78 36" stroke="white" stroke-opacity=".28" stroke-width="5" stroke-linecap="round"/>` : ''}
    <g filter="url(#${id}-shadow)">${shape}</g>
    <path d="M192 48Q193 61 205 64Q193 66 191 79Q190 66 179 64Q190 61 192 48Z" fill="#fff9e9"/>
    <circle cx="64" cy="187" r="4" fill="#fff9e9" opacity=".85"/>
  </svg>`;
}
