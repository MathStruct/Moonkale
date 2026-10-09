try {
    await dioxus.recv();
    const wait=async(condition,message)=>{for(let n=0;n<80;n++){if(condition())return;await new Promise(resolve=>setTimeout(resolve,100));}throw Error(message);};
    await wait(()=>document.getElementById('markdown-preview'),'fixture mount');
    document.getElementById('markdown-preview').click();
    await wait(()=>document.querySelectorAll('.language .mk-native-inline-widget').length===3,'Markdown preview');
    const panel=document.querySelector('.language');
    Object.assign(panel.style,{position:'fixed',left:'0',top:'0',width:'550px',height:'650px',zIndex:'1000'});
    const widgets=()=>[...panel.querySelectorAll('.mk-native-inline-widget')];
    const ready=()=>[...panel.querySelectorAll('.mk-proportional-run')].every(run=>run.dataset.ready==='true');
    await wait(ready,'measured Markdown');
    const labels=widgets().map(el=>el.textContent);
    if(JSON.stringify(labels)!==JSON.stringify(['猫 & dog','bold 😀','code <safe>']))throw Error('parsed labels');
    if(getComputedStyle(widgets()[0]).fontStyle!=='italic'||Number(getComputedStyle(widgets()[1]).fontWeight)<700)throw Error('Markdown styles');
    const canonical=()=>document.querySelector('.language-canonical').textContent;
    const original=canonical();
    const rect=el=>{const r=el.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};};
    const offset=()=>Number(panel.querySelector('.mk-native-surface').dataset.cursorOffset);
    const state=()=>({text:canonical(),offset:offset(),count:widgets().length,ready:ready(),rect:widgets()[0]?rect(widgets()[0]):null});
    dioxus.send({ok:true,text:original,geometry:rect(widgets()[0])});
    let pending=false;
    const publish=()=>{if(pending)return;pending=true;requestAnimationFrame(()=>{pending=false;dioxus.send({state:state()});});};
    new MutationObserver(publish).observe(panel,{subtree:true,attributes:true,childList:true,characterData:true});
    new MutationObserver(publish).observe(document.querySelector('.language-canonical'),{subtree:true,childList:true,characterData:true});
    for(const type of ['input','keyup','focusin'])panel.addEventListener(type,publish);
    publish();
} catch(error){dioxus.send({ok:false,error:String(error)});}
