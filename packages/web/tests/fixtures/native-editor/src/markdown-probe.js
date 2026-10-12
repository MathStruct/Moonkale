try {
    const mode=await dioxus.recv();
    const wait=async(condition,message)=>{for(let n=0;n<80;n++){if(condition())return;await new Promise(resolve=>setTimeout(resolve,100));}throw Error(message);};
    await wait(()=>document.getElementById('markdown-preview'),'fixture mount');
    document.getElementById(mode.fenced?'markdown-fenced':mode.setext?'markdown-setext':mode.headings?'markdown-headings':mode.nested?'markdown-nested':'markdown-preview').click();
    if(mode.nested||mode.headings||mode.setext||mode.fenced){
        await wait(()=>document.querySelector('.language .mk-native-preview'),'preview toggle');
        document.querySelector('.language .mk-native-preview').click();
    }
    if(mode.headings||mode.setext||mode.fenced){
        await wait(()=>document.querySelector('.language'),'heading panel');
        Object.assign(document.querySelector('.language').style,{position:'fixed',left:'0',top:'0',width:'550px',height:'1000px',zIndex:'1000'});
        await new Promise(resolve=>setTimeout(resolve,200));
        await wait(()=>document.querySelector('.language .mk-editor-core-input-sink'),'heading input');
        const input=document.querySelector('.language .mk-editor-core-input-sink');input.focus();
        input.dispatchEvent(new KeyboardEvent('keydown',{key:'End',ctrlKey:true,bubbles:true,cancelable:true}));
    }
    const count=mode.fenced?2:mode.setext?2:mode.headings?6:3;
    await wait(()=>document.querySelectorAll('.language .mk-native-inline-widget').length===count,'Markdown preview');
    const panel=document.querySelector('.language');
    Object.assign(panel.style,{position:'fixed',left:'0',top:'0',width:'550px',height:mode.headings||mode.fenced?'1000px':'650px',zIndex:'1000'});
    const widgets=()=>[...panel.querySelectorAll('.mk-native-inline-widget')];
    const ready=()=>[...panel.querySelectorAll('.mk-proportional-run')].every(run=>run.dataset.ready==='true');
    await wait(ready,'measured Markdown');
    // Let accepted height updates and the production settings effect settle.
    await new Promise(resolve=>setTimeout(resolve,400));
    await wait(ready,'settled Markdown layout');
    const labels=widgets().map(el=>el.textContent);
    const expected=mode.fenced?['fn main() {\n    let 猫 = \"<safe>\";\n    println!(\"{}\", 猫);\n}\n','*literal* <safe>\n\n']:mode.setext?['Title 猫 & dog','Second italic and code']:mode.headings?['Title 猫','Heading bold and code','Third','Fourth','Fifth','Sixth']:mode.nested?['both 猫','bold italic & 猫 and code <safe> tail','plain strong then x * y end']:['猫 & dog','bold 😀','code <safe>'];
    if(JSON.stringify(labels)!==JSON.stringify(expected))throw Error('parsed labels');
    const emphasis=mode.nested?widgets()[0].querySelector('.mk-markdown-fragment'):widgets()[0];
    const strong=mode.nested?emphasis:widgets()[1];
    if(mode.fenced){
        await wait(()=>new Set([...widgets()[0].querySelectorAll('.mk-markdown-code-fragment')].map(el=>getComputedStyle(el).color)).size>1,'fenced syntax theme colors');
        if(widgets()[0].dataset.codeLanguage!=='rust'||!widgets()[0].querySelector('[class*="a-"]')||widgets()[1].dataset.codeLanguage!=='unknown')throw Error('fenced code syntax');
    }else if(mode.headings||mode.setext){
        if(widgets().some((el,index)=>Number(el.dataset.headingLevel)!==index+1)||Number.parseFloat(getComputedStyle(widgets()[0]).fontSize)<=Number.parseFloat(getComputedStyle(widgets()[1]).fontSize))throw Error('heading levels/styles');
    }else if(getComputedStyle(emphasis).fontStyle!=='italic'||Number(getComputedStyle(strong).fontWeight)<700)throw Error('Markdown styles');
    const canonical=()=>document.querySelector('.language-canonical').textContent;
    const original=canonical();
    const rect=el=>{const r=el.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};};
    const offset=()=>Number(panel.querySelector('.mk-native-surface').dataset.cursorOffset);
    let pointer=null;
    panel.addEventListener('mousedown',event=>{pointer={run:{...event.target.closest('.mk-proportional-run')?.dataset},id:event.target.id,target:event.target.className,x:event.offsetX,y:event.offsetY,trusted:event.isTrusted};},true);
    const state=()=>({pointer,text:canonical(),offset:offset(),count:widgets().length,ready:ready(),rect:widgets()[0]?rect(widgets()[0]):null});
    const codeStart=original.indexOf('```rust');
    const codeEnd=original.indexOf('\r\n```',codeStart+3)+5;
    dioxus.send({ok:true,text:original,count,token:mode.fenced?original.slice(codeStart,codeEnd):mode.setext?'Title **猫** &amp; dog\r\n===':mode.headings?'# Title 猫':mode.nested?'***both 猫***':'*猫 &amp; dog*',geometry:rect(widgets()[0])});
    let pending=false;
    const publish=()=>{if(pending)return;pending=true;requestAnimationFrame(()=>{pending=false;dioxus.send({state:state()});});};
    new MutationObserver(publish).observe(panel,{subtree:true,attributes:true,childList:true,characterData:true});
    new MutationObserver(publish).observe(document.querySelector('.language-canonical'),{subtree:true,childList:true,characterData:true});
    for(const type of ['input','keyup','focusin'])panel.addEventListener(type,publish);
    publish();
} catch(error){dioxus.send({ok:false,error:String(error),styles:[...document.querySelectorAll('link[rel=stylesheet]')].map(el=>({href:el.href,loaded:!!el.sheet}))});}
