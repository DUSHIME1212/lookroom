export const FPS = 30;
export const uid = () => globalThis.crypto?.randomUUID?.() || `id-${Date.now()}-${Math.random().toString(36).slice(2)}`;
export const clamp = (v,a,b) => Math.max(a,Math.min(b,Number.isFinite(+v) ? +v : a));
export function duration(c) { return Math.max(1/FPS,(c.out-c.in)/c.speed); }
export function layout(clips) {
  let end=0;
  return clips.map((clip,i) => {
    const length=duration(clip);
    const previous=i ? duration(clips[i-1]) : 0;
    const previousLap=i>1 ? Math.min(clips[i-1].transition?.seconds || 0,duration(clips[i-2])-1/FPS,previous-1/FPS) : 0;
    const lap=i && clip.transition ? Math.max(0,Math.min(clip.transition.seconds,length-1/FPS,previous-previousLap-1/FPS)) : 0;
    const start=end-lap; end=start+length;
    return {clip,index:i,start,end,length,lap};
  });
}
export function shot(clips,time) {
  const list=layout(clips);
  if(!list.length) return null;
  const current=[...list].reverse().find(s=>s.start<=time) || list[0];
  const previous=current.index>0 && time<current.start+current.lap ? list[current.index-1] : null;
  return {current,previous,t:previous ? clamp((time-current.start)/current.lap,0,1) : 1};
}
export function split(clips,id,head) {
  const s=layout(clips).find(s=>s.clip.id===id);
  if(!s || head<=s.start+1/FPS || head>=s.end-1/FPS || head<s.start+s.lap) return null;
  const cut=s.clip.in+(head-s.start)*s.clip.speed;
  const left={...structuredClone(s.clip),out:cut};
  const right={...structuredClone(s.clip),id:uid(),in:cut,transition:null};
  return [...clips.slice(0,s.index),left,right,...clips.slice(s.index+1)];
}
export function newClip(media) {
  return {id:uid(),media:media.id,in:0,out:media.type==='image' ? 5 : media.duration,speed:1,volume:1,zoom:1,grade:{},look:'',effect:null,amount:1,transition:null};
}
export function emptyProject() { return {version:1,name:'Untitled Project',aspect:'16:9',fill:false,clips:[],audio:[],texts:[]}; }
export function validateProject(raw,media) {
  if(raw?.version!==1 || !Array.isArray(raw.clips) || raw.clips.length>500) throw new Error('This is not a supported Lookroom web project.');
  const ids=new Set(media.map(m=>m.id));
  const clipIds=new Set();
  const p=emptyProject(); p.name=String(raw.name||p.name).slice(0,80);
  p.aspect=['16:9','9:16','1:1','3:2','4:5'].includes(raw.aspect)?raw.aspect:'16:9'; p.fill=!!raw.fill;
  p.clips=raw.clips.map(c=>{
    if(!ids.has(c.media) || typeof c.id!=='string' || clipIds.has(c.id)) throw new Error('A project clip is missing its media or has an invalid ID.');
    clipIds.add(c.id);
    const m=media.find(m=>m.id===c.media);
    const max=m.type==='image'?86400:m.duration;
    const start=clamp(c.in,0,Math.max(0,max-1/FPS));
    return {...newClip(m),id:c.id,in:start,out:clamp(c.out,start+1/FPS,max),speed:clamp(c.speed,.25,4),volume:clamp(c.volume,0,1),zoom:clamp(c.zoom||1,1,3),grade:c.grade&&typeof c.grade==='object'?c.grade:{},look:String(c.look||''),effect:typeof c.effect==='string'?c.effect:null,amount:clamp(c.amount??1,0,1),transition:c.transition&&typeof c.transition.id==='string'?{id:c.transition.id,seconds:clamp(c.transition.seconds,1/FPS,3)}:null};
  });
  p.audio=(Array.isArray(raw.audio)?raw.audio:[]).slice(0,100).filter(a=>ids.has(a.media)).map(a=>({id:uid(),media:a.media,at:clamp(a.at,0,86400),volume:clamp(a.volume??1,0,1)}));
  p.texts=(Array.isArray(raw.texts)?raw.texts:[]).slice(0,100).map(t=>({id:uid(),text:String(t.text||'').slice(0,2000),at:clamp(t.at,0,86400),end:clamp(t.end,0,86400),size:clamp(t.size||.08,.02,.25),x:clamp(t.x??.5,0,1),y:clamp(t.y??.8,0,1),colour:/^#[0-9a-f]{6}$/i.test(t.colour)?t.colour:'#ffffff'}));
  return p;
}
