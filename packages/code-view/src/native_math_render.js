// TeX presentation only. All Markdown/source identity belongs to Rust.
const request = await dioxus.recv();
try {
    const key = Symbol.for('moonkale.math.loader');
    if (!window[key]) {
        window[key] = new Promise((resolve,reject) => {
            if (window.moonkale?.math) {resolve();return;}
            const script=document.createElement('script');script.src=request.url;
            const timeout=setTimeout(()=>reject(Error('timeout')),15000);
            script.onload=()=>{clearTimeout(timeout);resolve();};
            script.onerror=()=>{clearTimeout(timeout);reject(Error('load'));};
            document.head.appendChild(script);
        }).catch(error=>{delete window[key];throw error;});
    }
    await window[key];
    dioxus.send(window.moonkale.math.render(request));
} catch {dioxus.send({html:null,error:true});}
