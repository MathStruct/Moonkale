try {
    const mode=await dioxus.recv();
    const wait=async(condition,message)=>{for(let n=0;n<80;n++){if(condition())return;await new Promise(resolve=>setTimeout(resolve,100));}throw Error(message);};
    await wait(()=>document.getElementById('markdown-preview'),'fixture mount');
    document.getElementById(mode.nested?'markdown-nested':'markdown-preview').click();
    if(mode.nested){
        await wait(()=>document.querySelector('.language .mk-native-preview'),'preview toggle');
        document.querySelector('.language .mk-native-preview').click();
    }
    await wait(()=>document.querySelectorAll('.language .mk-native-inline-widget').length===3,'Markdown preview');
    const panel=document.querySelector('.language');
    Object.assign(panel.style,{position:'fixed',left:'0',top:'0',width:'550px',height:'650px',zIndex:'1000'});
    const widgets=()=>[...panel.querySelectorAll('.mk-native-inline-widget')];
    const ready=()=>[...panel.querySelectorAll('.mk-proportional-run')].every(run=>run.dataset.ready==='true');
    await wait(ready,'measured Markdown');
    // Let accepted height updates and the production settings effect settle.
    await new Promise(resolve=>setTimeout(resolve,400));
    await wait(ready,'settled Markdown layout');
    const labels=widgets().map(el=>el.textContent);
    const expected=mode.nested?['both 猫','bold italic & 猫 and code <safe> tail','plain strong then x * y end']:['猫 & dog','bold 😀','code <safe>'];
    if(JSON.stringify(labels)!==JSON.stringify(expected))throw Error('parsed labels');
    const emphasis=mode.nested?widgets()[0].querySelector('.mk-markdown-fragment'):widgets()[0];
    const strong=mode.nested?emphasis:widgets()[1];
    if(getComputedStyle(emphasis).fontStyle!=='italic'||Number(getComputedStyle(strong).fontWeight)<700)throw Error('Markdown styles');
    const canonical=()=>document.querySelector('.language-canonical').textContent;
    const original=canonical();
    const rect=el=>{const r=el.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};};
    const offset=()=>Number(panel.querySelector('.mk-native-surface').dataset.cursorOffset);
    let pointer=null;
    panel.addEventListener('mousedown',event=>{pointer={target:event.target.className,x:event.offsetX,y:event.offsetY,trusted:event.isTrusted};},true);
    const state=()=>({pointer,text:canonical(),offset:offset(),count:widgets().length,ready:ready(),rect:widgets()[0]?rect(widgets()[0]):null});
    dioxus.send({ok:true,text:original,token:mode.nested?'***both 猫***':'*猫 &amp; dog*',geometry:rect(widgets()[0])});
    let pending=false;
    const publish=()=>{if(pending)return;pending=true;requestAnimationFrame(()=>{pending=false;dioxus.send({state:state()});});};
    new MutationObserver(publish).observe(panel,{subtree:true,attributes:true,childList:true,characterData:true});
    new MutationObserver(publish).observe(document.querySelector('.language-canonical'),{subtree:true,childList:true,characterData:true});
    for(const type of ['input','keyup','focusin'])panel.addEventListener(type,publish);
    publish();
} catch(error){dioxus.send({ok:false,error:String(error)});}
