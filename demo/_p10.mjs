import { chromium } from "playwright";
const exe = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const b = await chromium.launch({ executablePath: exe });
const p = await b.newPage({ viewport: { width: 1600, height: 1000 } });
await p.goto("http://localhost:5199/demo/index.html#refund", { waitUntil: "load" });
const t0=Date.now();
const log=[];
let lastShot=0, n=0;
for (let i=0;i<320;i++){
  const r = await p.evaluate(()=>{
    const c=document.querySelector(".film-content");
    const term=document.querySelector(".rf-term");
    const inp=document.querySelector(".term-inputbox .term-input");
    const m=c?new DOMMatrix(getComputedStyle(c).transform):null;
    const lines=document.querySelectorAll(".term-tool, .term-say").length;
    const submitted=!!document.querySelector(".term-sent");
    return {s:m?+m.a.toFixed(3):0, tx:m?Math.round(m.e):0, ty:m?Math.round(m.f):0,
      termW:term?Math.round(term.getBoundingClientRect().width):0,
      chars:inp?inp.textContent.length:-1, lines, submitted};
  });
  const t=Date.now()-t0;
  log.push({t,...r});
  // screenshot the window from just-after-submit through the first couple tool lines
  if (r.submitted && r.lines<=2 && t-lastShot>250){ await p.screenshot({path:`/tmp/seq-${String(n++).padStart(2,'0')}.png`}); lastShot=t; }
  await p.waitForTimeout(45);
}
// typing-phase termW check
console.log("== typing termW (chars>0, pre-submit) ==");
const typ=log.filter(x=>x.chars>0 && !x.submitted);
const tw=[...new Set(typ.map(x=>x.termW))];
console.log("distinct termW during typing:", tw.join(", "), "| chars range:", Math.min(...typ.map(x=>x.chars)),"-",Math.max(...typ.map(x=>x.chars)));
console.log("\n== transform timeline submit→first tools ==");
let prev=null;
for (const x of log){ if(x.submitted){ const k=`${x.s}|${x.tx}|${x.ty}|${x.lines}`; if(k!==prev){console.log(`${String(x.t).padStart(6)}ms s=${x.s} tx=${x.tx} ty=${x.ty} outlines=${x.lines}`); prev=k;} } }
console.log("\nshots:", n);
await b.close();
