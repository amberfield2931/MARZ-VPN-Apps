// Country flags as local drawings. Windows ships no flag emoji, so WebView2 showed "US" and "SE" as plain letters; a
// drawing looks the same on every system and needs nothing from the network. Simplified on purpose: at this size an
// emblem would be a blot. Own work, same licence as the app.
const W = 28, H = 20;
const hBands = (...c) => c.map((f, i) => `<rect y="${(H / c.length * i).toFixed(3)}" width="${W}" height="${(H / c.length + 0.05).toFixed(3)}" fill="${f}"/>`).join('');
const vBands = (...c) => c.map((f, i) => `<rect x="${(W / c.length * i).toFixed(3)}" width="${(W / c.length + 0.05).toFixed(3)}" height="${H}" fill="${f}"/>`).join('');
const nordic = (bg, cross, inner) => `<rect width="${W}" height="${H}" fill="${bg}"/><path d="M0 10h28M10 0v20" stroke="${cross}" stroke-width="${inner ? 5 : 4}"/>${inner ? `<path d="M0 10h28M10 0v20" stroke="${inner}" stroke-width="2.4"/>` : ''}`;
const disc = (cx, cy, r, f) => `<circle cx="${cx}" cy="${cy}" r="${r}" fill="${f}"/>`;
const stars = (points, f, r = 0.55) => points.map(([x, y]) => disc(x, y, r, f)).join('');
const union = `<rect width="${W}" height="${H}" fill="#012169"/><path d="M0 0l28 20M28 0L0 20" stroke="#fff" stroke-width="4"/><path d="M0 0l28 20M28 0L0 20" stroke="#C8102E" stroke-width="1.6"/><path d="M14 0v20M0 10h28" stroke="#fff" stroke-width="6.6"/><path d="M14 0v20M0 10h28" stroke="#C8102E" stroke-width="4"/>`;
const usStripes = Array.from({ length: 13 }, (_, i) => `<rect y="${(i * 20 / 13).toFixed(3)}" width="${W}" height="${(20 / 13 + 0.03).toFixed(3)}" fill="${i % 2 ? '#fff' : '#B22234'}"/>`).join('');
const usStars = [];
for (let y = 0; y < 5; y++) for (let x = 0; x < (y % 2 ? 5 : 6); x++) usStars.push([1.3 + (y % 2 ? 1 : 0) + x * 2, 1.3 + y * 1.95]);

const ART = {
  AE: `<rect width="${W}" height="${H}" fill="#fff"/><rect width="${W}" height="6.7" fill="#00843D"/><rect y="13.3" width="${W}" height="6.7" fill="#000"/><rect width="7" height="${H}" fill="#EF3340"/>`,
  AM: hBands('#D90012', '#0033A0', '#F2A800'),
  AT: hBands('#ED2939', '#fff', '#ED2939'),
  AU: `<rect width="${W}" height="${H}" fill="#012169"/><g transform="scale(.5)">${union}</g>${stars([[7, 15.5], [21, 3.5], [24.5, 8], [21, 16.5], [18, 9]], '#fff', 0.9)}`,
  AZ: hBands('#00B5E2', '#EF3340', '#509E2F') + disc(13.4, 10, 2.6, '#fff') + disc(14.2, 10, 2.1, '#EF3340'),
  BE: vBands('#000', '#FDDA24', '#EF3340'),
  BG: hBands('#fff', '#00966E', '#D62612'),
  BR: `<rect width="${W}" height="${H}" fill="#009739"/><path d="M14 2.5L25.5 10 14 17.5 2.5 10z" fill="#FEDD00"/>${disc(14, 10, 4.2, '#012169')}`,
  BY: `<rect width="${W}" height="${H}" fill="#CE1720"/><rect y="13.3" width="${W}" height="6.7" fill="#007C30"/><rect width="4" height="${H}" fill="#fff"/>`,
  CA: `${vBands('#D80621', '#fff', '#fff', '#D80621')}<path d="M14 4l1.3 2.6 1.5-.7-.6 3.5 2-1.8.5 1.4 2-.3-.8 2.3.9.6-3.3 2.6.4 1.3-3.2-.5.1 2.9h-.6l.1-2.9-3.2.5.4-1.3-3.3-2.6.9-.6-.8-2.3 2 .3.5-1.4 2 1.8-.6-3.5 1.5.7z" fill="#D80621"/>`,
  CH: `<rect width="${W}" height="${H}" fill="#D52B1E"/><path d="M14 4.5v11M8.5 10h11" stroke="#fff" stroke-width="3.2"/>`,
  CY: `<rect width="${W}" height="${H}" fill="#fff"/><path d="M8 9.5c2-2.5 8-3.5 12-2.5-1 3-3 5-6 5.2-2.5.2-4.5-.7-6-2.7z" fill="#D57800"/><path d="M10 14.5c2 1.3 6 1.3 8 0" stroke="#4E5B31" stroke-width="1" fill="none"/>`,
  CZ: `${hBands('#fff', '#D7141A')}<path d="M0 0l12 10L0 20z" fill="#11457E"/>`,
  DE: hBands('#000', '#DD0000', '#FFCE00'),
  DK: nordic('#C8102E', '#fff'),
  EE: hBands('#0072CE', '#000', '#fff'),
  ES: `<rect width="${W}" height="${H}" fill="#AA151B"/><rect y="5" width="${W}" height="10" fill="#F1BF00"/>`,
  FI: nordic('#fff', '#002F6C'),
  FR: vBands('#002654', '#fff', '#CE1126'),
  GB: union,
  GE: `<rect width="${W}" height="${H}" fill="#fff"/><path d="M14 0v20M0 10h28" stroke="#F00" stroke-width="4"/>${[[7, 5], [21, 5], [7, 15], [21, 15]].map(([x, y]) => `<path d="M${x} ${y - 2}v4M${x - 2} ${y}h4" stroke="#F00" stroke-width="1.2"/>`).join('')}`,
  GR: `${Array.from({ length: 9 }, (_, i) => `<rect y="${(i * 20 / 9).toFixed(3)}" width="${W}" height="${(20 / 9 + 0.03).toFixed(3)}" fill="${i % 2 ? '#fff' : '#0D5EAF'}"/>`).join('')}<rect width="11.1" height="11.1" fill="#0D5EAF"/><path d="M5.55 0v11.1M0 5.55h11.1" stroke="#fff" stroke-width="2.2"/>`,
  HK: `<rect width="${W}" height="${H}" fill="#DE2910"/>${[0, 72, 144, 216, 288].map(a => `<ellipse cx="14" cy="6.6" rx="1.7" ry="3.4" fill="#fff" transform="rotate(${a} 14 10)"/>`).join('')}`,
  HR: `${hBands('#F00', '#fff', '#171796')}<rect x="11" y="5.5" width="6" height="8" rx="1" fill="#F00"/><rect x="13" y="7.4" width="2" height="2" fill="#fff"/>`,
  HU: hBands('#CE2939', '#fff', '#477050'),
  ID: hBands('#F00', '#fff'),
  IE: vBands('#169B62', '#fff', '#FF883E'),
  IL: `<rect width="${W}" height="${H}" fill="#fff"/><rect y="2.5" width="${W}" height="2.6" fill="#0038B8"/><rect y="14.9" width="${W}" height="2.6" fill="#0038B8"/><path d="M14 6.4l3.2 5.4h-6.4zM14 13.6l-3.2-5.4h6.4z" fill="none" stroke="#0038B8" stroke-width=".9"/>`,
  IN: `${hBands('#FF671F', '#fff', '#046A38')}<circle cx="14" cy="10" r="2.5" fill="none" stroke="#06038D" stroke-width=".8"/>`,
  IS: nordic('#02529C', '#fff', '#DC1E35'),
  IT: vBands('#008C45', '#F4F5F0', '#CD212A'),
  JP: `<rect width="${W}" height="${H}" fill="#fff"/>${disc(14, 10, 6, '#BC002D')}`,
  KG: `<rect width="${W}" height="${H}" fill="#E8112D"/>${disc(14, 10, 4.6, '#FFEF00')}${disc(14, 10, 2.3, '#E8112D')}`,
  KR: `<rect width="${W}" height="${H}" fill="#fff"/><path d="M9.9 7.1a5 5 0 0 1 8.2 5.8 2.5 2.5 0 0 0-4.1-2.9 2.5 2.5 0 0 1-4.1-2.9z" fill="#CD2E3A"/><path d="M9.9 7.1a5 5 0 0 0 8.2 5.8 2.5 2.5 0 0 0-4.1-2.9 2.5 2.5 0 0 1-4.1-2.9z" fill="#0047A0"/><path d="M3 5.5l3-3M22 2.5l3 3M3 14.5l3 3M22 17.5l3-3" stroke="#000" stroke-width="1.4"/>`,
  KZ: `<rect width="${W}" height="${H}" fill="#00AFCA"/>${disc(14, 9, 3.6, '#FEC50C')}<path d="M8.5 14c3.5 2 7.5 2 11 0" stroke="#FEC50C" stroke-width="1.2" fill="none"/>`,
  LT: hBands('#FDB913', '#006A44', '#C1272D'),
  LU: hBands('#EA141D', '#fff', '#51ADDA'),
  LV: `<rect width="${W}" height="${H}" fill="#9E3039"/><rect y="8" width="${W}" height="4" fill="#fff"/>`,
  MD: `${vBands('#0046AE', '#FFD200', '#CC092F')}<rect x="12" y="7" width="4" height="6" rx="1" fill="#8A6A2C"/>`,
  MX: `${vBands('#006847', '#fff', '#CE1126')}${disc(14, 10, 2.2, '#8A6A2C')}`,
  NL: hBands('#AE1C28', '#fff', '#21468B'),
  NO: nordic('#BA0C2F', '#fff', '#00205B'),
  PL: hBands('#fff', '#DC143C'),
  PT: `<rect width="${W}" height="${H}" fill="#F00"/><rect width="11" height="${H}" fill="#060"/>${disc(11, 10, 3.6, '#FF0')}${disc(11, 10, 1.9, '#F00')}`,
  RO: vBands('#002B7F', '#FCD116', '#CE1126'),
  RS: `${hBands('#C6363C', '#0C4076', '#fff')}<rect x="6.5" y="5.5" width="5" height="7" rx="1.2" fill="#fff"/>`,
  RU: hBands('#fff', '#0039A6', '#D52B1E'),
  SE: nordic('#006AA7', '#FECC02'),
  SG: `${hBands('#EF3340', '#fff')}${disc(6.5, 5, 3.2, '#fff')}${disc(7.8, 5, 3, '#EF3340')}${stars([[10, 3], [12, 4.4], [11.2, 6.6], [8.9, 6.6], [8.2, 4.4]], '#fff', 0.5)}`,
  SI: `${hBands('#fff', '#005DA4', '#ED1C24')}<path d="M5 3.5h5v4.3c0 2-2.5 3-2.5 3S5 9.8 5 7.8z" fill="#005DA4"/>`,
  SK: `${hBands('#fff', '#0B4EA2', '#EE1C25')}<path d="M5 5h7v6c0 2.4-3.5 4-3.5 4S5 13.4 5 11z" fill="#EE1C25" stroke="#fff" stroke-width=".7"/><path d="M8.5 6.8v6M6.8 8.6h3.4" stroke="#fff" stroke-width=".9"/>`,
  TH: `<rect width="${W}" height="${H}" fill="#A51931"/><rect y="3.4" width="${W}" height="13.2" fill="#F4F5F8"/><rect y="6.6" width="${W}" height="6.8" fill="#2D2A4A"/>`,
  TR: `<rect width="${W}" height="${H}" fill="#E30A17"/>${disc(11, 10, 5, '#fff')}${disc(12.3, 10, 4, '#E30A17')}<path d="M18 7.6l.7 1.8 1.9.1-1.5 1.2.5 1.9-1.6-1.1-1.6 1.1.5-1.9-1.5-1.2 1.9-.1z" fill="#fff"/>`,
  UA: hBands('#0057B7', '#FFD700'),
  US: `${usStripes}<rect width="12" height="10.8" fill="#3C3B6E"/>${stars(usStars, '#fff', 0.42)}`,
  UZ: `<rect width="${W}" height="${H}" fill="#fff"/><rect width="${W}" height="6.2" fill="#0099B5"/><rect y="13.8" width="${W}" height="6.2" fill="#1EB53A"/><rect y="6.2" width="${W}" height=".7" fill="#CE1126"/><rect y="13.1" width="${W}" height=".7" fill="#CE1126"/>${disc(4.5, 3.1, 2, '#fff')}${disc(5.3, 3.1, 1.8, '#0099B5')}`,
  VN: `<rect width="${W}" height="${H}" fill="#DA251D"/><path d="M14 4.5l1.6 4.2 4.4.2-3.5 2.7 1.2 4.3L14 13.4l-3.7 2.5 1.2-4.3L8 8.9l4.4-.2z" fill="#FF0"/>`,
  ZA: `<rect width="${W}" height="${H}" fill="#fff"/><rect width="${W}" height="6.7" fill="#E03C31"/><rect y="13.3" width="${W}" height="6.7" fill="#001489"/><path d="M0 0l13 10L0 20" stroke="#fff" stroke-width="8" fill="none"/><path d="M0 2l13 8-13 8M13 10h15" stroke="#007749" stroke-width="4" fill="none"/><path d="M0 4.5L8 10 0 15.5z" fill="#000"/>`,
};

const drawing = inner => `<svg class="flag-img" viewBox="0 0 ${W} ${H}" aria-hidden="true" focusable="false">${inner}</svg>`;
const symbol = path => `<svg class="flag-img neutral" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="${path}"/></svg>`;

const GLOBE = 'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm6.9 6h-2.9a15.6 15.6 0 0 0-1.4-3.6A8 8 0 0 1 18.9 8zM12 4c.8 1.2 1.5 2.5 1.9 4h-3.8c.4-1.5 1.1-2.8 1.9-4zM4.3 14a8 8 0 0 1 0-4h3.4a16.5 16.5 0 0 0 0 4zm.8 2H8a15.6 15.6 0 0 0 1.4 3.6A8 8 0 0 1 5.1 16zM8 8H5.1a8 8 0 0 1 4.3-3.6A15.6 15.6 0 0 0 8 8zm4 12c-.8-1.2-1.5-2.5-1.9-4h3.8c-.4 1.5-1.1 2.8-1.9 4zm2.3-6H9.7a14.6 14.6 0 0 1 0-4h4.6a14.6 14.6 0 0 1 0 4zm.3 5.6a15.6 15.6 0 0 0 1.4-3.6h2.9a8 8 0 0 1-4.3 3.6zm1.7-5.6a16.5 16.5 0 0 0 0-4h3.4a8 8 0 0 1 0 4z';
export const autoIcon = symbol('M13 2L4 14h6l-1 8 9-12h-6z');
export const warningIcon = symbol('M12 3L1.5 21h21zm-1 7h2v5h-2zm0 6.5h2v2h-2z');
export const neutralFlag = symbol(GLOBE);

/** A known country's drawing; anything else the neutral globe, never letters. */
export function flagImage(code) {
  const c = (code || '').toUpperCase();
  return Object.hasOwn(ART, c) ? drawing(ART[c]) : neutralFlag;
}
export const knownFlags = Object.keys(ART);
