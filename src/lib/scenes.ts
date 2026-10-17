// Placeholder artwork for demo thumbnails: simple landscapes, no network.

const palettes: [string, string, string, string][] = [
  ["#a5c7d4", "#e8e4ca", "#5d7d55", "#3e5a3b"],
  ["#c3c5b3", "#ece0be", "#7a8a62", "#4f5f42"],
  ["#b9ccd0", "#f0dcb8", "#9c8d7a", "#6b5d50"],
  ["#d8d4c5", "#f0d5ad", "#8b8577", "#5a554c"],
  ["#91c0cf", "#e4d3a9", "#4f8aa0", "#2f6178"],
  ["#d6d6cf", "#efeae0", "#b4b2a6", "#8d8a7f"],
  ["#b2c2b9", "#ebd8b4", "#8a7f6c", "#5e5547"],
  ["#b1c0a5", "#ead9b2", "#58714b", "#3a5032"],
];

export function sceneSvg(index: number): string {
  const [skyTop, skyBottom, hill, front] = palettes[index % palettes.length];
  const id = `g${index}`;
  return `<svg viewBox="0 0 360 230" xmlns="http://www.w3.org/2000/svg" preserveAspectRatio="xMidYMid slice" aria-hidden="true">
<defs><linearGradient id="${id}" x2="0" y2="1"><stop stop-color="${skyTop}"/><stop offset="1" stop-color="${skyBottom}"/></linearGradient></defs>
<rect width="360" height="230" fill="url(#${id})"/>
<circle cx="${70 + (index * 37) % 220}" cy="58" r="17" fill="#fff6dc" opacity=".8"/>
<path d="M0 150 L70 ${95 + (index * 13) % 30} L140 140 L220 ${80 + (index * 7) % 35} L300 135 L360 110 V230 H0 Z" fill="${hill}" opacity=".85"/>
<path d="M0 190 Q90 160 180 185 T360 175 V230 H0 Z" fill="${front}"/>
</svg>`;
}
