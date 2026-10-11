import { ImageResponse } from 'next/og';

export const size = { width: 180, height: 180 };
export const contentType = 'image/png';

/** The favicon's four tiles on the site background, for home screens (iOS fills transparency with black). */
export default function AppleIcon() {
  const tile = (left: number, top: number, w: number, h: number, background: string) => (
    <div style={{ position: 'absolute', left: 30 + left * 3.75, top: 30 + top * 3.75, width: w * 3.75, height: h * 3.75, borderRadius: 11.25, background }} />
  );
  return new ImageResponse(
    (
      <div style={{ display: 'flex', width: '100%', height: '100%', background: '#EDEFEA' }}>
        {tile(2, 2, 16, 16, '#2B44D9')}
        {tile(21, 2, 9, 9, '#F4B13A')}
        {tile(21, 14, 9, 16, '#12786A')}
        {tile(2, 21, 16, 9, '#C93450')}
      </div>
    ),
    size,
  );
}
