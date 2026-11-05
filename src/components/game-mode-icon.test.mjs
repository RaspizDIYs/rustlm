import { test } from "node:test";
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import ts from "typescript";
const source=readFileSync(new URL('./game-mode-icon.tsx',import.meta.url),'utf8');
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX}}).outputText;
const module={exports:{}};
vm.runInNewContext(compiled,{module,exports:module.exports,require(name){if(name==='react/jsx-runtime')return {jsx:(type,props)=>({type,props})};if(name==='@/lib/utils')return {cn:(...parts)=>parts.filter(Boolean).join(' ')};throw Error(name);}});
const icon=module.exports.GameModeIcon;
test('mode and lobby queue icons use exactly the user-selected artwork',()=>{
 for(const mode of ['classic',400,420,440,480,490])assert.equal(icon({mode}).props.src,'/images/game-modes/summoners-rift.png');
 for(const mode of ['arena',1700,1710,1750])assert.equal(icon({mode}).props.src,'/images/game-modes/nexus-blitz.png');
 for(const mode of ['aram','mayhem',450,2400])assert.equal(icon({mode}).props.src,'/images/game-modes/howling-abyss.png');
 assert.equal(icon({mode:9999}),null);
});
test('icons are bundled PNGs and decorative labels do not replace accessible mode names',()=>{
 for(const mode of ['classic','arena','aram']){const img=icon({mode});assert.equal(img.props.alt,'');assert.equal(img.props['aria-hidden'],'true');const file=new URL('../../public'+img.props.src,import.meta.url);assert.equal(readFileSync(file).subarray(0,8).toString('hex'),'89504e470d0a1a0a');}
});
