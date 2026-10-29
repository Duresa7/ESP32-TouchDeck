import clsx from 'clsx';
import type { Orientation } from '../companion/api.ts';
import { geometry, screenSize } from '../lib/deck/geometry.ts';
import { buildDots, buildPage, buildTitle, type TextView } from '../lib/deck/scene.ts';
import type { Preview } from './home-model.ts';

function SceneText({ view }: { view: TextView }) {
  return <g transform={`translate(${view.x} ${view.y})`} opacity={view.opacity / 255}>
    {view.label.lines.map((line, index) => <text key={index} x={line.x} y={line.y} dominantBaseline="hanging" fontSize={view.size}>
      {line.letters.map((letter) => String.fromCodePoint(letter.letter)).join('')}
    </text>)}
  </g>;
}

export function DevicePreview({ preview, live, orientation = 'landscape' }: { preview: Preview | null; live: boolean; orientation?: Orientation }) {
  const g = geometry(screenSize(preview?.layout.orientation ?? orientation), preview?.layout.fullScreen);
  const page = preview && buildPage(preview.layout.pages[preview.index], g);
  const title = page && buildTitle(page.title, g);
  return <span className={clsx('device-object', !live && 'is-off')} aria-hidden="true">
    {live && <span className="device-spill" />}
    <span className="device-bezel">
      <span className="device-screen" style={{ aspectRatio: `${g.screen.w} / ${g.screen.h}` }}>
        <svg className="device-scene" viewBox={`0 0 ${g.screen.w} ${g.screen.h}`} data-page={page?.id}>
          {title && <g className="preview-title"><SceneText view={title} /></g>}
          {page?.tiles.map((tile) => <g key={tile.id} transform={`translate(${tile.rect.x} ${tile.rect.y})`}>
            <rect width={tile.rect.w} height={tile.rect.h} rx={18} className="preview-tile" />
            {tile.texts.map((view, i) => <SceneText key={i} view={view} />)}
            {tile.bar && <rect {...{ x: tile.bar.x, y: tile.bar.y, width: tile.bar.w, height: tile.bar.h, rx: tile.bar.radius }} className="preview-active" />}
          </g>)}
          {preview && buildDots(g, preview.layout.pages.length, preview.index).map((dot, i) => <rect key={i} x={dot.x} y={dot.y} width={dot.w} height={dot.h} rx={dot.h / 2} className={dot.active ? 'preview-active' : 'preview-idle'} />)}
        </svg>
      </span>
    </span>
  </span>;
}
