import { fromRequest, groupedLayout, type LayoutRequest } from './grouped-layout.ts';

self.onmessage = (e: MessageEvent<{ run: number; input: LayoutRequest }>) => {
  const { run, input } = e.data;
  self.postMessage({ run, result: groupedLayout(fromRequest(input)) });
};
