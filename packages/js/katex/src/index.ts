import katex from 'katex';
type Request = {tex:string;display:boolean;macros:Record<string,string>};
export function render(request:Request):{html:string|null;error:boolean} {
  try {
    if(typeof request.tex!=='string'||new TextEncoder().encode(request.tex).length>16384)throw Error('size');
    // A fresh object isolates \gdef and global definitions between calls.
    const macros=Object.assign(Object.create(null),JSON.parse(JSON.stringify(request.macros)));
    const html=katex.renderToString(request.tex,{displayMode:request.display,output:'htmlAndMathml',throwOnError:true,trust:false,maxExpand:1000,maxSize:20,macros,strict:'ignore'});
    if(new TextEncoder().encode(html).length>1048576)throw Error('size');
    return {html,error:false};
  }catch {return {html:null,error:true};}
}
const host=window as typeof window & {moonkale?:{math?:{render:typeof render}}};
host.moonkale??={};host.moonkale.math={render};
