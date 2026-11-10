import init, { Engine } from './engine/lookroom_browser.js';
let engine;
const ready = (async () => { await init(); engine = new Engine(); return JSON.parse(engine.catalogue()); })();
self.onmessage = async ({data: m}) => {
  try {
    const catalogue = await ready;
    if (m.type === 'catalogue') return self.postMessage({id:m.id,result:catalogue});
    if (m.type === 'sample') { const rgba = engine.sample(m.w,m.h); return self.postMessage({id:m.id,result:rgba},[rgba.buffer]); }
    if (m.type !== 'render') throw new Error('Unknown render request');
    const frames = m.frames.map(f => {
      let rgba = f.rgba;
      if (f.grade && Object.keys(f.grade).length) rgba = engine.grade(rgba,m.w,m.h,JSON.stringify(f.grade),f.frame || 0);
      if (f.effect) rgba = engine.effect(rgba,m.w,m.h,f.effect,f.amount ?? 1,f.progress || 0,f.frame || 0);
      return rgba;
    });
    let result = frames[0];
    if (frames.length === 2) result = engine.transition(frames[0],frames[1],m.w,m.h,m.transition,m.t);
    self.postMessage({id:m.id,result},[result.buffer]);
  } catch (e) { self.postMessage({id:m.id,error:String(e.message || e)}); }
};
