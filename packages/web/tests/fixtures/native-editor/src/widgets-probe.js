// Native WebKit acceptance; synthetic events first, then optional real OS input.
try {
    await dioxus.recv();
    const wait = async (condition, message) => {
        for (let n=0;n<60;n++) { if(condition())return;await new Promise(resolve=>setTimeout(resolve,100)); }
        throw Error(message);
    };
    await wait(()=>document.querySelector('.primary .mk-editor-core-input-sink'),'editor mount');
    document.getElementById('layout-document').click();
    document.getElementById('layout-widget').click();
    await wait(()=>document.querySelector('.primary .mk-preview-widget'),'widget mount');
    const panel=document.querySelector('.primary');
    Object.assign(panel.style,{position:'fixed',left:'0',top:'0',width:'700px',height:'450px',zIndex:'1000'});
    const canonical=()=>document.querySelector('.canonical').textContent;
    const original=canonical();
    const widget=()=>panel.querySelector('.mk-preview-widget');
    const toggle=()=>widget().querySelector('.mk-preview-widget-toggle');
    const note=()=>widget().querySelector('.mk-preview-widget-note');
    const offset=()=>panel.querySelector('.mk-native-surface').dataset.cursorOffset;
    const aligned=()=>Math.abs(panel.querySelector('.mk-native-block-widget').getBoundingClientRect().bottom-panel.querySelector('[data-line="1"]').getBoundingClientRect().top)<1;
    await wait(()=>panel.querySelector('[data-line="1"]')?.textContent.includes('// row 1')&&aligned(),'canonical source rows and collapsed block measurement');
    const caret=offset();
    const height=panel.querySelector('.mk-native-block-widget').getBoundingClientRect().height;
    toggle().click();await wait(()=>!note().closest('[hidden]'),'expanded content');
    await wait(()=>aligned()&&panel.querySelector('.mk-native-block-widget').getBoundingClientRect().height>height,'expanded block measurement');
    note().focus();note().value='local 😀中';note().dispatchEvent(new Event('input',{bubbles:true}));
    await wait(()=>widget().querySelector('.mk-preview-widget-note-value').textContent==='local 😀中','widget input');
    for(const key of ['ArrowDown','Home','End','x'])note().dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true}));
    note().dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',isComposing:true,bubbles:true,cancelable:true}));
    await new Promise(resolve=>setTimeout(resolve,100));
    if(canonical()!==original||offset()!==caret||document.activeElement!==note())throw Error('widget input escaped into source');
    note().dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
    await wait(()=>document.activeElement===panel.querySelector('.mk-editor-core-input-sink'),'Escape source focus');
    const anchor=panel.querySelector('.mk-native-block-widget').dataset.sourceAnchor;
    await wait(()=>offset()===anchor,'source anchor');
    toggle().click();await wait(()=>note().closest('[hidden]'),'collapse');
    toggle().click();await wait(()=>!note().closest('[hidden]')&&aligned(),'toggle state/height');
    if(note().value!=='local 😀中'||canonical()!==original)throw Error('toggle changed content or source');
    note().value='';note().dispatchEvent(new Event('input',{bubbles:true}));await wait(()=>widget().querySelector('.mk-preview-widget-note-value').textContent==='','reset local note');
    const rect=el=>{const r=el.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};};
    const keys=[];
    const state=()=>({text:canonical(),offset:offset(),anchor:panel.querySelector('.mk-native-block-widget').dataset.sourceAnchor,
        keys:keys.slice(-12),note:note().value,noteFocused:document.activeElement===note(),sourceFocused:document.activeElement===panel.querySelector('.mk-editor-core-input-sink'),
        noteRect:rect(note()),sourceRect:rect(widget().querySelector('.mk-preview-widget-source')),aligned:aligned()});
    dioxus.send({ok:true,text:original,geometry:rect(panel.querySelector('[data-line="1"] .mk-editor-core-cell')),note:'Native widget focus, source isolation, expand/resize and Escape passed.'});
    let pending=false;
    const publish=()=>{if(pending)return;pending=true;requestAnimationFrame(()=>{pending=false;dioxus.send({state:state()});});};
    new MutationObserver(publish).observe(panel,{subtree:true,attributes:true,childList:true,characterData:true});
    panel.addEventListener('keydown',event=>{keys.push({key:event.key,composing:event.isComposing,trusted:event.isTrusted});publish();},true);
    for(const type of ['input','keyup','focusin','focusout'])panel.addEventListener(type,publish);
    publish();
} catch(error) {dioxus.send({ok:false,error:String(error)});}
