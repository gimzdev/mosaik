// Layouts in small: a preset's shape (the layouts beside Arrange, the zone editor's presets) and a
// workspace's zones (its card), drawn as tiles with a line of grout between them.

import type { MiniZone, RatioRect, ZoneColor } from './protocol';

const TILE_COLORS: readonly ZoneColor[] = ['lapis', 'verdigris', 'saffron', 'madder'];

/** Tiles in a box of `width` by `height`, `gap` apart, each in its colour (null: neutral). */
function Tiles({
  areas,
  width,
  height,
  gap,
  radius,
}: {
  areas: readonly { area: RatioRect; color: ZoneColor | null }[];
  width: number;
  height: number;
  gap: number;
  radius: number;
}) {
  return (
    <svg
      aria-hidden="true"
      width={width}
      height={height}
      viewBox={`0 0 ${String(width)} ${String(height)}`}
      className="shrink-0"
    >
      {areas.map(({ area, color }, i) => {
        const x = area.left * width + gap / 2;
        const y = area.top * height + gap / 2;
        const w = Math.max(1, (area.right - area.left) * width - gap);
        const h = Math.max(1, (area.bottom - area.top) * height - gap);
        return (
          <rect
            key={i}
            x={x}
            y={y}
            width={w}
            height={h}
            rx={radius}
            style={color ? { fill: `var(--zone-${color})` } : undefined}
            className={
              color
                ? 'forced-colors:fill-[Canvas] forced-colors:stroke-[CanvasText]'
                : 'fill-[var(--hover)] stroke-[var(--control)] forced-colors:fill-[Canvas] forced-colors:stroke-[CanvasText]'
            }
            strokeWidth={color ? 0 : 1}
          />
        );
      })}
    </svg>
  );
}

/** A layout preset's shape, its tiles in the palette's colours (`colored`) or neutral. */
export function LayoutGlyph({
  shape,
  width = 40,
  height = 24,
  colored = true,
}: {
  shape: readonly RatioRect[];
  width?: number;
  height?: number;
  colored?: boolean;
}) {
  return (
    <Tiles
      areas={shape.map((area, i) => ({ area, color: colored ? (TILE_COLORS[i % TILE_COLORS.length] ?? null) : null }))}
      width={width}
      height={height}
      gap={2.5}
      radius={2.5}
    />
  );
}

/** A workspace's zones on its first monitor, on a dark desk; an empty desk when it has none. */
export function MiniLayout({
  layout,
  width = 76,
  height = 46,
}: {
  layout: MiniZone[];
  width?: number;
  height?: number;
}) {
  return (
    <span
      className="sky grid shrink-0 place-items-center overflow-hidden rounded-[9px] p-[3px] ring-1 ring-black/30 forced-colors:border forced-colors:border-[CanvasText]"
      style={{ width, height }}
    >
      {layout.length ? (
        <Tiles
          areas={layout.map((z) => ({ area: z.area, color: z.color }))}
          width={width - 6}
          height={height - 6}
          gap={3}
          radius={3}
        />
      ) : null}
    </span>
  );
}
